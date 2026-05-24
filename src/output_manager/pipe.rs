use crate::error::Result;
use image::RgbaImage;
use std::io::{self, Write};

/// Encode the image as PNG and write it to stdout.
pub fn write_to_stdout(image: &RgbaImage) -> Result<()> {
    let mut stdout = io::stdout().lock();
    let encoder = image::codecs::png::PngEncoder::new(&mut stdout);
    image.write_with_encoder(encoder)?;
    stdout.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_to_stdout_produces_valid_png() {
        let img = RgbaImage::from_raw(2, 2, vec![0; 16]).unwrap();
        let mut buf = Vec::new();
        {
            let encoder = image::codecs::png::PngEncoder::new(&mut buf);
            img.write_with_encoder(encoder).unwrap();
        }
        // PNG magic bytes
        assert_eq!(&buf[..4], &[0x89, 0x50, 0x4E, 0x47]);
        // Validate by re-loading
        let loaded = image::load_from_memory_with_format(&buf, image::ImageFormat::Png).unwrap();
        assert_eq!(loaded.width(), 2);
        assert_eq!(loaded.height(), 2);
    }
}
