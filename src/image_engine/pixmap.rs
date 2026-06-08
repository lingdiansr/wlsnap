//! tiny-skia Pixmap ↔ image::RgbaImage interop.

use tiny_skia::IntSize;

/// Convert `image::RgbaImage` to `tiny_skia::Pixmap`.
///
/// Both formats store pixels as 4-byte RGBA sequences, so this performs a
/// direct memory copy.  The caller is responsible for any colour-space
/// conversion (e.g. straight vs. premultiplied alpha) if required.
pub fn rgba_to_pixmap(img: &image::RgbaImage) -> Option<tiny_skia::Pixmap> {
    let (width, height) = img.dimensions();
    let data = img.as_raw().clone();
    let size = IntSize::from_wh(width, height)?;
    tiny_skia::Pixmap::from_vec(data, size)
}

/// Convert `tiny_skia::Pixmap` to `image::RgbaImage`.
///
/// Both formats store pixels as 4-byte RGBA sequences, so this performs a
/// direct memory copy.  The caller is responsible for any colour-space
/// conversion if required.
pub fn pixmap_to_rgba(pixmap: &tiny_skia::Pixmap) -> image::RgbaImage {
    let width = pixmap.width();
    let height = pixmap.height();
    let data = pixmap.data().to_vec();
    image::RgbaImage::from_raw(width, height, data)
        .expect("valid Pixmap dimensions guarantee a valid RgbaImage")
}

#[cfg(test)]
mod tests {
    use image::Rgba;

    use super::*;

    #[test]
    fn pixmap_rgba_roundtrip_empty() {
        let img = image::RgbaImage::new(0, 0);
        // tiny-skia does not support zero-sized pixmaps.
        assert!(rgba_to_pixmap(&img).is_none());
    }

    #[test]
    fn pixmap_rgba_roundtrip_single_pixel() {
        let mut img = image::RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([255, 128, 64, 200]));

        let pixmap = rgba_to_pixmap(&img).expect("pixmap creation failed");
        let back = pixmap_to_rgba(&pixmap);
        assert_eq!(img, back);
    }

    #[test]
    fn pixmap_rgba_roundtrip_varied_pixels() {
        let mut img = image::RgbaImage::new(3, 2);
        img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([0, 255, 0, 255]));
        img.put_pixel(2, 0, Rgba([0, 0, 255, 255]));
        img.put_pixel(0, 1, Rgba([255, 255, 0, 128]));
        img.put_pixel(1, 1, Rgba([0, 255, 255, 64]));
        img.put_pixel(2, 1, Rgba([255, 255, 255, 0]));

        let pixmap = rgba_to_pixmap(&img).expect("pixmap creation failed");
        assert_eq!(pixmap.width(), 3);
        assert_eq!(pixmap.height(), 2);

        let back = pixmap_to_rgba(&pixmap);
        assert_eq!(img, back);
    }

    #[test]
    fn pixmap_to_rgba_dimensions_match() {
        let img = image::RgbaImage::new(7, 5);
        let pixmap = rgba_to_pixmap(&img).unwrap();
        let back = pixmap_to_rgba(&pixmap);
        assert_eq!(back.width(), 7);
        assert_eq!(back.height(), 5);
    }
}
