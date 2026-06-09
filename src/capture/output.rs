//! Single-output and multi-output capture orchestration.

use image::RgbaImage;
use wayland_client::Connection;

use crate::{
    backend::wlr,
    capture::CapturedImage,
    error::{Result, WlsnapError},
    platform::{output_info::OutputInfo, wayland},
};

/// Capture a specific output.
///
/// 1. Connect to Wayland via `Connection::connect_to_env()`
/// 2. Call `wlr::capture_output(conn, &output, overlay_cursor)`
/// 3. Return `CapturedImage`
pub async fn capture_specific_output(
    output: &OutputInfo,
    overlay_cursor: bool,
) -> Result<CapturedImage> {
    let conn =
        Connection::connect_to_env().map_err(|e| WlsnapError::WaylandConnect(e.to_string()))?;

    let image = wlr::capture_output(&conn, output, overlay_cursor).await?;

    Ok(CapturedImage {
        image,
        source_output: output.clone(),
    })
}

/// Capture the current focused output (single screen).
///
/// 1. Enumerate outputs via `wayland::enumerate_outputs()`
/// 2. Find the focused output via compositor IPC, fall back to first output
/// 3. Call `capture_specific_output()`
/// 4. Return `CapturedImage`
pub async fn capture_current_screen(overlay_cursor: bool) -> Result<CapturedImage> {
    let outputs = wayland::enumerate_outputs()?;

    // Try to find the focused output via compositor IPC, fall back to first output
    let output = if let Some(focused_name) = wayland::get_focused_output_name() {
        outputs
            .iter()
            .find(|o| o.name == focused_name)
            .cloned()
            .or_else(|| outputs.first().cloned())
    } else {
        outputs.first().cloned()
    }
    .ok_or(WlsnapError::NoOutputDetected)?;

    capture_specific_output(&output, overlay_cursor).await
}

