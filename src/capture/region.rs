//! Region selection parameter validation.

use crate::{
    error::{Result, WlsnapError},
    platform::output_info::{LogicalRect, OutputInfo},
};

/// Validate that a region is within the bounds of the given output.
/// Returns an error if the region is empty, negative-sized, or outside the output.
pub fn validate_region(region: LogicalRect, output: &OutputInfo) -> Result<LogicalRect> {
    let width = region.max.x - region.min.x;
    let height = region.max.y - region.min.y;

    if width <= 0.0 {
        return Err(WlsnapError::Stitching("region width must be positive"));
    }
    if height <= 0.0 {
        return Err(WlsnapError::Stitching("region height must be positive"));
    }

    let geom = &output.logical_geometry;
    if region.min.x < geom.min.x
        || region.min.y < geom.min.y
        || region.max.x > geom.max.x
        || region.max.y > geom.max.y
    {
        return Err(WlsnapError::Stitching("region is outside output bounds"));
    }

    Ok(region)
}

/// Crop a captured image to the specified region.
/// The region is in logical coordinates relative to the output's logical_geometry.
pub fn crop_image(
    image: &image::RgbaImage,
    region: &LogicalRect,
    output: &OutputInfo,
) -> image::RgbaImage {
    let img_w = image.width();
    let img_h = image.height();

    // Compute the actual scale ratio from the captured image size vs logical size.
    // This handles fractional scaling where output.scale_factor may not match
    // the actual physical/logical ratio.
    let log_w = output.logical_geometry.max.x - output.logical_geometry.min.x;
    let log_h = output.logical_geometry.max.y - output.logical_geometry.min.y;
    let scale_x = if log_w > 0.0 {
        img_w as f64 / log_w
    } else {
        output.scale_factor
    };
    let scale_y = if log_h > 0.0 {
        img_h as f64 / log_h
    } else {
        output.scale_factor
    };

    let x = (region.min.x * scale_x).round() as u32;
    let y = (region.min.y * scale_y).round() as u32;
    let width = ((region.max.x - region.min.x) * scale_x).round() as u32;
    let height = ((region.max.y - region.min.y) * scale_y).round() as u32;

    tracing::debug!(
        "crop_image: region={:?} log={:.1}x{:.1} img={}x{} scale_x={:.3} scale_y={:.3} → physical x={}, y={}, w={}, h={}",
        region,
        log_w,
        log_h,
        img_w,
        img_h,
        scale_x,
        scale_y,
        x,
        y,
        width,
        height
    );

    // Clamp to image bounds to avoid panics
    let x = x.min(img_w);
    let y = y.min(img_h);
    let width = width.min(img_w.saturating_sub(x));
    let height = height.min(img_h.saturating_sub(y));

    tracing::debug!(
        "crop_image after clamp: x={}, y={}, w={}, h={}",
        x,
        y,
        width,
        height
    );

    if width == 0 || height == 0 {
        tracing::warn!(
            "Crop region results in zero-size image: x={}, y={}, w={}, h={}; image={}x{}",
            x,
            y,
            width,
            height,
            img_w,
            img_h
        );
        // Return empty image of the same type to avoid downstream panics
        return image::RgbaImage::new(1, 1);
    }

    let result = image::imageops::crop_imm(image, x, y, width, height).to_image();
    tracing::debug!("crop_image result: {}x{}", result.width(), result.height());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::output_info::{LogicalPoint, OutputTransform};

    fn make_output(x: f64, y: f64, w: f64, h: f64, scale: f64) -> OutputInfo {
        OutputInfo {
            name: "test".to_string(),
            description: String::new(),
            logical_geometry: LogicalRect {
                min: LogicalPoint { x, y },
                max: LogicalPoint { x: x + w, y: y + h },
            },
            physical_size: ((w * scale) as u32, (h * scale) as u32),
            scale_factor: scale,
            transform: OutputTransform::Normal,
        }
    }

    #[test]
    fn validate_region_valid() {
        let output = make_output(0.0, 0.0, 1920.0, 1080.0, 1.0);
        let region = LogicalRect {
            min: LogicalPoint { x: 100.0, y: 100.0 },
            max: LogicalPoint { x: 500.0, y: 500.0 },
        };
        assert!(validate_region(region, &output).is_ok());
    }

    #[test]
    fn validate_region_zero_width() {
        let output = make_output(0.0, 0.0, 1920.0, 1080.0, 1.0);
        let region = LogicalRect {
            min: LogicalPoint { x: 100.0, y: 100.0 },
            max: LogicalPoint { x: 100.0, y: 500.0 },
        };
        assert!(validate_region(region, &output).is_err());
    }

    #[test]
    fn validate_region_zero_height() {
        let output = make_output(0.0, 0.0, 1920.0, 1080.0, 1.0);
        let region = LogicalRect {
            min: LogicalPoint { x: 100.0, y: 100.0 },
            max: LogicalPoint { x: 500.0, y: 100.0 },
        };
        assert!(validate_region(region, &output).is_err());
    }

    #[test]
    fn validate_region_negative_width() {
        let output = make_output(0.0, 0.0, 1920.0, 1080.0, 1.0);
        let region = LogicalRect {
            min: LogicalPoint { x: 500.0, y: 100.0 },
            max: LogicalPoint { x: 100.0, y: 500.0 },
        };
        assert!(validate_region(region, &output).is_err());
    }

    #[test]
    fn validate_region_outside_bounds() {
        let output = make_output(0.0, 0.0, 1920.0, 1080.0, 1.0);
        let region = LogicalRect {
            min: LogicalPoint {
                x: 1900.0,
                y: 100.0,
            },
            max: LogicalPoint {
                x: 2000.0,
                y: 500.0,
            },
        };
        assert!(validate_region(region, &output).is_err());
    }

    #[test]
    fn validate_region_exactly_at_bounds() {
        let output = make_output(0.0, 0.0, 1920.0, 1080.0, 1.0);
        let region = LogicalRect {
            min: LogicalPoint { x: 0.0, y: 0.0 },
            max: LogicalPoint {
                x: 1920.0,
                y: 1080.0,
            },
        };
        assert!(validate_region(region, &output).is_ok());
    }

    #[test]
    fn crop_image_basic() {
        let mut img = image::RgbaImage::new(100, 100);
        img.put_pixel(10, 20, image::Rgba([255, 0, 0, 255]));

        let output = make_output(0.0, 0.0, 100.0, 100.0, 1.0);
        let region = LogicalRect {
            min: LogicalPoint { x: 10.0, y: 20.0 },
            max: LogicalPoint { x: 20.0, y: 30.0 },
        };

        let cropped = crop_image(&img, &region, &output);
        assert_eq!(cropped.dimensions(), (10, 10));
        assert_eq!(cropped.get_pixel(0, 0), &image::Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn crop_image_scaled() {
        // Image is 200x200 physical pixels, output scale is 2.0
        // Logical region 10,20 -> 20,30 maps to physical 20,40 -> 40,60
        let mut img = image::RgbaImage::new(200, 200);
        img.put_pixel(20, 40, image::Rgba([0, 255, 0, 255]));

        let output = make_output(0.0, 0.0, 100.0, 100.0, 2.0);
        let region = LogicalRect {
            min: LogicalPoint { x: 10.0, y: 20.0 },
            max: LogicalPoint { x: 20.0, y: 30.0 },
        };

        let cropped = crop_image(&img, &region, &output);
        assert_eq!(cropped.dimensions(), (20, 20));
        assert_eq!(cropped.get_pixel(0, 0), &image::Rgba([0, 255, 0, 255]));
    }

    #[test]
    fn crop_image_clamped() {
        // Region extends beyond image bounds
        let img = image::RgbaImage::new(50, 50);
        let output = make_output(0.0, 0.0, 100.0, 100.0, 1.0);
        let region = LogicalRect {
            min: LogicalPoint { x: 40.0, y: 40.0 },
            max: LogicalPoint { x: 60.0, y: 60.0 },
        };

        let cropped = crop_image(&img, &region, &output);
        assert_eq!(cropped.dimensions(), (10, 10));
    }
}
