# CLI Headless Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** All screenshot capture modes (`--full`, `--screen`, `--full-all`, `--area`) run without creating a GUI window, avoiding self-capture on Niri and other compositors. GUI is only needed for system tray, toolbar, editor, scrolling stitch, and interactive post-capture.

**Architecture:** Extract output-action determination + capture execution from `WlsnapApp` into a standalone `src/cli_action.rs`. In `main.rs`, detect if CLI mode needs GUI (only `--post edit`/`--post ask` and future interactive features). If not needed, bypass `eframe` entirely — run capture + dispatch in a blocking tokio runtime and exit. GUI mode still uses `eframe` + `WlsnapApp`.

**Tech Stack:** Rust, tokio, eframe/egui, wlr-screencopy

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/cli_action.rs` (new) | Standalone `determine_output_action(cli, config)` + `run_cli_capture(cli, config)` — pure logic, no GUI dependency |
| `src/main.rs` (modify) | After CLI parse, check `needs_gui()`. If false → call `run_cli_capture` and exit. If true → launch `eframe` |
| `src/app.rs` (modify) | Remove `determine_output_action`, `post_action_to_output_action`, `parse_post_capture_config` — import from `cli_action`. Keep `WlsnapApp` for GUI mode only. Update tests to use `cli_action` functions. |
| `src/lib.rs` (modify) | Add `pub mod cli_action;` |

---

## Task 1: Create `src/cli_action.rs`

**Files:**
- Create: `src/cli_action.rs`

- [ ] **Step 1: Write the module**

```rust
//! Standalone CLI action determination and headless capture execution.
//!
//! This module contains pure logic (no GUI dependency) for:
//! - Mapping CLI flags + config to `OutputAction`
//! - Running capture + output dispatch in a blocking tokio runtime

use std::path::PathBuf;

use crate::cli::{Cli, PostCaptureAction};
use crate::config::Config;
use crate::error::Result;
use crate::output_manager::{OutputAction, dispatch};

/// Determine the output action based on CLI flags and config.
///
/// Priority (highest first):
/// 1. `--stdout`  → Pipe
/// 2. `--exec CMD` → Exec(cmd)
/// 3. `--clipboard` → Clipboard
/// 4. `-o PATH` → Save(Some(path))
/// 5. `--silent` → Save(None)
/// 6. `--post ACTION` → override config
/// 7. `general.post_capture` config → default action
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
/// For v0.1.0, `Edit` and `Ask` are mapped to `Save` because there is no
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
        "save" => OutputAction::Save(None),
        "clipboard" => OutputAction::Clipboard,
        "pipe" => OutputAction::Pipe,
        "edit" | "ask" => OutputAction::Save(None),
        _ => OutputAction::Save(None),
    }
}

/// Check whether the given CLI arguments require a GUI window.
///
/// GUI is needed for:
/// - `--post edit` or `--post ask` (interactive)
/// - `--window` (future interactive window selection)
/// - `--pin` (future pin window)
/// - `--scroll-auto` / `--scroll-manual` (future scrolling capture)
///
/// GUI is NOT needed for:
/// - `--full`, `--screen`, `--full-all`, `--area`
/// - Any non-interactive post-capture action
pub fn needs_gui(cli: &Cli) -> bool {
    // Interactive post-capture actions need GUI
    if let Some(post) = cli.post {
        if matches!(post, PostCaptureAction::Edit | PostCaptureAction::Ask) {
            return true;
        }
    }

    // Future interactive modes (not yet implemented, but guard against them)
    if cli.mode.window || cli.mode.pin.is_some() || cli.mode.scroll_auto || cli.mode.scroll_manual
    {
        return true;
    }

    // All capture modes (full, screen, full_all, area) are non-interactive in v0.1.0
    false
}

