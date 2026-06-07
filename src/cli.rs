use std::path::PathBuf;

use clap::{Args, Parser};

#[derive(Debug, Parser)]
#[command(name = "wlsnap")]
#[command(version, about = "Wayland screenshot utility")]
pub struct Cli {
    #[command(flatten)]
    pub mode: CaptureMode,

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
    #[arg(short, long)]
    pub clipboard: bool,

    /// Include the cursor in the screenshot
    #[arg(long)]
    pub cursor: bool,

    /// List all detected outputs and exit
    #[arg(long)]
    pub list_outputs: bool,

    /// Print available Wayland protocols and exit
    #[arg(long)]
    pub debug_protocol: bool,
}

#[derive(Debug, Args, Clone)]
#[group(required = false, multiple = false)]
pub struct CaptureMode {
    /// Capture the entire current screen
    #[arg(long, visible_alias = "full")]
    pub screen: bool,

    /// Capture all screens and stitch them into one image
    #[arg(short, long, visible_alias = "full-all")]
    pub all_screen: bool,

    /// Capture a region. Without value: interactive selection.
    /// With value: direct crop using "x,y,w,h" coordinates.
    #[arg(short, long, value_name = "X,Y,W,H", num_args = 0..=1, default_missing_value = "")]
    pub range: Option<String>,

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
        } else if self.all_screen {
            "all_screen"
        } else if self.range.is_some() {
            "range"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_screen_flag() {
        let cli = Cli::try_parse_from(["wlsnap", "--screen"]).unwrap();
        assert!(cli.mode.screen);
        assert!(cli.mode.range.is_none());
        assert_eq!(cli.mode.selected_mode_name(), "screen");
    }

    #[test]
    fn parse_full_alias() {
        let cli = Cli::try_parse_from(["wlsnap", "--full"]).unwrap();
        assert!(cli.mode.screen);
        assert_eq!(cli.mode.selected_mode_name(), "screen");
    }

    #[test]
    fn parse_range_and_stdout() {
        let cli = Cli::try_parse_from(["wlsnap", "--range", "--stdout"]).unwrap();
        // --range without value gives Some("") in clap
        assert!(cli.mode.range.is_some());
        assert!(cli.stdout);
        assert_eq!(cli.mode.selected_mode_name(), "range");
    }

    #[test]
    fn parse_range_short() {
        let cli = Cli::try_parse_from(["wlsnap", "-r", "100,200,500,400"]).unwrap();
        assert_eq!(cli.mode.range, Some("100,200,500,400".into()));
        assert_eq!(cli.mode.selected_mode_name(), "range");
    }

    #[test]
    fn parse_all_screen() {
        let cli = Cli::try_parse_from(["wlsnap", "--all-screen"]).unwrap();
        assert!(cli.mode.all_screen);
        assert_eq!(cli.mode.selected_mode_name(), "all_screen");
    }

    #[test]
    fn parse_all_screen_short() {
        let cli = Cli::try_parse_from(["wlsnap", "-a"]).unwrap();
        assert!(cli.mode.all_screen);
        assert_eq!(cli.mode.selected_mode_name(), "all_screen");
    }

    #[test]
    fn parse_clipboard_short() {
        let cli = Cli::try_parse_from(["wlsnap", "-c"]).unwrap();
        assert!(cli.clipboard);
    }

    #[test]
    fn conflicting_modes_rejected() {
        let result = Cli::try_parse_from(["wlsnap", "--screen", "--range"]);
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
