use std::sync::Arc;

use wlsnap::{
    capture::CapturedImage,
    cli::Cli,
    config::Config,
    output_manager::{OutputAction, dispatch},
};

/// 全局应用状态机
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AppState {
    Idle,
    SelectingRegion,
    SelectingWindow,
    Capturing,
    Editing,
    Scrolling,
    ChoosingAction,
}

/// 后端 → UI 的事件通道
#[derive(Debug)]
pub enum BackendEvent {
    /// Screenshot capture completed successfully.
    CaptureFinished {
        captured: CapturedImage,
        mode: String,
    },
    /// Backend encountered an error.
    Error { msg: String },
}

/// 全局应用结构
#[allow(dead_code)]
pub struct WlsnapApp {
    pub state: AppState,
    pub pin_windows: Vec<()>, // placeholder for PinWindow (not yet implemented)
    pub config: Arc<Config>,
    pub backend_rx: tokio::sync::mpsc::UnboundedReceiver<BackendEvent>,
    pub backend_tx: tokio::sync::mpsc::UnboundedSender<BackendEvent>,
    pub current_output: Option<()>, // placeholder
    pub all_outputs: Vec<()>,       // placeholder

    /// The CLI arguments that triggered this session (for v0.1.0 CLI-driven flow)
    pub cli: Option<Cli>,

    /// Whether the capture has been initiated (to avoid double-spawning)
    capture_initiated: bool,

    /// Whether output has been dispatched (to know when to exit)
    output_dispatched: bool,

    /// Pre-determined output action (stored before cli is taken for capture)
    pending_action: Option<OutputAction>,

    // Keep runtime alive for the duration of the app
    _runtime: tokio::runtime::Runtime,
}

impl WlsnapApp {
    pub fn new(config: Config) -> Self {
        let (backend_tx, backend_rx) = tokio::sync::mpsc::unbounded_channel();
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

        Self {
            state: AppState::Idle,
            pin_windows: Vec::new(),
            config: Arc::new(config),
            backend_rx,
            backend_tx,
            current_output: None,
            all_outputs: Vec::new(),
            cli: None,
            capture_initiated: false,
            output_dispatched: false,
            pending_action: None,
            _runtime: runtime,
        }
    }

    /// Create a new app instance that will be driven by CLI arguments.
    pub fn new_with_cli(config: Config, cli: Cli) -> Self {
        let mut app = Self::new(config);
        app.cli = Some(cli);
        app
    }

    /// Determine the output action based on CLI flags and config.
    ///
    /// Priority (highest first):
    /// 1. `--stdout`  → Pipe
    /// 2. `--clipboard` → Clipboard
    /// 3. `-o PATH` → Save(Some(path))
    /// 4. `general.post_capture` config → default action
    fn determine_output_action(&self) -> OutputAction {
        let cli = self
            .cli
            .as_ref()
            .expect("determine_output_action called without CLI");

        if cli.stdout {
            return OutputAction::Pipe;
        }
        if cli.clipboard {
            return OutputAction::Clipboard;
        }
        if let Some(ref path) = cli.output {
            return OutputAction::Save(Some(path.clone()));
        }

        Self::parse_post_capture_config(&self.config.general.post_capture)
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

    /// Spawn an async tokio task that performs the screenshot capture.
    fn spawn_capture_task(
        &self,
        cli: Cli,
        config: Arc<Config>,
        tx: tokio::sync::mpsc::UnboundedSender<BackendEvent>,
    ) {
        let overlay_cursor = config.advanced.include_cursor || cli.cursor;
        let mode_name = cli.mode.selected_mode_name().to_string();

        self._runtime.spawn(async move {
            let result = if cli.mode.all_screen {
                wlsnap::capture::output::capture_all_screens(overlay_cursor).await
            } else {
                // --screen, --range, --window, or default (no mode) all
                // map to capturing the current screen for v0.1.0.
                wlsnap::capture::output::capture_current_screen(overlay_cursor).await
            };

            match result {
                Ok(captured) => {
                    let _ = tx.send(BackendEvent::CaptureFinished {
                        captured,
                        mode: mode_name,
                    });
                }
                Err(e) => {
                    let _ = tx.send(BackendEvent::Error {
                        msg: format!("{e}"),
                    });
                }
            }
        });
    }
}

impl eframe::App for WlsnapApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ------------------------------------------------------------------
        // 1. Initiate capture if we have CLI args and haven't started yet
        // ------------------------------------------------------------------
        if matches!(self.state, AppState::Idle) && self.cli.is_some() && !self.capture_initiated {
            self.capture_initiated = true;
            self.state = AppState::Capturing;

            // Determine output action BEFORE taking cli, since we need cli data later
            self.pending_action = Some(self.determine_output_action());

            let tx = self.backend_tx.clone();
            let cli = self.cli.take().unwrap();
            let config = self.config.clone();

            self.spawn_capture_task(cli, config, tx);
        }

