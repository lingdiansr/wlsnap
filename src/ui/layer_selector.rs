//! Layer-shell based interactive region selector.
//!
//! Creates a fullscreen overlay surface using `zwlr_layer_shell_v1`.
//! The user drags to select a region; the selected rectangle is returned
//! in logical coordinates.

use std::num::NonZeroU32;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, QueueHandle,
};

use crate::platform::output_info::{LogicalPoint, LogicalRect};

/// Minimum selection size in logical pixels.
const MIN_SELECTION_SIZE: f64 = 10.0;

/// ARGB color constants.
const MASK_COLOR: u32 = 0x8000_0000; // semi-transparent black
const HIGHLIGHT_COLOR: u32 = 0x4000_0000; // lighter inside selection
const BORDER_COLOR: u32 = 0xFFFF_FFFF; // white border
const TEXT_COLOR: u32 = 0xFFFF_FFFF; // white text

/// Interactive region selector using layer-shell overlay.
pub struct LayerSelector {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,

    pool: SlotPool,
    width: u32,
    height: u32,
    layer: LayerSurface,

    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,

    /// Selection state.
    drag_start: Option<(f64, f64)>,
    drag_current: (f64, f64),
    selected_region: Option<LogicalRect>,

    /// Exit flags.
    exit: bool,
    cancelled: bool,
}

impl LayerSelector {
    /// Create and run the selector, returning the selected region or None if cancelled.
    pub fn run() -> Option<LogicalRect> {
        let conn = Connection::connect_to_env().ok()?;
        let (globals, mut event_queue) = registry_queue_init(&conn).ok()?;
        let qh = event_queue.handle();

        let compositor = CompositorState::bind(&globals, &qh).ok()?;
        let layer_shell = LayerShell::bind(&globals, &qh).ok()?;
        let shm = Shm::bind(&globals, &qh).ok()?;

        let surface = compositor.create_surface(&qh);
        let layer = layer_shell.create_layer_surface(
            &qh,
            surface,
            Layer::Overlay,
            Some("wlsnap-area-selection"),
            None,
        );

        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.set_size(0, 0);
        layer.commit();

        let pool = SlotPool::new(256 * 256 * 4, &shm).ok()?;

        let mut selector = LayerSelector {
            registry_state: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),
            shm,
            pool,
            width: 256,
            height: 256,
            layer,
            keyboard: None,
            pointer: None,
            drag_start: None,
            drag_current: (0.0, 0.0),
            selected_region: None,
            exit: false,
            cancelled: false,
        };

        while !selector.exit {
            if event_queue.blocking_dispatch(&mut selector).is_err() {
                break;
            }
        }

        selector.selected_region
    }

    /// Draw the overlay to the layer surface.
    fn draw(&mut self, _qh: &QueueHandle<Self>) {
        let width = self.width;
        let height = self.height;
        let stride = width as i32 * 4;
        let buf_size = (stride * height as i32) as usize;

        // Resize pool if needed.
        if self.pool.len() < buf_size {
            let _ = self.pool.resize(buf_size);
        }

        let (buffer, canvas) = self
            .pool
            .create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888)
            .expect("create buffer");

        // Use tiny-skia to render the overlay.
        render_selector(canvas, width, height, self.drag_start, self.drag_current);

        // Damage entire surface and present.
        self.layer
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);
        // No frame callback needed — we only redraw on user interaction.
        buffer.attach_to(self.layer.wl_surface()).expect("buffer attach");
        self.layer.commit();
    }
}

// ---------------------------------------------------------------------------
// tiny-skia rendering
// ---------------------------------------------------------------------------

