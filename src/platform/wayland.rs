use smithay_client_toolkit::{
    delegate_output, delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
};
use tracing::{debug, warn};
use wayland_client::{
    Connection, QueueHandle, globals::registry_queue_init, protocol::wl_output::WlOutput,
};

use super::output_info::{LogicalPoint, LogicalRect, OutputInfo, OutputTransform};
use crate::error::{Result, WlsnapError};

/// Minimal application state required by sctk 0.20 to enumerate outputs.
struct AppState {
    registry_state: RegistryState,
    output_state: OutputState,
}

impl OutputHandler for AppState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}

    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
    }
}

delegate_registry!(AppState);
delegate_output!(AppState);

impl ProvidesRegistryState for AppState {
    registry_handlers!(OutputState);

    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
}

// ---------------------------------------------------------------------------
// Focused output detection (compositor-specific IPC)
// ---------------------------------------------------------------------------

/// Try to get the focused output name using compositor-specific IPC.
///
/// Priority:
/// 1. Niri: `niri msg -j focused-output`
/// 2. Hyprland: `hyprctl monitors -j` (find focused monitor)
/// 3. Sway: `swaymsg -t get_outputs` (find focused output)
/// 4. None: could not detect
pub fn get_focused_output_name() -> Option<String> {
    // Try Niri first
    if let Some(name) = niri_focused_output() {
        return Some(name);
    }

    // Try Hyprland
    if let Some(name) = hyprland_focused_output() {
        return Some(name);
    }

    // Try Sway
    if let Some(name) = sway_focused_output() {
        return Some(name);
    }

    None
}

/// Query Niri's focused output via IPC.
fn niri_focused_output() -> Option<String> {
    let output = std::process::Command::new("niri")
        .args(["msg", "-j", "focused-output"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    json.get("name")?.as_str().map(String::from)
}

/// Query Hyprland's focused monitor via IPC.
fn hyprland_focused_output() -> Option<String> {
    let output = std::process::Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let monitors = json.as_array()?;

    for monitor in monitors {
        if monitor.get("focused")?.as_bool()? {
            return monitor.get("name")?.as_str().map(String::from);
        }
    }

    None
}

/// Query Sway's focused output via IPC.
fn sway_focused_output() -> Option<String> {
    let output = std::process::Command::new("swaymsg")
        .args(["-t", "get_outputs"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let outputs = json.as_array()?;

    for out in outputs {
        if out.get("focused")?.as_bool()? {
            return out.get("name")?.as_str().map(String::from);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Output enumeration
// ---------------------------------------------------------------------------

/// Enumerate all connected Wayland outputs and return their metadata.
///
/// If `WAYLAND_DISPLAY` is not set (i.e. not running under Wayland), returns an empty
/// vector after logging a warning rather than panicking.
pub fn enumerate_outputs() -> Result<Vec<OutputInfo>> {
    if std::env::var("WAYLAND_DISPLAY").is_err() {
        warn!("WAYLAND_DISPLAY not set; returning empty output list");
        return Ok(Vec::new());
    }

    let conn =
        Connection::connect_to_env().map_err(|e| WlsnapError::WaylandConnect(e.to_string()))?;

    let (globals, mut event_queue) = registry_queue_init::<AppState>(&conn)
        .map_err(|e| WlsnapError::WaylandConnect(e.to_string()))?;

    let qh = event_queue.handle();
    let registry_state = RegistryState::new(&globals);
    let output_state = OutputState::new(&globals, &qh);

    let mut state = AppState {
        registry_state,
        output_state,
    };

    // Dispatch events so the output state can receive wl_output events.
    event_queue
        .roundtrip(&mut state)
        .map_err(|e| WlsnapError::WaylandConnect(e.to_string()))?;

    let mut outputs = Vec::new();
    for output in state.output_state.outputs() {
        let Some(info) = state.output_state.info(&output) else {
            continue;
        };

        let name = info.name.unwrap_or_default();
        let description = info.description.unwrap_or_default();

        let loc_x = info.location.0 as f64;
        let loc_y = info.location.1 as f64;

        let (logical_w, logical_h) = match info.logical_size {
            Some((w, h)) => (w as f64, h as f64),
            None => {
                // Fall back to current mode dimensions divided by scale.
                let current_mode = info.modes.iter().find(|m| m.current);
                let (px_w, px_h) = match current_mode {
                    Some(mode) => (mode.dimensions.0 as f64, mode.dimensions.1 as f64),
                    None => (0.0, 0.0),
                };
                (
                    px_w / info.scale_factor as f64,
                    px_h / info.scale_factor as f64,
                )
            }
        };

        let logical_geometry = LogicalRect {
            min: LogicalPoint { x: loc_x, y: loc_y },
            max: LogicalPoint {
                x: loc_x + logical_w,
                y: loc_y + logical_h,
            },
        };

        let physical_size = {
            let current_mode = info.modes.iter().find(|m| m.current);
            match current_mode {
                Some(mode) => (mode.dimensions.0 as u32, mode.dimensions.1 as u32),
                None => (0, 0),
            }
        };

        let transform = OutputTransform::from(info.transform);

        debug!(
            "Detected output '{}' ({}) at {:?}x{:?}, scale={}",
            name, description, logical_geometry.min.x, logical_geometry.min.y, info.scale_factor
        );

        outputs.push(OutputInfo {
            name,
            description,
            logical_geometry,
            physical_size,
            scale_factor: info.scale_factor as f64,
            transform,
        });
    }

    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensure that `enumerate_outputs` does not panic when WAYLAND_DISPLAY is absent.
    #[test]
    fn enumerate_outputs_without_wayland_display() {
        let old = std::env::var_os("WAYLAND_DISPLAY");
        unsafe {
            std::env::remove_var("WAYLAND_DISPLAY");
        }
        let result = enumerate_outputs();
        if let Some(v) = old {
            unsafe {
                std::env::set_var("WAYLAND_DISPLAY", v);
            }
        }
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
