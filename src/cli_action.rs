//! CLI headless action logic.
//!
//! Extracts pure CLI capture/output dispatch so `main.rs` can bypass eframe
//! entirely for non-interactive modes.

use std::path::PathBuf;

use crate::cli::Cli;
use crate::config::Config;
use crate::error::Result;
use crate::output_manager::{OutputAction, dispatch};

/// Determine the final `OutputAction` from CLI flags and config.
///
/// Priority (highest first):
/// 1. `--stdout`  → Pipe
/// 2. `--exec CMD` → Exec(cmd)
/// 3. `--clipboard` → Clipboard
/// 4. `-o PATH` → Save(Some(path))
/// 5. `general.post_capture` config → fallback
pub fn determine_output_action(cli: &Cli, config: &Config) -> OutputAction {
    if cli.stdout {
        return OutputAction::Pipe;
    }
    if let Some(ref cmd) = cli.exec {
        return OutputAction::Exec(cmd.clone());
    }
    if cli.clipboard {
        return OutputAction::Clipboard;
    }
    if let Some(ref path) = cli.output {
        return OutputAction::Save(Some(path.clone()));
    }

    parse_post_capture_config(&config.general.post_capture)
}

/// Parse the `general.post_capture` config string into an `OutputAction`.
fn parse_post_capture_config(value: &str) -> OutputAction {
    match value.to_lowercase().as_str() {
        "clipboard" => OutputAction::Clipboard,
        "pipe" => OutputAction::Pipe,
        "save" | "edit" | "ask" => OutputAction::Save(None),
        _ => OutputAction::Save(None),
    }
}

/// Returns `true` if the selected CLI mode requires a GUI window.
///
/// In v0.1.0 only future modes (`--window`, `--pin`, `--scroll-auto`,
/// `--scroll-manual`) need GUI. All capture modes (`--screen`, `--all-screen`,
/// `--range`) return `false`.
pub fn needs_gui(cli: &Cli) -> bool {
    if cli.mode.window {
        return true;
    }
    if cli.mode.pin.is_some() {
        return true;
    }
    if cli.mode.scroll_auto {
        return true;
    }
    if cli.mode.scroll_manual {
        return true;
    }

    // --range without coordinates needs interactive GUI selection
    if cli.mode.range.as_ref().is_some_and(|s| s.is_empty()) {
        return true;
    }

    false
}

