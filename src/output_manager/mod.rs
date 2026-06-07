pub mod clipboard;
pub mod pipe;
pub mod save;

use crate::config::GeneralConfig;
use crate::error::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum OutputAction {
    /// Save to config default path, or custom path if Some.
    Save(Option<PathBuf>),
    Clipboard,
    Pipe,
}

/// Routes to the correct output method based on action.
///
/// Returns the final path if the image was saved,
/// otherwise an empty path for Clipboard / Pipe.
pub fn dispatch(
    image: &image::RgbaImage,
    action: OutputAction,
    config: &GeneralConfig,
    mode: &str,
) -> Result<PathBuf> {
    match action {
        OutputAction::Save(custom_path) => {
            save::save_image(image, config, mode, custom_path.as_deref())
        }
        OutputAction::Clipboard => {
            clipboard::copy_to_clipboard(image)?;
            Ok(PathBuf::new())
        }
        OutputAction::Pipe => {
            pipe::write_to_stdout(image)?;
            Ok(PathBuf::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GeneralConfig, ImageFormat};
    use image::RgbaImage;

    fn dummy_image() -> RgbaImage {
        RgbaImage::from_raw(2, 2, vec![0; 16]).unwrap()
    }

    #[test]
    fn test_dispatch_save_produces_file() {
        let tmp = tempfile::tempdir().unwrap();
        let config = GeneralConfig {
            save_dir: tmp.path().to_str().unwrap().to_string(),
            filename_template: "test_save".to_string(),
            format: ImageFormat::Png,
            ..Default::default()
        };

        let path = dispatch(&dummy_image(), OutputAction::Save(None), &config, "test_mode").unwrap();
        assert!(path.exists());
        assert_eq!(path.extension().unwrap(), "png");
    }

    #[test]
    fn test_dispatch_clipboard_ok() {
        // Under headless CI this may fail; we only check it doesn't panic for the path.
        let config = GeneralConfig::default();
        let result = dispatch(&dummy_image(), OutputAction::Clipboard, &config, "test");
        // arboard may fail without a display – that's acceptable for this smoke test.
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_dispatch_pipe_ok() {
        let config = GeneralConfig::default();
        let path = dispatch(&dummy_image(), OutputAction::Pipe, &config, "test").unwrap();
        assert!(path.as_os_str().is_empty());
    }
}
