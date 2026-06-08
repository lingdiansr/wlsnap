use smithay_client_toolkit::registry::RegistryState;
use tracing::{debug, warn};
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol::wl_registry::WlRegistry,
};

// Wayland protocol interface names used for probing.
const EXT_IMAGE_COPY_CAPTURE: &str = "ext_image_copy_capture_manager_v1";
const WLR_SCREENCOPY: &str = "zwlr_screencopy_manager_v1";
const WLR_VIRTUAL_POINTER: &str = "zwlr_virtual_pointer_manager_v1";
const WLR_LAYER_SHELL: &str = "zwlr_layer_shell_v1";

/// Minimal state required by `wayland_client::globals::registry_queue_init`.
struct ProbeState;

impl Dispatch<WlRegistry, GlobalListContents> for ProbeState {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as wayland_client::Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

/// Cached result of a Wayland protocol availability probe.
///
/// Construct this once at startup and reuse it to avoid repeated
/// round-trips with the compositor.
#[derive(Debug, Clone)]
pub struct ProtocolProbe {
    has_ext_image_copy_capture: bool,
    has_wlr_screencopy: bool,
    has_virtual_pointer: bool,
    has_layer_shell: bool,
    has_portal: bool,
}

impl Default for ProtocolProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtocolProbe {
    /// Probe the current session for available capture protocols.
    ///
    /// If `WAYLAND_DISPLAY` is not set, all Wayland-specific protocols
    /// are reported as unavailable. Portal availability is checked
    /// independently via D-Bus.
    pub fn new() -> Self {
        Self::probe()
    }

    fn probe() -> Self {
        let mut probe = Self {
            has_ext_image_copy_capture: false,
            has_wlr_screencopy: false,
            has_virtual_pointer: false,
            has_layer_shell: false,
            has_portal: check_portal(),
        };

        if std::env::var("WAYLAND_DISPLAY").is_err() {
            warn!("WAYLAND_DISPLAY not set; all Wayland protocols unavailable");
            return probe;
        }

        let conn = match Connection::connect_to_env() {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to connect to Wayland: {e}");
                return probe;
            }
        };

        let (globals, _event_queue) = match registry_queue_init::<ProbeState>(&conn) {
            Ok(g) => g,
            Err(e) => {
                warn!("Failed to initialize Wayland registry: {e}");
                return probe;
            }
        };

        let registry_state = RegistryState::new(&globals);
        for global in registry_state.globals() {
            debug!("Wayland global: {} v{}", global.interface, global.version);
            match global.interface.as_str() {
                EXT_IMAGE_COPY_CAPTURE => probe.has_ext_image_copy_capture = true,
                WLR_SCREENCOPY => probe.has_wlr_screencopy = true,
                WLR_VIRTUAL_POINTER => probe.has_virtual_pointer = true,
                WLR_LAYER_SHELL => probe.has_layer_shell = true,
                _ => {}
            }
        }

        probe
    }

    /// `ext-image-copy-capture-v1` is available.
    pub fn has_ext_image_copy_capture(&self) -> bool {
        self.has_ext_image_copy_capture
    }

    /// `wlr-screencopy-unstable-v1` is available.
    pub fn has_wlr_screencopy(&self) -> bool {
        self.has_wlr_screencopy
    }

    /// `wlr-virtual-pointer-unstable-v1` is available.
    pub fn has_virtual_pointer(&self) -> bool {
        self.has_virtual_pointer
    }

    /// `zwlr_layer_shell_v1` is available.
    pub fn has_layer_shell(&self) -> bool {
        self.has_layer_shell
    }

    /// xdg-desktop-portal is reachable over D-Bus.
    pub fn has_portal(&self) -> bool {
        self.has_portal
    }

    /// Returns the name of the recommended backend based on detected protocols.
    ///
    /// Priority (highest first):
    /// 1. `ext-image-copy-capture-v1`
    /// 2. `xdg-desktop-portal`
    /// 3. `wlr-screencopy-unstable-v1`
    ///
    /// If none are available, returns `"none"`.
    pub fn recommended_backend(&self) -> &'static str {
        if self.has_ext_image_copy_capture {
            "ext-image-copy-capture-v1"
        } else if self.has_portal {
            "xdg-desktop-portal"
        } else if self.has_wlr_screencopy {
            "wlr-screencopy-unstable-v1"
        } else {
            "none"
        }
    }
}

/// Try to ping the xdg-desktop-portal D-Bus service.
///
/// Spawns a temporary current-thread tokio runtime to perform the async check.
fn check_portal() -> bool {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map(|rt| {
            rt.block_on(async {
                match ashpd::zbus::Connection::session().await {
                    Ok(conn) => conn
                        .call_method(
                            Some("org.freedesktop.portal.Desktop"),
                            "/org/freedesktop/portal/desktop",
                            Some("org.freedesktop.DBus.Peer"),
                            "Ping",
                            &(),
                        )
                        .await
                        .is_ok(),
                    Err(_) => false,
                }
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ProtocolProbe` must be constructible even when no Wayland display is present.
    #[test]
    fn probe_constructible_without_wayland() {
        let old_display = std::env::var_os("WAYLAND_DISPLAY");
        let old_socket = std::env::var_os("WAYLAND_SOCKET");
        unsafe {
            std::env::set_var("WAYLAND_DISPLAY", "__nonexistent_display__");
            std::env::remove_var("WAYLAND_SOCKET");
        }
        let probe = ProtocolProbe::new();
        if let Some(v) = old_display {
            unsafe {
                std::env::set_var("WAYLAND_DISPLAY", v);
            }
        } else {
            unsafe {
                std::env::remove_var("WAYLAND_DISPLAY");
            }
        }
        if let Some(v) = old_socket {
            unsafe {
                std::env::set_var("WAYLAND_SOCKET", v);
            }
        }
        assert!(!probe.has_ext_image_copy_capture());
        assert!(!probe.has_wlr_screencopy());
        assert!(!probe.has_virtual_pointer());
        assert!(!probe.has_layer_shell());
    }

    /// When multiple backends are "present", the highest-priority one is recommended.
    #[test]
    fn recommended_backend_priority() {
        let probe = ProtocolProbe {
            has_ext_image_copy_capture: true,
            has_wlr_screencopy: true,
            has_virtual_pointer: false,
            has_layer_shell: true,
            has_portal: true,
        };
        assert_eq!(probe.recommended_backend(), "ext-image-copy-capture-v1");

        let probe = ProtocolProbe {
            has_ext_image_copy_capture: false,
            has_wlr_screencopy: true,
            has_virtual_pointer: false,
            has_layer_shell: true,
            has_portal: true,
        };
        assert_eq!(probe.recommended_backend(), "xdg-desktop-portal");

        let probe = ProtocolProbe {
            has_ext_image_copy_capture: false,
            has_wlr_screencopy: true,
            has_virtual_pointer: false,
            has_layer_shell: false,
            has_portal: false,
        };
        assert_eq!(probe.recommended_backend(), "wlr-screencopy-unstable-v1");

        let probe = ProtocolProbe {
            has_ext_image_copy_capture: false,
            has_wlr_screencopy: false,
            has_virtual_pointer: false,
            has_layer_shell: false,
            has_portal: false,
        };
        assert_eq!(probe.recommended_backend(), "none");
    }
}
