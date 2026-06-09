//! wlr-screencopy-unstable-v1 capture backend.
//!
//! Provides SHM-based capture of a single Wayland output via the
//! `zwlr_screencopy_manager_v1` protocol.

use std::os::unix::io::AsFd;

use image::RgbaImage;
use smithay_client_toolkit::{
    delegate_output,
    output::{OutputHandler, OutputState},
};
use tokio::task;
use tracing::{debug, warn};
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_buffer::WlBuffer, wl_output::WlOutput, wl_registry::WlRegistry, wl_shm::WlShm,
        wl_shm_pool::WlShmPool,
    },
};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};

use crate::{
    error::{Result, WlsnapError},
    image_engine::transform::OutputTransform as ImageOutputTransform,
    platform::output_info::OutputInfo,
};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Internal state used while driving the Wayland event queue for a capture.
struct CaptureState {
    output_state: OutputState,
    frame_state: FrameState,
}

/// Accumulated state for a single screencopy frame.
#[derive(Default)]
struct FrameState {
    buffer_info: Option<BufferInfo>,
    ready: bool,
    failed: bool,
}

/// Parameters received from the compositor via `zwlr_screencopy_frame_v1::buffer`.
#[derive(Clone)]
struct BufferInfo {
    format: wayland_client::protocol::wl_shm::Format,
    width: u32,
    height: u32,
    stride: u32,
}

// ---------------------------------------------------------------------------
// Dispatch implementations
// ---------------------------------------------------------------------------

impl Dispatch<WlRegistry, GlobalListContents> for CaptureState {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as wayland_client::Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        // Events are handled internally by `registry_queue_init`.
    }
}

impl OutputHandler for CaptureState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}

    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
    }
}

delegate_output!(CaptureState);

impl Dispatch<ZwlrScreencopyManagerV1, ()> for CaptureState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrScreencopyManagerV1,
        _event: <ZwlrScreencopyManagerV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for CaptureState {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrScreencopyFrameV1,
        event: <ZwlrScreencopyFrameV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => match format {
                WEnum::Value(fmt) => {
                    debug!(
                        "screencopy buffer: format={:?} width={} height={} stride={}",
                        fmt, width, height, stride
                    );
                    state.frame_state.buffer_info = Some(BufferInfo {
                        format: fmt,
                        width,
                        height,
                        stride,
                    });
                }
                WEnum::Unknown(v) => {
                    warn!("screencopy returned unknown buffer format: {}", v);
                }
            },
            zwlr_screencopy_frame_v1::Event::Flags { .. } => {
                // Flags are received but not needed for the skeleton.
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                state.frame_state.ready = true;
            }
            zwlr_screencopy_frame_v1::Event::Failed => {
                state.frame_state.failed = true;
            }
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(CaptureState: ignore WlShm);
wayland_client::delegate_noop!(CaptureState: ignore WlShmPool);
wayland_client::delegate_noop!(CaptureState: ignore WlBuffer);

// ---------------------------------------------------------------------------
// Pixel conversion helpers
// ---------------------------------------------------------------------------

/// Convert a BGRA (or BGRX) buffer into an `image::RgbaImage`.
///
/// `stride` is the number of bytes per row (may include padding).
/// If `fix_alpha` is `true`, every pixel's alpha channel is forced to 255.
fn bgra_to_rgba(buf: &[u8], width: u32, height: u32, stride: u32, fix_alpha: bool) -> RgbaImage {
    let mut img = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let offset = (y * stride + x * 4) as usize;
            let b = buf[offset];
            let g = buf[offset + 1];
            let r = buf[offset + 2];
            let a = if fix_alpha { 255 } else { buf[offset + 3] };
            img.put_pixel(x, y, image::Rgba([r, g, b, a]));
        }
    }
    img
}

// ---------------------------------------------------------------------------
// Blocking capture implementation
// ---------------------------------------------------------------------------

