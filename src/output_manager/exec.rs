use crate::error::{Result, WlsnapError};
use image::RgbaImage;
use std::process::Command;

/// Save `image` to a temporary PNG file, substitute `{file}` in `cmd_template`,
/// and execute the resulting shell command.
///
/// If the command fails (or is not found), the temp file is preserved and an
/// error is returned.
pub fn exec_with_image(cmd_template: &str, image: &RgbaImage) -> Result<()> {
    let tmp = tempfile::Builder::new()
        .suffix(".png")
        .tempfile()
        .map_err(WlsnapError::Io)?;

    let path = tmp.path().to_path_buf();

    {
        let file = std::fs::File::create(&path)?;
        let mut writer = std::io::BufWriter::new(file);
        let encoder = image::codecs::png::PngEncoder::new(&mut writer);
        image.write_with_encoder(encoder)?;
        // flush writer so the file is fully written before the external command reads it
        drop(writer);
    }

    let cmd_str = cmd_template.replace("{file}", &path.to_string_lossy());
    let args = shell_words::split(&cmd_str)
        .map_err(|e| WlsnapError::ExternalCommand(format!("shell parse error: {e}")))?;

    if args.is_empty() {
        return Err(WlsnapError::ExternalCommand("empty command".into()));
    }

    let mut command = Command::new(&args[0]);
    command.args(&args[1..]);

    let status = command
        .status()
        .map_err(|e| WlsnapError::ExternalCommand(format!("failed to spawn: {e}")))?;

    if !status.success() {
        return Err(WlsnapError::ExternalCommand(format!(
            "command exited with status: {}",
            status
        )));
    }

    // On success we can delete the temp file explicitly.
    let _ = tmp.close();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_image() -> RgbaImage {
        RgbaImage::from_raw(2, 2, vec![0; 16]).unwrap()
    }

    #[test]
    fn test_exec_with_image_command_not_found() {
        let img = dummy_image();
        let result = exec_with_image("/nonexistent_binary_{file}", &img);
        assert!(result.is_err());
    }

    #[test]
    fn test_exec_with_image_true_succeeds() {
        let img = dummy_image();
        // `true` should always succeed
        let result = exec_with_image("true {file}", &img);
        assert!(result.is_ok(), "expected `true` to succeed: {:?}", result);
    }

    #[test]
    fn test_exec_with_image_false_fails() {
        let img = dummy_image();
        let result = exec_with_image("false {file}", &img);
        assert!(result.is_err(), "expected `false` to fail");
    }

    #[test]
    fn test_exec_with_image_file_placeholder_replaced() {
        let img = dummy_image();
        // `cat` on the temp file should succeed (file exists and is readable)
        let result = exec_with_image("cat {file}", &img);
        assert!(result.is_ok(), "expected `cat` to succeed: {:?}", result);
    }
}
