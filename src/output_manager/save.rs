use crate::config::{GeneralConfig, ImageFormat, expand_placeholders};
use crate::error::Result;
use std::fs;
use std::io::BufWriter;
use std::path::PathBuf;

/// Save an image to disk according to `config`.
///
/// If `custom_path` is provided, the image is saved directly to that path
/// (ignoring `save_dir` and `filename_template`).
///
/// Otherwise, placeholders in `save_dir` and `filename_template` are expanded,
/// directories are created with `0o700`, and the file is written with `0o600`.
pub fn save_image(
    image: &image::RgbaImage,
    config: &GeneralConfig,
    mode: &str,
    custom_path: Option<&std::path::Path>,
) -> Result<PathBuf> {
    let path = if let Some(custom) = custom_path {
        // Use the exact path provided by CLI -o
        if let Some(parent) = custom.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            }
        }
        custom.to_path_buf()
    } else {
        let dir = expand_placeholders(&config.save_dir, Some(mode))?;
        let filename = expand_placeholders(&config.filename_template, Some(mode))?;
        let ext = config.format.extension();

        let path = PathBuf::from(&dir).join(format!("{}.{}", filename, ext));

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            }
        }
        path
    };

    let file = fs::File::create(&path)?;
    let mut writer = BufWriter::new(file);

    match config.format {
        ImageFormat::Png => {
            let encoder = image::codecs::png::PngEncoder::new(&mut writer);
            image.write_with_encoder(encoder)?;
        }
        ImageFormat::Jpeg => {
            let rgb_image = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut writer,
                config.jpeg_quality,
            );
            rgb_image.write_with_encoder(encoder)?;
        }
        ImageFormat::WebP => {
            let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut writer);
            image.write_with_encoder(encoder)?;
        }
    }

    drop(writer);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    fn dummy_image() -> RgbaImage {
        // Non-uniform pixel data so compression quality differences matter.
        RgbaImage::from_raw(
            4,
            4,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255, 255, 0, 255, 255,
                0, 255, 255, 255, 128, 128, 128, 255, 64, 64, 64, 255, 255, 255, 255, 255, 0, 0, 0,
                255, 255, 128, 0, 255, 0, 128, 255, 255, 128, 0, 128, 255, 0, 255, 128, 255, 128,
                128, 128, 255, 200, 200, 200, 255,
            ],
        )
        .unwrap()
    }

    fn make_config(tmp: &tempfile::TempDir, format: ImageFormat, quality: u8) -> GeneralConfig {
        GeneralConfig {
            post_capture: "save".into(),
            save_dir: tmp.path().to_str().unwrap().into(),
            filename_template: "test".into(),
            format,
            jpeg_quality: quality,
        }
    }

    #[test]
    fn test_save_image_png() {
        let tmp = tempfile::tempdir().unwrap();
        let config = make_config(&tmp, ImageFormat::Png, 90);
        let path = save_image(&dummy_image(), &config, "region", None).unwrap();
        assert!(path.exists());
        assert_eq!(path.extension().unwrap(), "png");
        let meta = fs::metadata(&path).unwrap();
        assert!(meta.len() > 0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn test_save_image_jpeg() {
        let tmp = tempfile::tempdir().unwrap();
        let config = make_config(&tmp, ImageFormat::Jpeg, 90);
        let path = save_image(&dummy_image(), &config, "window", None).unwrap();
        assert!(path.exists());
        assert_eq!(path.extension().unwrap(), "jpeg");
    }

    #[test]
    fn test_save_image_webp() {
        let tmp = tempfile::tempdir().unwrap();
        let config = make_config(&tmp, ImageFormat::WebP, 90);
        let path = save_image(&dummy_image(), &config, "full", None).unwrap();
        assert!(path.exists());
        assert_eq!(path.extension().unwrap(), "webp");
    }

    #[test]
    fn test_save_image_custom_path() {
        let tmp = tempfile::tempdir().unwrap();
        let config = make_config(&tmp, ImageFormat::Png, 90);
        let custom = tmp.path().join("my/custom/path.png");
        let path = save_image(&dummy_image(), &config, "ignored", Some(&custom)).unwrap();
        assert!(path.exists());
        assert_eq!(path, custom);
        assert_eq!(path.extension().unwrap(), "png");
    }

    #[test]
    fn test_jpeg_quality_affects_size() {
        let tmp1 = tempfile::tempdir().unwrap();
        let tmp2 = tempfile::tempdir().unwrap();

        let config_high = make_config(&tmp1, ImageFormat::Jpeg, 95);
        let config_low = make_config(&tmp2, ImageFormat::Jpeg, 50);

        let path_high = save_image(&dummy_image(), &config_high, "test", None).unwrap();
        let path_low = save_image(&dummy_image(), &config_low, "test", None).unwrap();

        let size_high = fs::metadata(&path_high).unwrap().len();
        let size_low = fs::metadata(&path_low).unwrap().len();

        // Higher quality should generally produce a larger file for our test image.
        assert!(
            size_high >= size_low,
            "high-quality JPEG ({}) should be >= low-quality JPEG ({})",
            size_high,
            size_low
        );
    }

    #[test]
    fn test_placeholders_expanded() {
        let tmp = tempfile::tempdir().unwrap();
        let mut config = make_config(&tmp, ImageFormat::Png, 90);
        config.filename_template = "img_{mode}".into();

        let path = save_image(&dummy_image(), &config, "my_mode", None).unwrap();
        let name = path.file_stem().unwrap().to_str().unwrap();
        assert!(
            name.contains("my_mode"),
            "expected mode placeholder in {}",
            name
        );
    }

    #[test]
    fn test_directory_permissions_unix() {
        let tmp = tempfile::tempdir().unwrap();
        let mut config = make_config(&tmp, ImageFormat::Png, 90);
        config.save_dir = tmp.path().join("sub/deep").to_str().unwrap().into();
        config.filename_template = "deep".into();

        let path = save_image(&dummy_image(), &config, "test", None).unwrap();
        assert!(path.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let parent = path.parent().unwrap();
            let perm = fs::metadata(parent).unwrap().permissions().mode() & 0o777;
            assert_eq!(perm, 0o700);
        }
    }
}
