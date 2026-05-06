use crate::config::Config;
use std::sync::Arc;

/// 全局应用状态机
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone)]
pub enum BackendEvent {
    CaptureFinished,
    Error { msg: String },
}

/// 全局应用结构
pub struct WlsnapApp {
    pub state: AppState,
    pub pin_windows: Vec<()>, // placeholder for PinWindow (not yet implemented)
    pub config: Arc<Config>,
    pub backend_rx: tokio::sync::mpsc::UnboundedReceiver<BackendEvent>,
    pub backend_tx: tokio::sync::mpsc::UnboundedSender<BackendEvent>,
    pub current_output: Option<()>, // placeholder
    pub all_outputs: Vec<()>,      // placeholder
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
            _runtime: runtime,
        }
    }
}

impl eframe::App for WlsnapApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process backend events
        while let Ok(event) = self.backend_rx.try_recv() {
            match event {
                BackendEvent::CaptureFinished => {
                    self.state = AppState::Editing;
                }
                BackendEvent::Error { msg } => {
                    tracing::error!("Backend error: {msg}");
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let state_label = match self.state {
            AppState::Idle => "State: Idle",
            AppState::SelectingRegion => "State: SelectingRegion",
            AppState::SelectingWindow => "State: SelectingWindow",
            AppState::Capturing => "State: Capturing",
            AppState::Editing => "State: Editing",
            AppState::Scrolling => "State: Scrolling",
            AppState::ChoosingAction => "State: ChoosingAction",
        };
        ui.heading(state_label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_new() {
        let config = Config::default();
        let app = WlsnapApp::new(config);
        assert!(matches!(app.state, AppState::Idle));
        assert!(app.pin_windows.is_empty());
        assert!(app.current_output.is_none());
        assert!(app.all_outputs.is_empty());
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
            .send(BackendEvent::Error {
                msg: "test".into(),
            })
            .unwrap();
        // Just verify the channel works
    }

    #[test]
    fn test_backend_event_capture_finished() {
        let config = Config::default();
        let app = WlsnapApp::new(config);
        app.backend_tx.send(BackendEvent::CaptureFinished).unwrap();
        // Just verify the channel works
    }
}
