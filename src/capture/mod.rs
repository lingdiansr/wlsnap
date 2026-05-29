//! Screenshot capture orchestration.
//!
//! Coordinates between the platform layer (output enumeration), backend
//! (wlr-screencopy), and output manager (save/clipboard/pipe/exec).

pub mod output;
pub mod region;

use crate::platform::output_info::OutputInfo;
use image::RgbaImage;

/// A captured screenshot with metadata.
#[derive(Debug, Clone)]
pub struct CapturedImage {
    pub image: RgbaImage,
    pub source_output: OutputInfo,
}

/// Determines which output(s) to capture based on the current pointer position.
pub fn current_output(
    outputs: &[OutputInfo],
    pointer_pos: Option<(f64, f64)>,
) -> Option<OutputInfo> {
    if let Some((px, py)) = pointer_pos {
        for output in outputs {
            let geom = &output.logical_geometry;
            if px >= geom.min.x && px < geom.max.x && py >= geom.min.y && py < geom.max.y {
                return Some(output.clone());
            }
        }
    }
    outputs.first().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::output_info::{LogicalPoint, LogicalRect};

    fn make_output(name: &str, x: f64, y: f64, w: f64, h: f64) -> OutputInfo {
        OutputInfo {
            name: name.to_string(),
            description: String::new(),
            logical_geometry: LogicalRect {
                min: LogicalPoint { x, y },
                max: LogicalPoint { x: x + w, y: y + h },
            },
            physical_size: (w as u32, h as u32),
            scale_factor: 1.0,
            transform: crate::platform::output_info::OutputTransform::Normal,
        }
    }

    #[test]
    fn current_output_with_pointer_inside() {
        let outputs = vec![make_output("DP-1", 0.0, 0.0, 1920.0, 1080.0)];
        let result = current_output(&outputs, Some((100.0, 200.0)));
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "DP-1");
    }

    #[test]
    fn current_output_with_pointer_outside() {
        let outputs = vec![make_output("DP-1", 0.0, 0.0, 1920.0, 1080.0)];
        let result = current_output(&outputs, Some((2000.0, 2000.0)));
        assert!(result.is_some()); // falls back to first
        assert_eq!(result.unwrap().name, "DP-1");
    }

    #[test]
    fn current_output_no_pointer() {
        let outputs = vec![make_output("DP-1", 0.0, 0.0, 1920.0, 1080.0)];
        let result = current_output(&outputs, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "DP-1");
    }

    #[test]
    fn current_output_empty_list() {
        let outputs: Vec<OutputInfo> = vec![];
        let result = current_output(&outputs, Some((100.0, 100.0)));
        assert!(result.is_none());
    }

    #[test]
    fn current_output_multi_screen_pointer_on_second() {
        let outputs = vec![
            make_output("DP-1", 0.0, 0.0, 1920.0, 1080.0),
            make_output("DP-2", 1920.0, 0.0, 1920.0, 1080.0),
        ];
        let result = current_output(&outputs, Some((2000.0, 500.0)));
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "DP-2");
    }

    #[test]
    fn current_output_multi_screen_pointer_on_edge() {
        // Point exactly on min edge (inclusive)
        let outputs = vec![make_output("DP-1", 0.0, 0.0, 1920.0, 1080.0)];
        let result = current_output(&outputs, Some((0.0, 0.0)));
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "DP-1");

        // Point exactly on max edge (exclusive for x and y)
        let result = current_output(&outputs, Some((1920.0, 1080.0)));
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "DP-1");
    }
}
