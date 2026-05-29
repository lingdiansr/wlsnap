#![allow(dead_code)]

use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalRect {
    pub min: LogicalPoint,
    pub max: LogicalPoint,
}

/// Information about a Wayland output (monitor).
#[derive(Debug, Clone, PartialEq)]
pub struct OutputInfo {
    /// Output name, e.g. "DP-1".
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Position and size in the global logical coordinate space.
    pub logical_geometry: LogicalRect,
    /// Physical pixel dimensions (width, height) of the current mode.
    pub physical_size: (u32, u32),
    /// Logical scale factor (e.g. 1.0, 1.5, 2.0).
    pub scale_factor: f64,
    /// Current output transform.
    pub transform: OutputTransform,
}

/// Possible output transforms as defined by the Wayland protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTransform {
    Normal,
    Rotated90,
    Rotated180,
    Rotated270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

impl OutputTransform {
    /// Apply transform to physical dimensions, returning logical-oriented dimensions.
    ///
    /// 90° and 270° rotations swap width and height; all other transforms keep them.
    pub fn apply(&self, width: u32, height: u32) -> (u32, u32) {
        match self {
            OutputTransform::Normal
            | OutputTransform::Rotated180
            | OutputTransform::Flipped
            | OutputTransform::Flipped180 => (width, height),
            OutputTransform::Rotated90
            | OutputTransform::Rotated270
            | OutputTransform::Flipped90
            | OutputTransform::Flipped270 => (height, width),
        }
    }
}

impl From<wayland_client::protocol::wl_output::Transform> for OutputTransform {
    fn from(value: wayland_client::protocol::wl_output::Transform) -> Self {
        use wayland_client::protocol::wl_output::Transform as WlTransform;
        match value {
            WlTransform::Normal => OutputTransform::Normal,
            WlTransform::_90 => OutputTransform::Rotated90,
            WlTransform::_180 => OutputTransform::Rotated180,
            WlTransform::_270 => OutputTransform::Rotated270,
            WlTransform::Flipped => OutputTransform::Flipped,
            WlTransform::Flipped90 => OutputTransform::Flipped90,
            WlTransform::Flipped180 => OutputTransform::Flipped180,
            WlTransform::Flipped270 => OutputTransform::Flipped270,
            _ => {
                warn!(
                    "Unknown wl_output transform {:?}, defaulting to Normal",
                    value
                );
                OutputTransform::Normal
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_transform_apply_all_variants() {
        let width = 1920_u32;
        let height = 1080_u32;

        assert_eq!(OutputTransform::Normal.apply(width, height), (1920, 1080));
        assert_eq!(
            OutputTransform::Rotated90.apply(width, height),
            (1080, 1920)
        );
        assert_eq!(
            OutputTransform::Rotated180.apply(width, height),
            (1920, 1080)
        );
        assert_eq!(
            OutputTransform::Rotated270.apply(width, height),
            (1080, 1920)
        );
        assert_eq!(OutputTransform::Flipped.apply(width, height), (1920, 1080));
        assert_eq!(
            OutputTransform::Flipped90.apply(width, height),
            (1080, 1920)
        );
        assert_eq!(
            OutputTransform::Flipped180.apply(width, height),
            (1920, 1080)
        );
        assert_eq!(
            OutputTransform::Flipped270.apply(width, height),
            (1080, 1920)
        );
    }

    #[test]
    fn output_transform_apply_square() {
        let size = 1440_u32;
        // For square dimensions all transforms should return the same size.
        for t in [
            OutputTransform::Normal,
            OutputTransform::Rotated90,
            OutputTransform::Rotated180,
            OutputTransform::Rotated270,
            OutputTransform::Flipped,
            OutputTransform::Flipped90,
            OutputTransform::Flipped180,
            OutputTransform::Flipped270,
        ] {
            assert_eq!(t.apply(size, size), (size, size), "Failed for {:?}", t);
        }
    }

    #[test]
    fn logical_rect_equality() {
        let a = LogicalRect {
            min: LogicalPoint { x: 0.0, y: 0.0 },
            max: LogicalPoint {
                x: 1920.0,
                y: 1080.0,
            },
        };
        let b = LogicalRect {
            min: LogicalPoint { x: 0.0, y: 0.0 },
            max: LogicalPoint {
                x: 1920.0,
                y: 1080.0,
            },
        };
        assert_eq!(a, b);
    }
}
