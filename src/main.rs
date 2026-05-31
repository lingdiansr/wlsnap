mod app;

use app::WlsnapApp;
use clap::Parser;
use wlsnap::cli::Cli;
use wlsnap::cli_action;
use wlsnap::config::Config;

fn main() -> eframe::Result {
    // 1. Initialize tracing
    tracing_subscriber::fmt::init();

    // 2. Parse CLI arguments
    let cli = Cli::parse();

    // 3. Handle immediate-exit flags
    if cli.list_outputs {
        match wlsnap::platform::wayland::enumerate_outputs() {
            Ok(outputs) => {
                if outputs.is_empty() {
                    println!("No outputs detected.");
                } else {
                    for (i, out) in outputs.iter().enumerate() {
                        println!(
                            "{}: {} ({}x{} @ {:?})",
                            i,
                            out.name,
                            out.physical_size.0,
                            out.physical_size.1,
                            out.logical_geometry.min
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to enumerate outputs: {}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    if cli.debug_protocol {
        let probe = wlsnap::backend::probe_all();
        println!("{:#?}", probe);
        return Ok(());
    }

    // 4. Load config
    let config = Config::load().unwrap_or_default();

    // 5. If this is a pure CLI mode (no GUI needed), run headless and exit
    if !cli_action::needs_gui(&cli) {
        match cli_action::run_cli_capture(&cli, &config) {
            Ok(path) => {
                tracing::info!("Output dispatched: {:?}", path);
                return Ok(());
            }
            Err(e) => {
                tracing::error!("Output dispatch failed: {}", e);
                std::process::exit(1);
            }
        }
    }

    // 6. Interactive --area without coordinates: run selector (layer-shell or eframe fallback)
    if cli.mode.area.as_ref().is_some_and(|s| s.is_empty()) {
        let probe = wlsnap::backend::probe_all();
        let region = if probe.has_layer_shell() {
            wlsnap::ui::layer_selector::LayerSelector::run()
        } else {
            wlsnap::ui::eframe_selector::EframeSelector::run()
        };

        match region {
            Some(region) => {
                let mut cli = cli;
                cli.mode.area = Some(format!(
                    "{},{},{},{}",
                    region.min.x.round() as i64,
                    region.min.y.round() as i64,
                    (region.max.x - region.min.x).round() as i64,
                    (region.max.y - region.min.y).round() as i64,
                ));
                match cli_action::run_cli_capture(&cli, &config) {
                    Ok(path) => {
                        tracing::info!("Output dispatched: {:?}", path);
                        return Ok(());
                    }
                    Err(e) => {
                        tracing::error!("Output dispatch failed: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            None => {
                tracing::info!("Area selection cancelled.");
                return Ok(());
            }
        }
    }

    // 7. GUI mode: run eframe with CLI-driven app
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 100.0])
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native(
        wlsnap::constants::APP_NAME,
        native_options,
        Box::new(|_cc| Ok(Box::new(WlsnapApp::new_with_cli(config, cli)))),
    )
}
