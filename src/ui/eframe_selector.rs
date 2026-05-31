//! eframe-based area selector fallback for compositors that do not support
//! `zwlr_layer_shell_v1` (e.g. GNOME).
//!
//! This module provides [`EframeSelector`], a minimal interactive region picker
//! rendered as a borderless egui window covering the target output.

use egui::{Color32, FontId, Pos2, Rect, Stroke, Vec2};
use crate::platform::output_info::{LogicalPoint, LogicalRect, OutputInfo};

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
}
