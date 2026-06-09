mod app;

use app::WlsnapApp;
use clap::Parser;
use wlsnap::{cli::Cli, cli_action, config::Config};

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

    // 6. Interactive --range: run selector (layer-shell or eframe fallback)
    if cli.mode.range {
        let probe = wlsnap::backend::probe_all();
        let region = if probe.has_layer_shell() {
            wlsnap::ui::layer_selector::LayerSelector::run()
        } else {
            wlsnap::ui::eframe_selector::EframeSelector::run()
        };

        match region {
            Some(region) => {
                tracing::debug!(
                    "Selected region: min=({:.1},{:.1}) max=({:.1},{:.1})",
                    region.min.x,
                    region.min.y,
                    region.max.x,
                    region.max.y
                );

                // Determine the output on which the selector ran (focused output)
                let outputs = match wlsnap::platform::wayland::enumerate_outputs() {
                    Ok(o) => o,
                    Err(e) => {
                        tracing::error!("Failed to enumerate outputs: {}", e);
                        std::process::exit(1);
                    }
                };

                // Determine which output the selector ran on.
                // The selector returns local coordinates (relative to the output).
                // Find the output whose logical size can contain the region.
                tracing::debug!(
                    "-- Output matching for region max=({:.1},{:.1}) --",
                    region.max.x,
                    region.max.y
                );
                let mut matched_output = None;
                for o in &outputs {
                    let log_w = o.logical_geometry.max.x - o.logical_geometry.min.x;
                    let log_h = o.logical_geometry.max.y - o.logical_geometry.min.y;
                    let matches = region.max.x <= log_w && region.max.y <= log_h;
                    tracing::debug!(
                        "  output='{}' log_w={:.1} log_h={:.1} match_x={} match_y={} MATCH={}",
                        o.name,
                        log_w,
                        log_h,
                        region.max.x <= log_w,
                        region.max.y <= log_h,
                        matches
                    );
                    if matches && matched_output.is_none() {
                        matched_output = Some(o.clone());
                    }
                }

                let output = matched_output.or_else(|| {
                    let focused_name = wlsnap::platform::wayland::get_focused_output_name();
                    tracing::debug!(
                        "  size-match failed, trying focused_output={:?}",
                        focused_name
                    );
                    if let Some(name) = focused_name {
                        outputs.iter().find(|o| o.name == name).cloned()
                    } else {
                        outputs.first().cloned()
                    }
                });

                let Some(output) = output else {
                    tracing::error!("No outputs available for capture");
                    return Ok(());
                };

                tracing::debug!(
                    "Capturing output: {} logical={:?} physical={:?} scale={}",
                    output.name,
                    output.logical_geometry,
                    output.physical_size,
                    output.scale_factor
                );

                // Capture the specific output and crop to the selected region
                let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
                let overlay_cursor = config.advanced.include_cursor || cli.cursor;

                let captured = rt.block_on(async {
                    wlsnap::capture::output::capture_specific_output(&output, overlay_cursor).await
                });

                match captured {
                    Ok(captured) => {
                        tracing::debug!(
                            "Captured image size: {}x{}",
                            captured.image.width(),
                            captured.image.height()
                        );
                        let cropped =
                            wlsnap::capture::region::crop_image(&captured.image, &region, &output);
                        tracing::debug!(
                            "Cropped image size: {}x{}",
                            cropped.width(),
                            cropped.height()
                        );

                        // If crop results in zero-size image, fall back to full capture
                        let image_to_dispatch = if cropped.width() == 0 || cropped.height() == 0 {
                            tracing::warn!(
                                "Crop produced zero-size image ({}x{}), falling back to full screen",
                                cropped.width(),
                                cropped.height()
                            );
                            &captured.image
                        } else {
                            &cropped
                        };

                        let action = cli_action::determine_output_action(&cli, &config);
                        let mode_name = "range";
                        match wlsnap::output_manager::dispatch(
                            image_to_dispatch,
                            action,
                            &config.general,
                            mode_name,
                        ) {
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
                    Err(e) => {
                        tracing::error!("Capture failed: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            None => {
                tracing::info!("Range selection cancelled.");
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
