//! CLI headless action logic.
//!
//! Extracts pure CLI capture/output dispatch so `main.rs` can bypass eframe
//! entirely for non-interactive modes.

use std::path::PathBuf;

use crate::cli::{Cli, PostCaptureAction};
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
/// 5. `--silent` → Save(None)
/// 6. `--post ACTION` → override
/// 7. `general.post_capture` config → fallback
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
    if cli.silent {
        return OutputAction::Save(None);
    }
    if let Some(post) = cli.post {
        return post_action_to_output_action(post);
    }

    parse_post_capture_config(&config.general.post_capture)
}

/// Map a CLI `PostCaptureAction` to an `OutputAction`.
///
/// For v0.1.0, `Edit` and `Ask` are mapped to `Save(None)` because there is no
/// GUI editor yet.
fn post_action_to_output_action(action: PostCaptureAction) -> OutputAction {
    match action {
        PostCaptureAction::Edit => OutputAction::Save(None),
        PostCaptureAction::Save => OutputAction::Save(None),
        PostCaptureAction::Clipboard => OutputAction::Clipboard,
        PostCaptureAction::Pipe => OutputAction::Pipe,
        PostCaptureAction::Ask => OutputAction::Save(None),
    }
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
/// In v0.1.0 only `--post edit`, `--post ask`, and future modes
/// (`--window`, `--pin`, `--scroll-auto`, `--scroll-manual`) need GUI.
/// All capture modes (`--full`, `--screen`, `--full-all`, `--area`) return
/// `false`.
pub fn needs_gui(cli: &Cli) -> bool {
    if let Some(post) = cli.post
        && matches!(post, PostCaptureAction::Edit | PostCaptureAction::Ask)
    {
        return true;
    }

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

    // --area without coordinates needs interactive GUI selection
    if cli.mode.area.as_ref().is_some_and(|s| s.is_empty()) {
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
        if cli.mode.screen_all {
            crate::capture::output::capture_all_screens(overlay_cursor).await
        } else {
            crate::capture::output::capture_current_screen(overlay_cursor).await
        }
    })?;

    // If --area has coordinates, crop to the specified region.
    let image = if let Some(ref coords) = cli.mode.area {
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
                screen_all: false,
                area: None,
                window: false,
                pin: None,
                scroll_auto: false,
                scroll_manual: false,
            },
            post: None,
            stdout: false,
            output: None,
            exec: None,
            clipboard: false,
            silent: false,
            cursor: false,
            list_outputs: false,
            debug_protocol: false,
            config: None,
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
        cli.silent = true;
        cli
    }

    fn make_cli_with_post(post: PostCaptureAction) -> Cli {
        let mut cli = make_cli_screen();
        cli.post = Some(post);
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
        cli.silent = true;
        let config = Config::default();
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Clipboard),
            "clipboard should win over silent"
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
    fn test_silent_maps_to_save() {
        let cli = make_cli_with_silent();
        let config = Config::default();
        assert!(matches!(determine_output_action(&cli, &config), OutputAction::Save(None)));
    }

    #[test]
    fn test_post_override() {
        let cli = make_cli_with_post(PostCaptureAction::Clipboard);
        let config = Config::default();
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Clipboard),
            "--post clipboard should map to Clipboard"
        );
    }

    #[test]
    fn test_post_edit_maps_to_save_in_v010() {
        let cli = make_cli_with_post(PostCaptureAction::Edit);
        let config = Config::default();
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Save(None)),
            "Edit should map to Save in v0.1.0"
        );
    }

    #[test]
    fn test_post_ask_maps_to_save_in_v010() {
        let cli = make_cli_with_post(PostCaptureAction::Ask);
        let config = Config::default();
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Save(None)),
            "Ask should map to Save in v0.1.0"
        );
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

    #[test]
    fn test_config_default_edit_maps_to_save() {
        let cli = make_cli_screen();
        let config = make_config_with_post_capture("edit");
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Save(None)),
            "config 'edit' should map to Save in v0.1.0"
        );
    }

    #[test]
    fn test_config_default_unknown_maps_to_save() {
        let cli = make_cli_screen();
        let config = make_config_with_post_capture("foobar");
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Save(None)),
            "unknown config value should default to Save"
        );
    }

    #[test]
    fn test_explicit_flags_beat_post() {
        let mut cli = make_cli_with_post(PostCaptureAction::Save);
        cli.stdout = true;
        let config = Config::default();
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Pipe),
            "--stdout should beat --post save"
        );
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
    fn test_needs_gui_screen_all() {
        let mut cli = make_cli_screen();
        cli.mode.screen = false;
        cli.mode.screen_all = true;
        assert!(!needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_area() {
        let mut cli = make_cli_screen();
        cli.mode.screen = false;
        cli.mode.area = Some(String::new());
        assert!(needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_area_with_coords() {
        let mut cli = make_cli_screen();
        cli.mode.screen = false;
        cli.mode.area = Some("100,200,500,400".into());
        assert!(!needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_post_edit() {
        let cli = make_cli_with_post(PostCaptureAction::Edit);
        assert!(needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_post_ask() {
        let cli = make_cli_with_post(PostCaptureAction::Ask);
        assert!(needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_post_save_false() {
        let cli = make_cli_with_post(PostCaptureAction::Save);
        assert!(!needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_post_clipboard_false() {
        let cli = make_cli_with_post(PostCaptureAction::Clipboard);
        assert!(!needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_post_pipe_false() {
        let cli = make_cli_with_post(PostCaptureAction::Pipe);
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
