use std::path::PathBuf;

use clap::{Args, Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "wlsnap")]
#[command(version, about = "Wayland screenshot utility")]
pub struct Cli {
    #[command(flatten)]
    pub mode: CaptureMode,

    /// Action to perform after capture
    #[arg(short, long, value_name = "ACTION")]
    pub post: Option<PostCaptureAction>,

    /// Output the image to stdout as PNG
    #[arg(long)]
    pub stdout: bool,

    /// Save the image to the specified path
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Execute a command with the captured image file path
    #[arg(long, value_name = "CMD")]
    pub exec: Option<String>,

    /// Copy the image to the clipboard
    #[arg(long)]
    pub clipboard: bool,

    /// Save without printing the file path to stdout
    #[arg(long)]
    pub silent: bool,

    /// Include the cursor in the screenshot
    #[arg(long)]
    pub cursor: bool,

    /// List all detected outputs and exit
    #[arg(long)]
    pub list_outputs: bool,

    /// Print available Wayland protocols and exit
    #[arg(long)]
    pub debug_protocol: bool,

    /// Path to a custom configuration file
    #[arg(short, long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args, Clone)]
#[group(required = false, multiple = false)]
pub struct CaptureMode {
    /// Capture the entire current screen
    #[arg(long)]
    pub screen: bool,

    /// Capture all screens and stitch them into one image
    #[arg(long)]
    pub screen_all: bool,

    /// Capture the current screen (same as --full in v0.1.0)
    #[arg(long)]
    pub area: bool,

    /// Capture a specific window (interactive, requires GUI)
    #[arg(long)]
    pub window: bool,



    /// Pin the captured image as a floating window (requires GUI)
    #[arg(long, value_name = "PATH")]
    pub pin: Option<Option<PathBuf>>,

    /// Automatically capture a scrolling area (requires GUI)
    #[arg(long)]
    pub scroll_auto: bool,

    /// Manually capture a scrolling area (requires GUI)
    #[arg(long)]
    pub scroll_manual: bool,
}

impl CaptureMode {
    pub fn selected_mode_name(&self) -> &'static str {
        if self.screen {
            "screen"
        } else if self.screen_all {
            "screen_all"
        } else if self.area {
            "area"
        } else if self.window {
            "window"
        } else if self.pin.is_some() {
            "pin"
        } else if self.scroll_auto {
            "scroll_auto"
        } else if self.scroll_manual {
            "scroll_manual"
        } else {
            "screen"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PostCaptureAction {
    /// Open the image in the built-in editor (requires GUI)
    Edit,
    /// Save the image to disk
    Save,
    /// Copy the image to the clipboard
    Clipboard,
    /// Output the image to stdout as PNG
    Pipe,
    /// Ask what to do with the image (requires GUI)
    Ask,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_screen_flag() {
        let cli = Cli::try_parse_from(["wlsnap", "--screen"]).unwrap();
        assert!(cli.mode.screen);
        assert!(!cli.mode.area);
        assert_eq!(cli.mode.selected_mode_name(), "screen");
    }

    #[test]
    fn parse_area_and_stdout() {
        let cli = Cli::try_parse_from(["wlsnap", "--area", "--stdout"]).unwrap();
        assert!(cli.mode.area);
        assert!(cli.stdout);
        assert_eq!(cli.mode.selected_mode_name(), "area");
    }

    #[test]
    fn parse_post_edit() {
        let cli = Cli::try_parse_from(["wlsnap", "--post", "edit"]).unwrap();
        assert_eq!(cli.post, Some(PostCaptureAction::Edit));
    }

    #[test]
    fn conflicting_modes_rejected() {
        let result = Cli::try_parse_from(["wlsnap", "--screen", "--area"]);
        assert!(result.is_err());
    }

    #[test]
    fn help_produces_display_help_error() {
        let result = Cli::try_parse_from(["wlsnap", "--help"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }
}