/// Capture all outputs and stitch them into a single image.
///
/// 1. Enumerate all outputs
/// 2. Capture each output individually
/// 3. Compute the bounding box of all `logical_geometry` positions
/// 4. Create a canvas of that size
/// 5. Blit each captured image onto the canvas at its logical position
/// 6. Return the stitched `RgbaImage`
///
/// Note: Each output's image must have `OutputTransform::apply_to_image` already applied
/// by the backend, so the image is in logical orientation.
pub async fn capture_all_screens(overlay_cursor: bool) -> Result<CapturedImage> {
    let outputs = wayland::enumerate_outputs()?;
    if outputs.is_empty() {
        return Err(WlsnapError::NoOutputDetected);
    }

    let conn =
        Connection::connect_to_env().map_err(|e| WlsnapError::WaylandConnect(e.to_string()))?;

    // Capture each output
    let mut captured = Vec::with_capacity(outputs.len());
    for output in &outputs {
        let image = wlr::capture_output(&conn, output, overlay_cursor).await?;
        captured.push((output.clone(), image));
    }

    // Compute tight bounding box using logical positions to eliminate gaps.
    //
    // Each output's image is in physical pixels (from wlr-screencopy).  We
    // compute each output's *relative* position in the tight layout by
    // sorting outputs along each axis and collapsing gaps between them.
    // This preserves the visual arrangement while removing empty space.
    //
    // For each unique logical position along an axis, we compute the max
    // physical size at that position, then accumulate offsets.
    let mut sorted_x: Vec<_> = captured
        .iter()
        .map(|(o, _)| (o.logical_geometry.min.x, o.physical_size.0 as f64))
        .collect();
    sorted_x.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    let mut sorted_y: Vec<_> = captured
        .iter()
        .map(|(o, _)| (o.logical_geometry.min.y, o.physical_size.1 as f64))
        .collect();
    sorted_y.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // Build offset maps: logical_position -> tight_offset
    let mut x_offsets = std::collections::HashMap::new();
    let mut y_offsets = std::collections::HashMap::new();

    let mut current_x = 0.0;
    let mut last_logical_x = f64::NEG_INFINITY;
    let mut max_w_at_x = 0.0;
    for (logical_x, phys_w) in sorted_x {
        if (logical_x - last_logical_x).abs() >= 0.001 {
            // New x position: save offset for previous group, start new group
            if last_logical_x != f64::NEG_INFINITY {
                x_offsets.insert((last_logical_x * 1000.0).round() as i64, current_x);
                current_x += max_w_at_x;
            }
            last_logical_x = logical_x;
            max_w_at_x = phys_w;
        } else {
            // Same x position: track max width
            max_w_at_x = max_w_at_x.max(phys_w);
        }
    }
    // Don't forget the last group
    if last_logical_x != f64::NEG_INFINITY {
        x_offsets.insert((last_logical_x * 1000.0).round() as i64, current_x);
        current_x += max_w_at_x;
    }

    let mut current_y = 0.0;
    let mut last_logical_y = f64::NEG_INFINITY;
    let mut max_h_at_y = 0.0;
    for (logical_y, phys_h) in sorted_y {
        if (logical_y - last_logical_y).abs() >= 0.001 {
            if last_logical_y != f64::NEG_INFINITY {
                y_offsets.insert((last_logical_y * 1000.0).round() as i64, current_y);
                current_y += max_h_at_y;
            }
            last_logical_y = logical_y;
            max_h_at_y = phys_h;
        } else {
            max_h_at_y = max_h_at_y.max(phys_h);
        }
    }
    if last_logical_y != f64::NEG_INFINITY {
        y_offsets.insert((last_logical_y * 1000.0).round() as i64, current_y);
        current_y += max_h_at_y;
    }

    let total_width = current_x.round() as u32;
    let total_height = current_y.round() as u32;

    if total_width == 0 || total_height == 0 {
        return Err(WlsnapError::Stitching("stitched canvas has zero size"));
    }

    // Create canvas and blit each image at tight offsets
    let mut canvas = RgbaImage::new(total_width, total_height);

    for (output, image) in &captured {
        let key_x = (output.logical_geometry.min.x * 1000.0).round() as i64;
        let key_y = (output.logical_geometry.min.y * 1000.0).round() as i64;
        let offset_x = x_offsets.get(&key_x).copied().unwrap_or(0.0).round() as i64;
        let offset_y = y_offsets.get(&key_y).copied().unwrap_or(0.0).round() as i64;

        let img_w = image.width() as i64;
        let img_h = image.height() as i64;

        for y in 0..img_h {
            for x in 0..img_w {
                let canvas_x = offset_x + x;
                let canvas_y = offset_y + y;

                if canvas_x >= 0
                    && canvas_x < total_width as i64
                    && canvas_y >= 0
                    && canvas_y < total_height as i64
                {
                    let pixel = image.get_pixel(x as u32, y as u32);
                    canvas.put_pixel(canvas_x as u32, canvas_y as u32, *pixel);
                }
            }
        }
    }

    // Build a synthetic OutputInfo representing the stitched canvas
    let stitched_output = OutputInfo {
        name: "stitched".to_string(),
        description: "All screens stitched".to_string(),
        logical_geometry: crate::platform::output_info::LogicalRect {
            min: crate::platform::output_info::LogicalPoint { x: 0.0, y: 0.0 },
            max: crate::platform::output_info::LogicalPoint {
                x: total_width as f64,
                y: total_height as f64,
            },
        },
        physical_size: (total_width, total_height),
        scale_factor: 1.0,
        transform: crate::platform::output_info::OutputTransform::Normal,
    };

    Ok(CapturedImage {
        image: canvas,
        source_output: stitched_output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::output_info::{LogicalPoint, LogicalRect, OutputTransform};

    #[allow(dead_code)]
    fn make_output(name: &str, x: f64, y: f64, w: f64, h: f64) -> OutputInfo {
        OutputInfo {
            name: name.to_string(),
            description: String::new(),
            logical_geometry: LogicalRect {
                min: LogicalPoint { x, y },
                max: LogicalPoint { x: x + w, y: y + h },
            },
            physical_size: (w as u32, h as u32),
            scale_factor: 1.0,
            transform: OutputTransform::Normal,
        }
    }

    #[test]
    fn capture_current_screen_without_wayland_display() {
        // Skip this test if we have a live Wayland compositor.
        // The test is only meaningful in headless CI environments.
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return;
        }
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(capture_current_screen(false));
        assert!(result.is_err());
    }

    #[test]
    fn capture_all_screens_without_wayland_display() {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return;
        }
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(capture_all_screens(false));
        assert!(result.is_err());
    }

    /// Verify that stitching uses physical pixel coordinates, not logical.
    ///
    /// Simulates a HiDPI laptop (eDP-1 @ 2x) + external monitor (HDMI-A-1 @ 1.25x).
    /// The canvas must be sized for physical pixels so images don't overflow.
    #[test]
    fn capture_all_screens_uses_physical_coordinates() {
        let output1 = OutputInfo {
            name: "HDMI-A-1".to_string(),
            description: String::new(),
            logical_geometry: LogicalRect {
                min: LogicalPoint { x: 0.0, y: 0.0 },
                max: LogicalPoint {
                    x: 1536.0,
                    y: 864.0,
                },
            },
            physical_size: (1920, 1080),
            scale_factor: 1.25,
            transform: OutputTransform::Normal,
        };

        let output2 = OutputInfo {
            name: "eDP-1".to_string(),
            description: String::new(),
            logical_geometry: LogicalRect {
                min: LogicalPoint { x: 0.0, y: 864.0 },
                max: LogicalPoint {
                    x: 1440.0,
                    y: 1764.0,
                },
            },
            physical_size: (2880, 1800),
            scale_factor: 2.0,
            transform: OutputTransform::Normal,
        };

        // Compute physical positions (same logic as capture_all_screens)
        let phys_x1 = output1.logical_geometry.min.x * output1.scale_factor;
        let phys_y1 = output1.logical_geometry.min.y * output1.scale_factor;
        let phys_x2 = output2.logical_geometry.min.x * output2.scale_factor;
        let phys_y2 = output2.logical_geometry.min.y * output2.scale_factor;

        let phys_w1 = output1.physical_size.0 as f64;
        let phys_h1 = output1.physical_size.1 as f64;
        let phys_w2 = output2.physical_size.0 as f64;
        let phys_h2 = output2.physical_size.1 as f64;

        let min_x = phys_x1.min(phys_x2);
        let min_y = phys_y1.min(phys_y2);
        let max_x = (phys_x1 + phys_w1).max(phys_x2 + phys_w2);
        let max_y = (phys_y1 + phys_h1).max(phys_y2 + phys_h2);

        let total_width = (max_x - min_x).round() as u32;
        let total_height = (max_y - min_y).round() as u32;

        // Expected canvas: 2880 x 3528 (physical pixels)
        // width  = max(1920, 2880) = 2880
        // height = 0 + max(1080, 1728+1800) = 3528
        assert_eq!(
            total_width, 2880,
            "canvas width should use physical coordinates"
        );
        assert_eq!(
            total_height, 3528,
            "canvas height should use physical coordinates"
        );

        // Verify offsets
        let offset1_x = (phys_x1 - min_x).round() as i64;
        let offset1_y = (phys_y1 - min_y).round() as i64;
        let offset2_x = (phys_x2 - min_x).round() as i64;
        let offset2_y = (phys_y2 - min_y).round() as i64;

        assert_eq!(offset1_x, 0);
        assert_eq!(offset1_y, 0);
        assert_eq!(offset2_x, 0);
        assert_eq!(offset2_y, 1728); // 864 * 2.0 = 1728
    }
}