/// Render the selector UI using tiny-skia.
fn render_selector(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    drag_start: Option<(f64, f64)>,
    drag_current: (f64, f64),
) {
    use tiny_skia::{Color, Paint, Pixmap, Transform};

    let mut pixmap = Pixmap::new(width, height).expect("pixmap");
    pixmap.fill(Color::from_rgba8(0, 0, 0, 128));

    if let Some(start) = drag_start {
        let x1 = start.0.min(drag_current.0) as f32;
        let y1 = start.1.min(drag_current.1) as f32;
        let x2 = start.0.max(drag_current.0) as f32;
        let y2 = start.1.max(drag_current.1) as f32;

        let sel_rect = tiny_skia::Rect::from_ltrb(x1, y1, x2, y2);
        if let Some(rect) = sel_rect {
            // Highlight fill
            let mut paint = Paint::default();
            paint.set_color(Color::from_rgba8(255, 255, 255, 64));
            pixmap.fill_rect(rect, &paint, Transform::identity(), None);

            // White border (1px)
            let mut border_paint = Paint::default();
            border_paint.set_color(Color::from_rgba8(255, 255, 255, 255));
            border_paint.anti_alias = false;

            // Top
            if let Some(top) = tiny_skia::Rect::from_ltrb(x1, y1, x2, y1 + 1.0) {
                pixmap.fill_rect(top, &border_paint, Transform::identity(), None);
            }
            // Bottom
            if let Some(bottom) = tiny_skia::Rect::from_ltrb(x1, y2 - 1.0, x2, y2) {
                pixmap.fill_rect(bottom, &border_paint, Transform::identity(), None);
            }
            // Left
            if let Some(left) = tiny_skia::Rect::from_ltrb(x1, y1, x1 + 1.0, y2) {
                pixmap.fill_rect(left, &border_paint, Transform::identity(), None);
            }
            // Right
            if let Some(right) = tiny_skia::Rect::from_ltrb(x2 - 1.0, y1, x2, y2) {
                pixmap.fill_rect(right, &border_paint, Transform::identity(), None);
            }

            // Size label
            let w = (x2 - x1) as i32;
            let h = (y2 - y1) as i32;
            let label = format!("{}x{}", w, h);
            draw_label(&mut pixmap, x2 - 4.0, y2 - 4.0, &label);
        }
    }

    // Hint text
    draw_label(&mut pixmap, width as f32 / 2.0, height as f32 - 30.0, "Esc cancel | Drag to select");

    // Copy from pixmap (RGBA) to canvas (BGRA for Wayland ARGB8888)
    // tiny-skia stores as RGBA; Wayland ARGB8888 expects BGRA in memory.
    let data = pixmap.data();
    for (dst_chunk, src_chunk) in canvas.chunks_exact_mut(4).zip(data.chunks_exact(4)) {
        dst_chunk[0] = src_chunk[2]; // B
        dst_chunk[1] = src_chunk[1]; // G
        dst_chunk[2] = src_chunk[0]; // R
        dst_chunk[3] = src_chunk[3]; // A
    }
}

/// Draw a simple text label using tiny-skia (basic pixel font).
fn draw_label(pixmap: &mut tiny_skia::Pixmap, x: f32, y: f32, text: &str) {
    use tiny_skia::{Color, Paint, Transform};

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(255, 255, 255, 255));
    paint.anti_alias = false;

    let scale = 2.0;
    let mut cx = x - (text.len() as f32 * 5.0 * scale) / 2.0;
    let cy = y;

    for ch in text.chars() {
        let pattern = char_pattern(ch);
        for (row, &bits) in pattern.iter().enumerate() {
            for col in 0..5u32 {
                if (bits >> (4 - col)) & 1 != 0 {
                    let px = cx + col as f32 * scale;
                    let py = cy + row as f32 * scale;
                    if let Some(rect) = tiny_skia::Rect::from_xywh(px, py, scale, scale) {
                        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                    }
                }
            }
        }
        cx += 6.0 * scale;
    }
}