/// Run a headless CLI capture and dispatch the output.
///
/// Creates a temporary tokio runtime, performs the screenshot capture, then
/// dispatches the result according to `determine_output_action`.
pub fn run_cli_capture(cli: &Cli, config: &Config) -> Result<PathBuf> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| crate::error::WlsnapError::Io(std::io::Error::other(e)))?;

    let overlay_cursor = config.advanced.include_cursor || cli.cursor;
    let mode_name = cli.mode.selected_mode_name().to_string();

    let captured = rt.block_on(async {
        if cli.mode.all_screen {
            crate::capture::output::capture_all_screens(overlay_cursor).await
        } else {
            crate::capture::output::capture_current_screen(overlay_cursor).await
        }
    })?;

    // If --range has coordinates, crop to the specified region.
    let image = if let Some(ref coords) = cli.mode.range {
        if !coords.is_empty() {
            let region = crate::capture::region::parse_region_arg(coords)?;
            crate::capture::region::crop_image(&captured.image, &region, &captured.source_output)
        } else {
            captured.image
        }
    } else {
        captured.image
    };

    let action = determine_output_action(cli, config);
    dispatch(&image, action, &config.general, &mode_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_cli_screen() -> Cli {
        Cli {
            mode: crate::cli::CaptureMode {
                screen: true,
                all_screen: false,
                range: None,
                window: false,
                pin: None,
                scroll_auto: false,
                scroll_manual: false,
            },
            stdout: false,
            output: None,
            exec: None,
            clipboard: false,
            cursor: false,
            list_outputs: false,
            debug_protocol: false,
        }
    }

    fn make_cli_with_stdout() -> Cli {
        let mut cli = make_cli_screen();
        cli.stdout = true;
        cli
    }

    fn make_cli_with_exec(cmd: &str) -> Cli {
        let mut cli = make_cli_screen();
        cli.exec = Some(cmd.into());
        cli
    }

    fn make_cli_with_clipboard() -> Cli {
        let mut cli = make_cli_screen();
        cli.clipboard = true;
        cli
    }

    fn make_cli_with_output(path: PathBuf) -> Cli {
        let mut cli = make_cli_screen();
        cli.output = Some(path);
        cli
    }

    fn make_cli_with_silent() -> Cli {
        let mut cli = make_cli_screen();
        cli
    }

    fn make_config_with_post_capture(value: &str) -> Config {
        let mut config = Config::default();
        config.general.post_capture = value.into();
        config
    }

    // ------------------------------------------------------------------
    // determine_output_action priority tests
    // ------------------------------------------------------------------

    #[test]
    fn test_stdout_wins() {
        let cli = make_cli_with_stdout();
        let config = Config::default();
        assert!(matches!(determine_output_action(&cli, &config), OutputAction::Pipe));
    }

    #[test]
    fn test_exec_wins_over_clipboard() {
        let mut cli = make_cli_with_exec("echo {file}");
        cli.clipboard = true;
        let config = Config::default();
        match determine_output_action(&cli, &config) {
            OutputAction::Exec(cmd) => assert_eq!(cmd, "echo {file}"),
            other => panic!("expected Exec, got {:?}", other),
        }
    }

    #[test]
    fn test_clipboard_wins_over_save() {
        let mut cli = make_cli_with_clipboard();
        let config = Config::default();
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Clipboard),
            "clipboard should win over default save"
        );
    }

    #[test]
    fn test_output_flag_maps_to_save() {
        let cli = make_cli_with_output(PathBuf::from("/tmp/test.png"));
        let config = Config::default();
        match determine_output_action(&cli, &config) {
            OutputAction::Save(Some(p)) => assert_eq!(p, PathBuf::from("/tmp/test.png")),
            other => panic!("expected Save(Some), got {:?}", other),
        }
    }

    #[test]
    fn test_config_default_save() {
        let cli = make_cli_screen();
        let config = make_config_with_post_capture("save");
        assert!(matches!(determine_output_action(&cli, &config), OutputAction::Save(None)));
    }

    #[test]
    fn test_config_default_clipboard() {
        let cli = make_cli_screen();
        let config = make_config_with_post_capture("clipboard");
        assert!(matches!(determine_output_action(&cli, &config), OutputAction::Clipboard));
    }

    #[test]
    fn test_config_default_pipe() {
        let cli = make_cli_screen();
        let config = make_config_with_post_capture("pipe");
        assert!(matches!(determine_output_action(&cli, &config), OutputAction::Pipe));
    }

    // ------------------------------------------------------------------
    // needs_gui tests
    // ------------------------------------------------------------------

    #[test]
    fn test_needs_gui_screen() {
        let cli = make_cli_screen();
        assert!(!needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_all_screen() {
        let mut cli = make_cli_screen();
        cli.mode.screen = false;
        cli.mode.all_screen = true;
        assert!(!needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_range() {
        let mut cli = make_cli_screen();
        cli.mode.screen = false;
        cli.mode.range = Some(String::new());
        assert!(needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_range_with_coords() {
        let mut cli = make_cli_screen();
        cli.mode.screen = false;
        cli.mode.range = Some("100,200,500,400".into());
        assert!(!needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_window() {
        let mut cli = make_cli_screen();
        cli.mode.screen = false;
        cli.mode.window = true;
        assert!(needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_pin() {
        let mut cli = make_cli_screen();
        cli.mode.screen = false;
        cli.mode.pin = Some(None);
        assert!(needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_scroll_auto() {
        let mut cli = make_cli_screen();
        cli.mode.screen = false;
        cli.mode.scroll_auto = true;
        assert!(needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_scroll_manual() {
        let mut cli = make_cli_screen();
        cli.mode.screen = false;
        cli.mode.scroll_manual = true;
        assert!(needs_gui(&cli));
    }
}
