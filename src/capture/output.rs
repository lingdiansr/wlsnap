//! Single-output and multi-output capture orchestration.

use crate::backend::wlr;
use crate::capture::{CapturedImage, current_output};
use crate::error::{Result, WlsnapError};
use crate::platform::output_info::OutputInfo;
use crate::platform::wayland;
use image::RgbaImage;
use wayland_client::Connection;

/// Capture the current pointer output (single screen).
///
/// 1. Enumerate outputs via `wayland::enumerate_outputs()`
/// 2. Determine current output via `capture::current_output()` (pointer fallback to first)
/// 3. Connect to Wayland via `Connection::connect_to_env()`
/// 4. Call `wlr::capture_output(conn, &output, overlay_cursor)`
/// 5. Return `CapturedImage`
pub async fn capture_current_screen(overlay_cursor: bool) -> Result<CapturedImage> {
    let outputs = wayland::enumerate_outputs()?;
    let output = current_output(&outputs, None).ok_or(WlsnapError::NoOutputDetected)?;

    let conn =
        Connection::connect_to_env().map_err(|e| WlsnapError::WaylandConnect(e.to_string()))?;

    let image = wlr::capture_output(&conn, &output, overlay_cursor).await?;

    Ok(CapturedImage {
        image,
        source_output: output,
    })
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

    // Compute bounding box of all logical geometries
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for (output, _) in &captured {
        let geom = &output.logical_geometry;
        min_x = min_x.min(geom.min.x);
        min_y = min_y.min(geom.min.y);
        max_x = max_x.max(geom.max.x);
        max_y = max_y.max(geom.max.y);
    }

    let total_width = (max_x - min_x).round() as u32;
    let total_height = (max_y - min_y).round() as u32;

    if total_width == 0 || total_height == 0 {
        return Err(WlsnapError::Stitching("stitched canvas has zero size"));
    }

    // Create canvas and blit each image
    let mut canvas = RgbaImage::new(total_width, total_height);

    for (output, image) in &captured {
        let geom = &output.logical_geometry;
        let offset_x = (geom.min.x - min_x).round() as i64;
        let offset_y = (geom.min.y - min_y).round() as i64;

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
            min: crate::platform::output_info::LogicalPoint { x: min_x, y: min_y },
            max: crate::platform::output_info::LogicalPoint { x: max_x, y: max_y },
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
}
