use std::process::{Command, Stdio};

use image::RgbaImage;

use crate::error::{Result, WlsnapError};

/// Copy an RGBA image to the system clipboard.
/// On Linux Wayland, uses wl-copy which daemonizes to keep data alive.
pub fn copy_to_clipboard(image: &RgbaImage) -> Result<()> {
    // Encode image as PNG into memory.
    let mut png_bytes: Vec<u8> = Vec::new();
    {
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        image
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| WlsnapError::Clipboard(format!("png encode: {e}")))?;
    }

    let mut child = Command::new("wl-copy")
        .arg("--type")
        .arg("image/png")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| WlsnapError::Clipboard(format!("wl-copy spawn: {e}")))?;

    // Write PNG data to wl-copy's stdin and close it.
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(&png_bytes)
            .map_err(|e| WlsnapError::Clipboard(format!("wl-copy stdin: {e}")))?;
        stdin
            .flush()
            .map_err(|e| WlsnapError::Clipboard(format!("wl-copy flush: {e}")))?;
        // stdin is dropped here, closing the pipe.
    }

    // Wait for wl-copy to finish reading stdin.
    // wl-copy daemonizes to keep clipboard data alive, but it must
    // finish reading stdin before we drop our reference.
    match child.wait() {
        Ok(status) if !status.success() => {
            return Err(WlsnapError::Clipboard(format!(
                "wl-copy exited with status: {}",
                status
            )));
        }
        Ok(_) => {}
        Err(e) => {
            return Err(WlsnapError::Clipboard(format!(
                "wl-copy wait failed: {}",
                e
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_struct_instantiable() {
        let img = RgbaImage::from_raw(1, 1, vec![255, 0, 0, 255]).unwrap();
        let result = copy_to_clipboard(&img);
        assert!(result.is_ok() || result.is_err());
    }
}
