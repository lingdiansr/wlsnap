//! Capture backend abstraction layer.
//!
//! This module provides protocol probing, capability flags, and the
//! [`CaptureBackend`] trait that concrete backends (wlr-screencopy,
//! xdg-desktop-portal, ext-image-copy-capture) will implement.

mod capabilities;
mod protocol;

pub use capabilities::CaptureCapabilities;
pub use protocol::ProtocolProbe;

/// Placeholder trait for screenshot backends.
///
/// Async capture methods (`capture_current_screen`, `capture_region`, etc.)
/// will be added in later tasks once the individual backend implementations
/// are in place.
pub trait CaptureBackend: Send + Sync {
    /// Human-readable backend name, e.g. `"wlr-screencopy-unstable-v1"`.
    fn name(&self) -> &'static str;

    /// Supported capture capabilities.
    fn capabilities(&self) -> CaptureCapabilities;
}

/// Convenience function: probe all protocols and return a [`ProtocolProbe`].
///
/// This is equivalent to [`ProtocolProbe::new()`].
pub fn probe_all() -> ProtocolProbe {
    ProtocolProbe::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_all_returns_probe() {
        let probe = probe_all();
        // The probe must always be constructible.
        let _ = probe.recommended_backend();
    }
}