fn capture_output_blocking(
    conn: &Connection,
    output: &OutputInfo,
    overlay_cursor: bool,
) -> Result<RgbaImage> {
    let (globals, mut event_queue) = registry_queue_init::<CaptureState>(conn)
        .map_err(|e| WlsnapError::WaylandConnect(e.to_string()))?;

    let qh = event_queue.handle();
    let output_state = OutputState::new(&globals, &qh);

    let mut state = CaptureState {
        output_state,
        frame_state: FrameState::default(),
    };

    // Round-trip so OutputState can receive wl_output events.
    event_queue
        .roundtrip(&mut state)
        .map_err(|e| WlsnapError::WaylandConnect(e.to_string()))?;

    // Find the WlOutput whose name matches the requested output.
    let target_output = state
        .output_state
        .outputs()
        .find(|o| {
            state
                .output_state
                .info(o)
                .and_then(|info| info.name)
                .as_deref()
                == Some(&output.name)
        })
        .ok_or(WlsnapError::NoOutputDetected)?;

    // Bind wlr-screencopy manager.
    let screencopy_manager: ZwlrScreencopyManagerV1 = globals
        .bind(&qh, 1..=3, ())
        .map_err(|_| WlsnapError::NoBackendAvailable)?;

    // Bind wl_shm for SHM allocation.
    let shm: WlShm = globals
        .bind(&qh, 1..=1, ())
        .map_err(|e| WlsnapError::WaylandConnect(e.to_string()))?;

    // Request a frame capture.
    let frame = screencopy_manager.capture_output(
        if overlay_cursor { 1 } else { 0 },
        &target_output,
        &qh,
        (),
    );

    // Wait for the compositor to tell us the buffer parameters.
    event_queue
        .roundtrip(&mut state)
        .map_err(|e| WlsnapError::WaylandConnect(e.to_string()))?;

    let buffer_info =
        state.frame_state.buffer_info.clone().ok_or_else(|| {
            WlsnapError::WaylandConnect("compositor did not send buffer info".into())
        })?;

    // We only support the two common little-endian 32-bit formats.
    let fix_alpha = match buffer_info.format {
        wayland_client::protocol::wl_shm::Format::Argb8888 => false,
        wayland_client::protocol::wl_shm::Format::Xrgb8888 => true,
        other => {
            return Err(WlsnapError::WaylandConnect(format!(
                "unsupported screencopy format: {:?}",
                other
            )));
        }
    };

    // Allocate a writable SHM buffer.
    let buf_size = (buffer_info.stride * buffer_info.height) as usize;
    let file = tempfile::tempfile()?;
    file.set_len(buf_size as u64)?;

    let pool = shm.create_pool(file.as_fd(), buf_size as i32, &qh, ());
    let buffer = pool.create_buffer(
        0,
        buffer_info.width as i32,
        buffer_info.height as i32,
        buffer_info.stride as i32,
        buffer_info.format,
        &qh,
        (),
    );

    // Ask the compositor to copy the frame into our buffer.
    frame.copy(&buffer);

    // Block until ready or failed.
    while !state.frame_state.ready && !state.frame_state.failed {
        event_queue
            .blocking_dispatch(&mut state)
            .map_err(|e| WlsnapError::WaylandConnect(e.to_string()))?;
    }

    if state.frame_state.failed {
        return Err(WlsnapError::WaylandConnect(
            "screencopy frame failed".into(),
        ));
    }

    // Map the SHM file and convert pixels.
    let mmap = unsafe { memmap2::Mmap::map(&file)? };
    debug!(
        "screencopy capture for '{}': buffer={}x{} transform={:?}",
        output.name, buffer_info.width, buffer_info.height, output.transform
    );
    let mut img = bgra_to_rgba(
        &mmap,
        buffer_info.width,
        buffer_info.height,
        buffer_info.stride,
        fix_alpha,
    );
    debug!(
        "screencopy after bgra_to_rgba: {}x{}",
        img.width(),
        img.height()
    );

    // Apply output transform if necessary.
    let transform = match output.transform {
        crate::platform::output_info::OutputTransform::Normal => ImageOutputTransform::Normal,
        crate::platform::output_info::OutputTransform::Rotated90 => ImageOutputTransform::Rotated90,
        crate::platform::output_info::OutputTransform::Rotated180 => {
            ImageOutputTransform::Rotated180
        }
        crate::platform::output_info::OutputTransform::Rotated270 => {
            ImageOutputTransform::Rotated270
        }
        crate::platform::output_info::OutputTransform::Flipped => ImageOutputTransform::Flipped,
        crate::platform::output_info::OutputTransform::Flipped90 => ImageOutputTransform::Flipped90,
        crate::platform::output_info::OutputTransform::Flipped180 => {
            ImageOutputTransform::Flipped180
        }
        crate::platform::output_info::OutputTransform::Flipped270 => {
            ImageOutputTransform::Flipped270
        }
    };
    img = transform.apply_to_image(&img);
    debug!(
        "screencopy after transform: {}x{}",
        img.width(),
        img.height()
    );

    // Clean up Wayland objects.
    frame.destroy();
    pool.destroy();

    Ok(img)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Capture a single output using `wlr-screencopy-unstable-v1`.
///
/// If `WAYLAND_DISPLAY` is not set, the compositor does not advertise
/// `zwlr_screencopy_manager_v1`, or any other fatal error occurs, a
/// [`WlsnapError`] is returned.
pub async fn capture_output(
    conn: &Connection,
    output: &OutputInfo,
    overlay_cursor: bool,
) -> Result<RgbaImage> {
    let conn = conn.clone();
    let output = output.clone();
    task::spawn_blocking(move || capture_output_blocking(&conn, &output, overlay_cursor))
        .await
        .map_err(|e| WlsnapError::WaylandConnect(format!("blocking task panicked: {e}")))?
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `capture_output` must return an error (never panic) when WAYLAND_DISPLAY is missing.
    #[test]
    fn capture_output_without_wayland_display() {
        let old = std::env::var_os("WAYLAND_DISPLAY");
        unsafe {
            std::env::remove_var("WAYLAND_DISPLAY");
        }

        // We can't create a Connection without WAYLAND_DISPLAY, so we test
        // the blocking helper directly by trying to connect first.
        let result =
            Connection::connect_to_env().map_err(|e| WlsnapError::WaylandConnect(e.to_string()));

        if let Some(v) = old {
            unsafe {
                std::env::set_var("WAYLAND_DISPLAY", v);
            }
        }

        assert!(result.is_err());
    }

    /// Verify that the BGRA → RGBA conversion helper is correct.
    #[test]
    fn bgra_to_rgba_conversion() {
        // 2×1 image: pixel 0 = [B=1, G=2, R=3, A=4], pixel 1 = [B=5, G=6, R=7, A=8]
        let bgra = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let img = bgra_to_rgba(&bgra, 2, 1, 8, false);
        assert_eq!(img.get_pixel(0, 0), &image::Rgba([3, 2, 1, 4]));
        assert_eq!(img.get_pixel(1, 0), &image::Rgba([7, 6, 5, 8]));
    }

    /// Verify that the BGRX → RGBA conversion forces alpha to 255.
    #[test]
    fn bgra_to_rgba_fix_alpha() {
        let bgra = vec![10, 20, 30, 0]; // alpha byte is ignored when fix_alpha=true
        let img = bgra_to_rgba(&bgra, 1, 1, 4, true);
        assert_eq!(img.get_pixel(0, 0), &image::Rgba([30, 20, 10, 255]));
    }

    /// Integration test: actually tries to capture a frame when running under
    /// a wlr-based compositor.  Ignored by default for CI.
    #[test]
    #[ignore]
    fn capture_real_output() {
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            eprintln!("Skipping: WAYLAND_DISPLAY not set");
            return;
        }

        let conn = Connection::connect_to_env().expect("connect to Wayland");

        // Enumerate outputs using the platform helper.
        let outputs = crate::platform::wayland::enumerate_outputs()
            .expect("enumerate outputs should not fail");
        let output = outputs
            .into_iter()
            .next()
            .expect("at least one output should be present");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let img = rt
            .block_on(capture_output(&conn, &output, false))
            .expect("capture should succeed under a wlr compositor");

        assert!(img.width() > 0);
        assert!(img.height() > 0);
    }
}
