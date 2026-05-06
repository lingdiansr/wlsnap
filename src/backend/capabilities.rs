use bitflags::bitflags;

bitflags! {
    /// Bitflags representing the capture capabilities of a backend.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CaptureCapabilities: u32 {
        /// Fullscreen capture of the current screen.
        const FULLSCREEN    = 1 << 0;
        /// Region / area selection capture.
        const REGION        = 1 << 1;
        /// Single-window capture.
        const WINDOW        = 1 << 2;
        /// Specific output capture.
        const OUTPUT        = 1 << 3;
        /// Automatic scrolling capture.
        const SCROLL_AUTO   = 1 << 4;
        /// Manual scrolling capture.
        const SCROLL_MANUAL = 1 << 5;
        /// Include cursor in capture.
        const CURSOR        = 1 << 6;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitflags_or_combination() {
        let caps = CaptureCapabilities::FULLSCREEN | CaptureCapabilities::REGION;
        assert!(caps.contains(CaptureCapabilities::FULLSCREEN));
        assert!(caps.contains(CaptureCapabilities::REGION));
        assert!(!caps.contains(CaptureCapabilities::WINDOW));
    }

    #[test]
    fn bitflags_and_intersection() {
        let caps = CaptureCapabilities::FULLSCREEN
            | CaptureCapabilities::REGION
            | CaptureCapabilities::CURSOR;
        let intersection = caps & CaptureCapabilities::FULLSCREEN;
        assert_eq!(intersection, CaptureCapabilities::FULLSCREEN);

        let no_match = caps & CaptureCapabilities::WINDOW;
        assert!(no_match.is_empty());
    }

    #[test]
    fn bitflags_all_individual_flags() {
        let all = CaptureCapabilities::FULLSCREEN
            | CaptureCapabilities::REGION
            | CaptureCapabilities::WINDOW
            | CaptureCapabilities::OUTPUT
            | CaptureCapabilities::SCROLL_AUTO
            | CaptureCapabilities::SCROLL_MANUAL
            | CaptureCapabilities::CURSOR;

        assert!(all.contains(CaptureCapabilities::FULLSCREEN));
        assert!(all.contains(CaptureCapabilities::REGION));
        assert!(all.contains(CaptureCapabilities::WINDOW));
        assert!(all.contains(CaptureCapabilities::OUTPUT));
        assert!(all.contains(CaptureCapabilities::SCROLL_AUTO));
        assert!(all.contains(CaptureCapabilities::SCROLL_MANUAL));
        assert!(all.contains(CaptureCapabilities::CURSOR));
    }
}
