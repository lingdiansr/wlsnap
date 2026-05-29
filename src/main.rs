mod app;

use app::WlsnapApp;
use wlsnap::config::Config;

fn main() -> eframe::Result {
    // 1. Initialize tracing
    tracing_subscriber::fmt::init();

    // 2. Load config
    let config = Config::load().unwrap_or_default();

    // 3. Run eframe
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        wlsnap::constants::APP_NAME,
        native_options,
        Box::new(|_cc| Ok(Box::new(WlsnapApp::new(config)))),
    )
}