        // ------------------------------------------------------------------
        // 2. Process backend events
        // ------------------------------------------------------------------
        while let Ok(event) = self.backend_rx.try_recv() {
            match event {
                BackendEvent::CaptureFinished { captured, mode } => {
                    self.state = AppState::Idle;
                    self.output_dispatched = true;

                    let action = self
                        .pending_action
                        .take()
                        .unwrap_or(OutputAction::Save(None));
                    let general_config = &self.config.general;

                    match dispatch(&captured.image, action, general_config, &mode) {
                        Ok(path) => {
                            tracing::info!("Output dispatched: {:?}", path);
                        }
                        Err(e) => {
                            tracing::error!("Output dispatch failed: {}", e);
                            std::process::exit(1);
                        }
                    }

                    // Exit after output is dispatched (v0.1.0 CLI mode)
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                BackendEvent::Error { msg } => {
                    tracing::error!("Backend error: {}", msg);
                    std::process::exit(1);
                }
            }
        }

        // ------------------------------------------------------------------
        // 3. If output was dispatched but we're still running, force close
        // ------------------------------------------------------------------
        if self.output_dispatched {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let state_label = match self.state {
            AppState::Idle => "wlsnap — Idle",
            AppState::SelectingRegion => "wlsnap — Selecting region…",
            AppState::SelectingWindow => "wlsnap — Selecting window…",
            AppState::Capturing => "wlsnap — Capturing…",
            AppState::Editing => "wlsnap — Editing…",
            AppState::Scrolling => "wlsnap — Scrolling…",
            AppState::ChoosingAction => "wlsnap — Choosing action…",
        };

        ui.vertical_centered(|ui| {
            ui.heading(state_label);
            ui.label("v0.1.0 CLI-driven capture");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn make_cli_with_stdout() -> Cli {
        Cli {
            mode: wlsnap::cli::CaptureMode {
                screen: true,
                all_screen: false,
                range: None,
                window: false,
                pin: None,
                scroll_auto: false,
                scroll_manual: false,
            },
            stdout: true,
            output: None,
            clipboard: false,
            cursor: false,
            list_outputs: false,
            debug_protocol: false,
        }
    }

    fn make_cli_with_clipboard() -> Cli {
        Cli {
            mode: wlsnap::cli::CaptureMode {
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
            clipboard: true,
            cursor: false,
            list_outputs: false,
            debug_protocol: false,
        }
    }

    fn make_cli_with_output(path: PathBuf) -> Cli {
        Cli {
            mode: wlsnap::cli::CaptureMode {
                screen: true,
                all_screen: false,
                range: None,
                window: false,
                pin: None,
                scroll_auto: false,
                scroll_manual: false,
            },
            stdout: false,
            output: Some(path),
            clipboard: false,
            cursor: false,
            list_outputs: false,
            debug_protocol: false,
        }
    }

    fn make_app_with_cli(cli: Cli) -> WlsnapApp {
        WlsnapApp::new_with_cli(Config::default(), cli)
    }

    fn make_app_with_config(cli: Cli, config: Config) -> WlsnapApp {
        WlsnapApp::new_with_cli(config, cli)
    }

    #[test]
    fn test_app_new() {
        let config = Config::default();
        let app = WlsnapApp::new(config);
        assert!(matches!(app.state, AppState::Idle));
        assert!(app.pin_windows.is_empty());
        assert!(app.current_output.is_none());
        assert!(app.all_outputs.is_empty());
        assert!(app.cli.is_none());
        assert!(!app.capture_initiated);
        assert!(!app.output_dispatched);
    }

    #[test]
    fn test_app_new_with_cli() {
        let cli = make_cli_with_stdout();
        let app = WlsnapApp::new_with_cli(Config::default(), cli);
        assert!(app.cli.is_some());
        assert!(!app.capture_initiated);
    }

    #[test]
    fn test_state_transitions() {
        let config = Config::default();
        let mut app = WlsnapApp::new(config);

        assert!(matches!(app.state, AppState::Idle));

        app.state = AppState::Capturing;
        assert!(matches!(app.state, AppState::Capturing));

        app.state = AppState::Editing;
        assert!(matches!(app.state, AppState::Editing));
    }

    #[test]
    fn test_backend_event_error() {
        let config = Config::default();
        let app = WlsnapApp::new(config);
        app.backend_tx
            .send(BackendEvent::Error { msg: "test".into() })
            .unwrap();
        // Just verify the channel works
    }

    #[test]
    fn test_backend_event_capture_finished() {
        let config = Config::default();
        let app = WlsnapApp::new(config);
        let captured = CapturedImage {
            image: image::RgbaImage::new(1, 1),
            source_output: wlsnap::platform::output_info::OutputInfo {
                name: "test".into(),
                description: String::new(),
                logical_geometry: wlsnap::platform::output_info::LogicalRect {
                    min: wlsnap::platform::output_info::LogicalPoint { x: 0.0, y: 0.0 },
                    max: wlsnap::platform::output_info::LogicalPoint { x: 1.0, y: 1.0 },
                },
                physical_size: (1, 1),
                scale_factor: 1.0,
                transform: wlsnap::platform::output_info::OutputTransform::Normal,
            },
        };
        app.backend_tx
            .send(BackendEvent::CaptureFinished {
                captured,
                mode: "screen".into(),
            })
            .unwrap();
        // Just verify the channel works
    }

    // ------------------------------------------------------------------
    // determine_output_action tests
    // ------------------------------------------------------------------

    #[test]
    fn test_determine_output_action_stdout_wins() {
        let cli = make_cli_with_stdout();
        let app = make_app_with_cli(cli);
        assert!(matches!(app.determine_output_action(), OutputAction::Pipe));
    }

    #[test]
    fn test_determine_output_action_clipboard_wins_over_save() {
        let cli = make_cli_with_clipboard();
        let app = make_app_with_cli(cli);
        assert!(
            matches!(app.determine_output_action(), OutputAction::Clipboard),
            "clipboard should win over default save"
        );
    }

    #[test]
    fn test_determine_output_action_output_flag_maps_to_save() {
        let cli = make_cli_with_output(PathBuf::from("/tmp/test.png"));
        let app = make_app_with_cli(cli);
        assert!(matches!(
            app.determine_output_action(),
            OutputAction::Save(_)
        ));
    }

    #[test]
    fn test_determine_output_action_config_default_save() {
        let cli = make_cli_with_stdout();
        let mut cli = cli;
        cli.stdout = false;
        let mut config = Config::default();
        config.general.post_capture = "save".into();
        let app = make_app_with_config(cli, config);
        assert!(matches!(
            app.determine_output_action(),
            OutputAction::Save(_)
        ));
    }

    #[test]
    fn test_determine_output_action_config_default_clipboard() {
        let cli = make_cli_with_stdout();
        let mut cli = cli;
        cli.stdout = false;
        let mut config = Config::default();
        config.general.post_capture = "clipboard".into();
        let app = make_app_with_config(cli, config);
        assert!(matches!(
            app.determine_output_action(),
            OutputAction::Clipboard
        ));
    }

    #[test]
    fn test_determine_output_action_config_default_pipe() {
        let cli = make_cli_with_stdout();
        let mut cli = cli;
        cli.stdout = false;
        let mut config = Config::default();
        config.general.post_capture = "pipe".into();
        let app = make_app_with_config(cli, config);
        assert!(matches!(app.determine_output_action(), OutputAction::Pipe));
    }
}
