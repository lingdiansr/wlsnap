//! eframe-based area selector fallback for compositors that do not support
//! `zwlr_layer_shell_v1` (e.g. GNOME).
//!
//! This module provides [`EframeSelector`], a minimal interactive region picker
//! rendered as a borderless egui window covering the target output.

use egui::{Color32, FontId, Pos2, Rect, Stroke};
use crate::platform::output_info::{LogicalPoint, LogicalRect, OutputInfo};
use std::sync::mpsc;

/// Minimum side length (in logical pixels) for a selection to be considered valid.
const MIN_SELECTION_SIZE: f64 = 10.0;

/// State machine for the eframe area selector.
pub struct EframeSelector {
    /// The output (monitor) on which selection happens.
    pub output: OutputInfo,
    /// Cursor position when the user started dragging (None until first press).
    pub drag_start: Option<Pos2>,
    /// Current cursor position, updated on every pointer move.
    pub drag_current: Pos2,
    /// Final selected region in logical coordinates, set on mouse release.
    pub selected_region: Option<LogicalRect>,
    /// True if the user cancelled the selection (e.g. pressed Escape).
    pub cancelled: bool,
    /// True once the user has confirmed or finished the selection.
    pub done: bool,
}

impl EframeSelector {
    /// Create a new selector targeting the given output.
    pub fn new(output: OutputInfo) -> Self {
        Self {
            output,
            drag_start: None,
            drag_current: Pos2::ZERO,
            selected_region: None,
            cancelled: false,
            done: false,
        }
    }

    /// Run the eframe area selector and return the selected region, if any.
    pub fn run() -> Option<LogicalRect> {
        // 1. Enumerate outputs
        let outputs = match crate::platform::wayland::enumerate_outputs() {
            Ok(o) => o,
            Err(_) => return None,
        };
        if outputs.is_empty() {
            return None;
        }

        // 2. Select output (first for now)
        // TODO: pointer-based output selection
        let output = outputs.into_iter().next().unwrap();

        let (tx, rx) = mpsc::channel();

        let selector = EframeSelector::new(output);
        let app = SelectorApp {
            selector,
            tx: Some(tx),
        };

        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_fullscreen(true)
                .with_always_on_top(),
            ..Default::default()
        };

        // 4. Run event loop (ignore result, we get the value through the channel)
        let _ = eframe::run_native(
            "wlsnap area selector",
            options,
            Box::new(|_cc| Ok(Box::new(app))),
        );

        // 5. Return result
        rx.recv().unwrap_or(None)
    }
}

/// Wrapper that holds [`EframeSelector`] and a channel sender to return the result.
struct SelectorApp {
    selector: EframeSelector,
    tx: Option<mpsc::Sender<Option<LogicalRect>>>,
}

impl eframe::App for SelectorApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.selector.logic(ctx, frame);

        if self.selector.done {
            if let Some(tx) = self.tx.take() {
                let result = if self.selector.cancelled {
                    None
                } else {
                    self.selector.selected_region
                };
                let _ = tx.send(result);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.selector.ui(ui, frame);
    }
}

impl eframe::App for EframeSelector {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Handle Esc key
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cancelled = true;
            self.done = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let _screen_rect = ctx.content_rect();

        // 2. Handle mouse input
        let pointer = ctx.input(|i| i.pointer.clone());

        if pointer.button_pressed(egui::PointerButton::Primary) {
            if let Some(pos) = pointer.interact_pos() {
                self.drag_start = Some(pos);
                self.drag_current = pos;
            }
        }

        if self.drag_start.is_some() {
            if let Some(pos) = pointer.interact_pos() {
                self.drag_current = pos;
            }
        }

        if pointer.button_released(egui::PointerButton::Primary) {
            if let Some(start) = self.drag_start {
                let rect = Rect::from_two_pos(start, self.drag_current);
                let width = rect.width() as f64;
                let height = rect.height() as f64;

                if width >= MIN_SELECTION_SIZE && height >= MIN_SELECTION_SIZE {
                    let min = LogicalPoint {
                        x: (rect.min.x as f64) + self.output.logical_geometry.min.x,
                        y: (rect.min.y as f64) + self.output.logical_geometry.min.y,
                    };
                    let max = LogicalPoint {
                        x: (rect.max.x as f64) + self.output.logical_geometry.min.x,
                        y: (rect.max.y as f64) + self.output.logical_geometry.min.y,
                    };
                    self.selected_region = Some(LogicalRect { min, max });
                }

                self.done = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }

        // Request continuous repaint while dragging
        if self.drag_start.is_some() {
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let screen_rect = ui.ctx().content_rect();

        let painter = ui.painter_at(screen_rect);

        // Full-screen semi-transparent black mask
        painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(128));

        // Draw selection rectangle if dragging
        if let Some(start) = self.drag_start {
            let sel_rect = Rect::from_two_pos(start, self.drag_current);

            // White border
            painter.rect_stroke(
                sel_rect,
                0.0,
                Stroke::new(1.0, Color32::WHITE),
                egui::StrokeKind::Inside,
            );

            // Semi-transparent white fill
            painter.rect_filled(sel_rect, 0.0, Color32::from_white_alpha(64));

            // Size label at bottom-right of selection
            let width = sel_rect.width().abs() as i32;
            let height = sel_rect.height().abs() as i32;
            let label = format!("{} x {}", width, height);
            let galley = painter.layout(
                label,
                FontId::proportional(14.0),
                Color32::WHITE,
                f32::INFINITY,
            );
            let label_pos = Pos2::new(
                sel_rect.max.x - galley.rect.width() - 4.0,
                sel_rect.max.y + 4.0,
            );
            painter.galley(label_pos, galley, Color32::WHITE);
        }

        // Hint text at bottom center
        let hint = "Esc cancel | Drag to select";
        let galley = painter.layout(
            hint.to_string(),
            FontId::proportional(16.0),
            Color32::WHITE,
            f32::INFINITY,
        );
        let hint_pos = Pos2::new(
            screen_rect.center().x - galley.rect.width() / 2.0,
            screen_rect.max.y - 40.0,
        );
        painter.galley(hint_pos, galley, Color32::WHITE);
    }
}