/// Simple 5x7 bitmap patterns for ASCII characters.
fn char_pattern(ch: char) -> [u8; 7] {
    match ch {
        '0' => [0x3E, 0x51, 0x49, 0x45, 0x3E, 0x00, 0x00],
        '1' => [0x00, 0x42, 0x7F, 0x40, 0x00, 0x00, 0x00],
        '2' => [0x42, 0x61, 0x51, 0x49, 0x46, 0x00, 0x00],
        '3' => [0x21, 0x41, 0x45, 0x4B, 0x31, 0x00, 0x00],
        '4' => [0x18, 0x14, 0x12, 0x7F, 0x10, 0x00, 0x00],
        '5' => [0x27, 0x45, 0x45, 0x45, 0x39, 0x00, 0x00],
        '6' => [0x3C, 0x4A, 0x49, 0x49, 0x30, 0x00, 0x00],
        '7' => [0x01, 0x71, 0x09, 0x05, 0x03, 0x00, 0x00],
        '8' => [0x36, 0x49, 0x49, 0x49, 0x36, 0x00, 0x00],
        '9' => [0x06, 0x49, 0x49, 0x29, 0x1E, 0x00, 0x00],
        'E' => [0x7F, 0x49, 0x49, 0x49, 0x41, 0x00, 0x00],
        's' => [0x32, 0x49, 0x49, 0x49, 0x26, 0x00, 0x00],
        'c' => [0x1E, 0x21, 0x21, 0x21, 0x12, 0x00, 0x00],
        'a' => [0x20, 0x54, 0x54, 0x54, 0x78, 0x00, 0x00],
        'n' => [0x7C, 0x08, 0x04, 0x04, 0x78, 0x00, 0x00],
        'l' => [0x00, 0x41, 0x7F, 0x40, 0x00, 0x00, 0x00],
        'D' => [0x7F, 0x41, 0x41, 0x22, 0x1C, 0x00, 0x00],
        'r' => [0x7C, 0x08, 0x04, 0x04, 0x08, 0x00, 0x00],
        'g' => [0x3E, 0x41, 0x49, 0x49, 0x7A, 0x00, 0x00],
        't' => [0x04, 0x3F, 0x44, 0x40, 0x20, 0x00, 0x00],
        'o' => [0x1C, 0x22, 0x41, 0x41, 0x22, 0x00, 0x00],
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '|' => [0x00, 0x00, 0x7F, 0x00, 0x00, 0x00, 0x00],
        '-' => [0x08, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00],
        'x' => [0x44, 0x2A, 0x11, 0x2A, 0x44, 0x00, 0x00],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    }
}

// ---------------------------------------------------------------------------
// Wayland trait implementations
// ---------------------------------------------------------------------------

impl CompositorHandler for LayerSelector {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        // Intentionally empty: we do not need continuous animation.
        // The selector only redraws on pointer interaction.
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for LayerSelector {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for LayerSelector {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
        self.cancelled = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.width = NonZeroU32::new(configure.new_size.0).map_or(256, NonZeroU32::get);
        self.height = NonZeroU32::new(configure.new_size.1).map_or(256, NonZeroU32::get);
        self.draw(qh);
    }
}

impl SeatHandler for LayerSelector {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let keyboard = self
                .seat_state
                .get_keyboard(qh, &seat, None)
                .expect("Failed to create keyboard");
            self.keyboard = Some(keyboard);
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            let pointer = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("Failed to create pointer");
            self.pointer = Some(pointer);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_some() {
            self.keyboard.take().unwrap().release();
        }
        if capability == Capability::Pointer && self.pointer.is_some() {
            self.pointer.take().unwrap().release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for LayerSelector {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if event.keysym == Keysym::Escape {
            self.cancelled = true;
            self.exit = true;
        }
    }

    fn repeat_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _event: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _modifiers: smithay_client_toolkit::seat::keyboard::Modifiers,
        _raw_modifiers: smithay_client_toolkit::seat::keyboard::RawModifiers,
        _layout: u32,
    ) {
    }
}

impl PointerHandler for LayerSelector {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        use PointerEventKind::*;
        for event in events {
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            match event.kind {
                Enter { .. } | Motion { .. } => {
                    self.drag_current = event.position;
                    if self.drag_start.is_some() {
                        self.draw(qh);
                    }
                }
                Press { button, .. } => {
                    // Left button (272)
                    if button == 272 {
                        self.drag_start = Some(event.position);
                        self.drag_current = event.position;
                        self.draw(qh);
                    }
                }
                Release { button, .. } => {
                    if button == 272 && let Some(start) = self.drag_start {
                        let x1 = start.0.min(self.drag_current.0);
                        let y1 = start.1.min(self.drag_current.1);
                        let x2 = start.0.max(self.drag_current.0);
                        let y2 = start.1.max(self.drag_current.1);

                        let w = x2 - x1;
                        let h = y2 - y1;

                        if w >= MIN_SELECTION_SIZE && h >= MIN_SELECTION_SIZE {
                            self.selected_region = Some(LogicalRect {
                                min: LogicalPoint { x: x1, y: y1 },
                                max: LogicalPoint { x: x2, y: y2 },
                            });
                        }
                        self.exit = true;
                    }
                }
                Leave { .. } => {}
                Axis { .. } => {}
            }
        }
    }
}

impl ShmHandler for LayerSelector {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_compositor!(LayerSelector);
delegate_output!(LayerSelector);
delegate_shm!(LayerSelector);
delegate_seat!(LayerSelector);
delegate_keyboard!(LayerSelector);
delegate_pointer!(LayerSelector);
delegate_layer!(LayerSelector);
delegate_registry!(LayerSelector);

impl ProvidesRegistryState for LayerSelector {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}