/// Run a headless (no-GUI) capture + output dispatch.
///
/// This function blocks the current thread while running an async tokio runtime
/// internally.
pub fn run_cli_capture(cli: &Cli, config: &Config) -> Result<PathBuf> {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    let overlay_cursor = config.advanced.include_cursor || cli.cursor;
    let mode_name = cli.mode.selected_mode_name().to_string();
    let action = determine_output_action(cli, config);

    rt.block_on(async move {
        let captured = if cli.mode.full_all {
            crate::capture::output::capture_all_screens(overlay_cursor).await?
        } else {
            // --full, --screen, --area, or default (no mode) all map to
            // capturing the current screen for v0.1.0.
            crate::capture::output::capture_current_screen(overlay_cursor).await?
        };

        dispatch(&captured.image, action, &config.general, &mode_name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_cli(mode_fn: impl FnOnce(&mut crate::cli::CaptureMode)) -> Cli {
        let mut cli = Cli {
            mode: crate::cli::CaptureMode {
                full: false,
                full_all: false,
                area: false,
                window: false,
                output: false,
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
        };
        mode_fn(&mut cli.mode);
        cli
    }

    #[test]
    fn test_determine_output_action_stdout_wins() {
        let mut cli = make_cli(|m| m.full = true);
        cli.stdout = true;
        let config = Config::default();
        assert!(matches!(
            determine_output_action(&cli, &config),
            OutputAction::Pipe
        ));
    }

    #[test]
    fn test_determine_output_action_exec_wins_over_clipboard() {
        let mut cli = make_cli(|m| m.full = true);
        cli.exec = Some("echo {file}".into());
        cli.clipboard = true;
        let config = Config::default();
        match determine_output_action(&cli, &config) {
            OutputAction::Exec(cmd) => assert_eq!(cmd, "echo {file}"),
            other => panic!("expected Exec, got {:?}", other),
        }
    }

    #[test]
    fn test_determine_output_action_clipboard_wins_over_save() {
        let mut cli = make_cli(|m| m.full = true);
        cli.clipboard = true;
        cli.silent = true;
        let config = Config::default();
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Clipboard),
            "clipboard should win over silent"
        );
    }

    #[test]
    fn test_determine_output_action_output_flag_with_path() {
        let mut cli = make_cli(|m| m.full = true);
        cli.output = Some(PathBuf::from("/tmp/test.png"));
        let config = Config::default();
        match determine_output_action(&cli, &config) {
            OutputAction::Save(Some(p)) => assert_eq!(p, PathBuf::from("/tmp/test.png")),
            other => panic!("expected Save(Some(...)), got {:?}", other),
        }
    }

    #[test]
    fn test_determine_output_action_silent_maps_to_save_none() {
        let mut cli = make_cli(|m| m.full = true);
        cli.silent = true;
        let config = Config::default();
        match determine_output_action(&cli, &config) {
            OutputAction::Save(None) => {}
            other => panic!("expected Save(None), got {:?}", other),
        }
    }

    #[test]
    fn test_determine_output_action_post_override() {
        let mut cli = make_cli(|m| m.full = true);
        cli.post = Some(PostCaptureAction::Clipboard);
        let config = Config::default();
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Clipboard),
            "--post clipboard should map to Clipboard"
        );
    }

    #[test]
    fn test_determine_output_action_post_edit_maps_to_save() {
        let mut cli = make_cli(|m| m.full = true);
        cli.post = Some(PostCaptureAction::Edit);
        let config = Config::default();
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Save(None)),
            "Edit should map to Save(None) in v0.1.0"
        );
    }

    #[test]
    fn test_determine_output_action_config_default_save() {
        let cli = make_cli(|m| m.full = true);
        let mut config = Config::default();
        config.general.post_capture = "save".into();
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Save(None))
        );
    }

    #[test]
    fn test_determine_output_action_config_default_clipboard() {
        let cli = make_cli(|m| m.full = true);
        let mut config = Config::default();
        config.general.post_capture = "clipboard".into();
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Clipboard)
        );
    }

    #[test]
    fn test_determine_output_action_config_default_pipe() {
        let cli = make_cli(|m| m.full = true);
        let mut config = Config::default();
        config.general.post_capture = "pipe".into();
        assert!(matches!(determine_output_action(&cli, &config), OutputAction::Pipe));
    }

    #[test]
    fn test_determine_output_action_explicit_flags_beat_post() {
        let mut cli = make_cli(|m| m.full = true);
        cli.post = Some(PostCaptureAction::Save);
        cli.stdout = true;
        let config = Config::default();
        assert!(
            matches!(determine_output_action(&cli, &config), OutputAction::Pipe),
            "--stdout should beat --post save"
        );
    }

    #[test]
    fn test_needs_gui_full_is_false() {
        let cli = make_cli(|m| m.full = true);
        assert!(!needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_area_is_false() {
        let cli = make_cli(|m| m.area = true);
        assert!(!needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_post_edit_is_true() {
        let mut cli = make_cli(|m| m.full = true);
        cli.post = Some(PostCaptureAction::Edit);
        assert!(needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_post_ask_is_true() {
        let mut cli = make_cli(|m| m.full = true);
        cli.post = Some(PostCaptureAction::Ask);
        assert!(needs_gui(&cli));
    }

    #[test]
    fn test_needs_gui_window_is_true() {
        let cli = make_cli(|m| m.window = true);
        assert!(needs_gui(&cli));
    }
}
