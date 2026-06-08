//! Output transform (rotation / flipping) for Wayland output buffers.

/// Describes how an output's buffer is transformed relative to its logical
/// orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputTransform {
    Normal,
    Rotated90,
    Rotated180,
    Rotated270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

impl OutputTransform {
    /// Returns the physical dimensions after applying the transform.
    pub fn apply(&self, width: u32, height: u32) -> (u32, u32) {
        match self {
            OutputTransform::Normal
            | OutputTransform::Flipped
            | OutputTransform::Rotated180
            | OutputTransform::Flipped180 => (width, height),
            OutputTransform::Rotated90
            | OutputTransform::Rotated270
            | OutputTransform::Flipped90
            | OutputTransform::Flipped270 => (height, width),
        }
    }

    /// Rotate or flip an `RgbaImage` according to the transform.
    pub fn apply_to_image(&self, img: &image::RgbaImage) -> image::RgbaImage {
        let (w, h) = img.dimensions();
        let (new_w, new_h) = self.apply(w, h);

        // Empty images need no pixel shuffling.
        if w == 0 || h == 0 {
            return image::RgbaImage::new(new_w, new_h);
        }

        let mut result = image::RgbaImage::new(new_w, new_h);

        for (x, y, pixel) in img.enumerate_pixels() {
            let (nx, ny) = match self {
                OutputTransform::Normal => (x, y),
                OutputTransform::Rotated90 => (y, w - 1 - x),
                OutputTransform::Rotated180 => (w - 1 - x, h - 1 - y),
                OutputTransform::Rotated270 => (h - 1 - y, x),
                OutputTransform::Flipped => (w - 1 - x, y),
                OutputTransform::Flipped90 => (y, x),
                OutputTransform::Flipped180 => (x, h - 1 - y),
                OutputTransform::Flipped270 => (h - 1 - y, w - 1 - x),
            };
            result.put_pixel(nx, ny, *pixel);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use image::Rgba;

    use super::*;

    #[test]
    fn apply_dimensions_no_swap() {
        assert_eq!(OutputTransform::Normal.apply(1920, 1080), (1920, 1080));
        assert_eq!(OutputTransform::Flipped.apply(1920, 1080), (1920, 1080));
        assert_eq!(OutputTransform::Rotated180.apply(1920, 1080), (1920, 1080));
        assert_eq!(OutputTransform::Flipped180.apply(1920, 1080), (1920, 1080));
    }

    #[test]
    fn apply_dimensions_swapped() {
        assert_eq!(OutputTransform::Rotated90.apply(1920, 1080), (1080, 1920));
        assert_eq!(OutputTransform::Rotated270.apply(1920, 1080), (1080, 1920));
        assert_eq!(OutputTransform::Flipped90.apply(1920, 1080), (1080, 1920));
        assert_eq!(OutputTransform::Flipped270.apply(1920, 1080), (1080, 1920));
    }

    #[test]
    fn apply_to_image_rotated90() {
        // 2×3 image
        let mut img = image::RgbaImage::new(2, 3);
        img.put_pixel(0, 0, Rgba([1, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([2, 0, 0, 255]));
        img.put_pixel(0, 1, Rgba([3, 0, 0, 255]));
        img.put_pixel(1, 1, Rgba([4, 0, 0, 255]));
        img.put_pixel(0, 2, Rgba([5, 0, 0, 255]));
        img.put_pixel(1, 2, Rgba([6, 0, 0, 255]));

        let out = OutputTransform::Rotated90.apply_to_image(&img);
        assert_eq!(out.dimensions(), (3, 2));

        // After 90° CCW:
        //   old (0,0)=1 -> (0,1)
        //   old (1,0)=2 -> (0,0)
        //   old (0,1)=3 -> (1,1)
        //   old (1,1)=4 -> (1,0)
        //   old (0,2)=5 -> (2,1)
        //   old (1,2)=6 -> (2,0)
        assert_eq!(out.get_pixel(0, 0), &Rgba([2, 0, 0, 255]));
        assert_eq!(out.get_pixel(0, 1), &Rgba([1, 0, 0, 255]));
        assert_eq!(out.get_pixel(1, 0), &Rgba([4, 0, 0, 255]));
        assert_eq!(out.get_pixel(1, 1), &Rgba([3, 0, 0, 255]));
        assert_eq!(out.get_pixel(2, 0), &Rgba([6, 0, 0, 255]));
        assert_eq!(out.get_pixel(2, 1), &Rgba([5, 0, 0, 255]));
    }

    #[test]
    fn apply_to_image_rotated180() {
        let mut img = image::RgbaImage::new(2, 2);
        img.put_pixel(0, 0, Rgba([1, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([2, 0, 0, 255]));
        img.put_pixel(0, 1, Rgba([3, 0, 0, 255]));
        img.put_pixel(1, 1, Rgba([4, 0, 0, 255]));

        let out = OutputTransform::Rotated180.apply_to_image(&img);
        assert_eq!(out.get_pixel(0, 0), &Rgba([4, 0, 0, 255]));
        assert_eq!(out.get_pixel(1, 0), &Rgba([3, 0, 0, 255]));
        assert_eq!(out.get_pixel(0, 1), &Rgba([2, 0, 0, 255]));
        assert_eq!(out.get_pixel(1, 1), &Rgba([1, 0, 0, 255]));
    }

    #[test]
    fn apply_to_image_flipped() {
        let mut img = image::RgbaImage::new(3, 1);
        img.put_pixel(0, 0, Rgba([1, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([2, 0, 0, 255]));
        img.put_pixel(2, 0, Rgba([3, 0, 0, 255]));

        let out = OutputTransform::Flipped.apply_to_image(&img);
        assert_eq!(out.get_pixel(0, 0), &Rgba([3, 0, 0, 255]));
        assert_eq!(out.get_pixel(1, 0), &Rgba([2, 0, 0, 255]));
        assert_eq!(out.get_pixel(2, 0), &Rgba([1, 0, 0, 255]));
    }

    #[test]
    fn apply_to_image_empty() {
        let img = image::RgbaImage::new(0, 5);
        let out = OutputTransform::Rotated90.apply_to_image(&img);
        assert_eq!(out.dimensions(), (5, 0));
    }
}
