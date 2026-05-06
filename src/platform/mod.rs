pub mod output_info;
pub mod wayland;

#[allow(unused_imports)]
pub use output_info::{LogicalPoint, LogicalRect, OutputInfo, OutputTransform};
#[allow(unused_imports)]
pub use wayland::enumerate_outputs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexports_are_available() {
        // This test simply verifies that the re-exported types are reachable.
        let _ = LogicalPoint { x: 0.0, y: 0.0 };
        let _ = LogicalRect {
            min: LogicalPoint { x: 0.0, y: 0.0 },
            max: LogicalPoint { x: 1.0, y: 1.0 },
        };
        let _ = OutputTransform::Normal;
    }
}
