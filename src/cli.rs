use std::path::PathBuf;

use clap::{Args, Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "wlsnap")]
#[command(version, about = "Wayland screenshot utility")]
pub struct Cli {
    #[command(flatten)]
    pub mode: CaptureMode,

    #[arg(short, long, value_name = "ACTION")]
    pub post: Option<PostCaptureAction>,

    #[arg(long)]
    pub stdout: bool,

    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    #[arg(long, value_name = "CMD")]
    pub exec: Option<String>,

    #[arg(long)]
    pub clipboard: bool,

    #[arg(long)]
    pub silent: bool,

    #[arg(long)]
    pub cursor: bool,

    #[arg(long)]
    pub list_outputs: bool,

    #[arg(long)]
    pub debug_protocol: bool,

    #[arg(short, long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args, Clone)]
#[group(required = false, multiple = false)]
pub struct CaptureMode {
    #[arg(long)]
    pub full: bool,

    #[arg(long)]
    pub full_all: bool,

    #[arg(long)]
    pub area: bool,

    #[arg(long)]
    pub window: bool,

    #[arg(id = "screen", long = "screen")]
    pub output: bool,

    #[arg(long, value_name = "PATH")]
    pub pin: Option<Option<PathBuf>>,

    #[arg(long)]
    pub scroll_auto: bool,

    #[arg(long)]
    pub scroll_manual: bool,
}

impl CaptureMode {
    pub fn selected_mode_name(&self) -> &'static str {
        if self.full {
            "full"
        } else if self.full_all {
            "full_all"
        } else if self.area {
            "area"
        } else if self.window {
            "window"
        } else if self.output {
            "screen"
        } else if self.pin.is_some() {
            "pin"
        } else if self.scroll_auto {
            "scroll_auto"
        } else if self.scroll_manual {
            "scroll_manual"
        } else {
            "full"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PostCaptureAction {
    Edit,
    Save,
    Clipboard,
    Pipe,
    Ask,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_flag() {
        let cli = Cli::try_parse_from(["wlsnap", "--full"]).unwrap();
        assert!(cli.mode.full);
        assert!(!cli.mode.area);
        assert_eq!(cli.mode.selected_mode_name(), "full");
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
        let result = Cli::try_parse_from(["wlsnap", "--full", "--area"]);
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
