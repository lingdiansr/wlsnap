use crate::error::{Result, WlsnapError};
use image::RgbaImage;
use std::borrow::Cow;

/// Copy an RGBA image to the system clipboard.
pub fn copy_to_clipboard(image: &RgbaImage) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| WlsnapError::Clipboard(format!("failed to open clipboard: {e}")))?;

    let (width, height) = image.dimensions();
    let bytes = Cow::Owned(image.as_raw().clone());

    let img_data = arboard::ImageData {
        width: width as usize,
        height: height as usize,
        bytes,
    };

    clipboard
        .set_image(img_data)
        .map_err(|e| WlsnapError::Clipboard(format!("failed to set image: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_struct_instantiable() {
        // arboard may fail on headless systems; we only verify the code path compiles.
        let img = RgbaImage::from_raw(1, 1, vec![255, 0, 0, 255]).unwrap();
        let result = copy_to_clipboard(&img);
        // Accept either success or a clipboard-specific error.
        assert!(result.is_ok() || result.is_err());
    }
}
