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
    shm::{slot::{Slot, SlotPool}, Shm, ShmHandler},
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

    /// Throttle redraws: only redraw when the mouse has moved at least this many pixels.
    last_drawn_pos: Option<(f64, f64)>,

    // Double-buffered slots to avoid page-faults on every draw.
    slot_a: Option<Slot>,
    slot_b: Option<Slot>,
    use_slot_b: bool,
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
            last_drawn_pos: None,
            slot_a: None,
            slot_b: None,
            use_slot_b: false,
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

        // Pick the next slot, falling back to the other if it is still active.
        let (slot_ref, next_use_b) = if self.use_slot_b {
            if let Some(ref slot) = self.slot_b {
                if !slot.has_active_buffers() {
                    (slot, false)
                } else if let Some(ref slot_a) = self.slot_a {
                    (slot_a, false)
                } else {
                    return;
                }
            } else if let Some(ref slot_a) = self.slot_a {
                (slot_a, false)
            } else {
                return;
            }
        } else {
            if let Some(ref slot) = self.slot_a {
                if !slot.has_active_buffers() {
                    (slot, true)
                } else if let Some(ref slot_b) = self.slot_b {
                    (slot_b, true)
                } else {
                    return;
                }
            } else if let Some(ref slot_b) = self.slot_b {
                (slot_b, true)
            } else {
                return;
            }
        };
        self.use_slot_b = next_use_b;

        let buffer = self
            .pool
            .create_buffer_in(slot_ref, width as i32, height as i32, stride, wl_shm::Format::Argb8888)
            .expect("create buffer in slot");
        let canvas = self.pool.raw_data_mut(slot_ref);

        // Fill entire screen with mask.
        let mask_bytes = MASK_COLOR.to_le_bytes();
        for chunk in canvas[..buf_size].chunks_exact_mut(4) {
            chunk.copy_from_slice(&mask_bytes);
        }

        // Draw selection highlight if dragging.
        if let Some(start) = self.drag_start {
            let x1 = start.0.min(self.drag_current.0);
            let y1 = start.1.min(self.drag_current.1);
            let x2 = start.0.max(self.drag_current.0);
            let y2 = start.1.max(self.drag_current.1);

            let x1_i = x1 as i32;
            let y1_i = y1 as i32;
            let x2_i = x2 as i32;
            let y2_i = y2 as i32;

            // Fill highlight inside selection.
            let highlight_bytes = HIGHLIGHT_COLOR.to_le_bytes();
            for y in y1_i..y2_i {
                if y < 0 || y >= height as i32 {
                    continue;
                }
                let row_offset = (y * stride) as usize;
                for x in x1_i..x2_i {
                    if x < 0 || x >= width as i32 {
                        continue;
                    }
                    let offset = row_offset + (x as usize) * 4;
                    canvas[offset..offset + 4].copy_from_slice(&highlight_bytes);
                }
            }

            // Draw border.
            draw_rect_border(
                canvas,
                width,
                height,
                Rect {
                    x1: x1_i,
                    y1: y1_i,
                    x2: x2_i,
                    y2: y2_i,
                },
                BORDER_COLOR,
            );

            // Draw size label near bottom-right of selection.
            let w = (x2 - x1) as i32;
            let h = (y2 - y1) as i32;
            let label = format!("{}x{}", w, h);
            let label_x = (x2_i - label.len() as i32 * 8).max(4);
            let label_y = (y2_i - 16).max(4);
            draw_text(canvas, width, height, label_x, label_y, &label, TEXT_COLOR);
        }

        // Draw hint text at bottom center.
        let hint = "Esc cancel | Drag to select";
        let hint_x = (width as i32 - hint.len() as i32 * 8) / 2;
        let hint_y = height as i32 - 30;
        draw_text(canvas, width, height, hint_x, hint_y, &hint, TEXT_COLOR);

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
// Simple software rendering helpers
// ---------------------------------------------------------------------------

/// Rectangle for border drawing.
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

/// Draw a 1-pixel border around a rectangle.
fn draw_rect_border(canvas: &mut [u8], width: u32, height: u32, rect: Rect, color: u32) {
    let stride = width as i32 * 4;
    let color_bytes = color.to_le_bytes();

    for x in rect.x1..rect.x2 {
        set_pixel(canvas, stride, height, x, rect.y1, color_bytes);
        set_pixel(canvas, stride, height, x, rect.y2 - 1, color_bytes);
    }
    for y in rect.y1..rect.y2 {
        set_pixel(canvas, stride, height, rect.x1, y, color_bytes);
        set_pixel(canvas, stride, height, rect.x2 - 1, y, color_bytes);
    }
}

/// Set a single pixel (clamped to bounds).
fn set_pixel(canvas: &mut [u8], stride: i32, height: u32, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= stride / 4 || y >= height as i32 {
        return;
    }
    let offset = (y * stride + x * 4) as usize;
    canvas[offset..offset + 4].copy_from_slice(&color);
}

/// Draw simple 8x8 pixel text (ASCII only).
fn draw_text(canvas: &mut [u8], width: u32, height: u32, x: i32, y: i32, text: &str, color: u32) {
    let stride = width as i32 * 4;
    let color_bytes = color.to_le_bytes();
    let mut cx = x;
    for ch in text.chars() {
        if cx + 8 >= width as i32 {
            break;
        }
        draw_char(canvas, stride, height, cx, y, ch, color_bytes);
        cx += 8;
    }
}

/// Draw a single character using a simple bitmap font.
fn draw_char(canvas: &mut [u8], stride: i32, height: u32, x: i32, y: i32, ch: char, color: [u8; 4]) {
    // Simple 8x8 bitmap patterns for digits and common chars.
    let pattern = char_pattern(ch);
    for (row, &pattern_byte) in pattern.iter().enumerate() {
        for col in 0..8 {
            if (pattern_byte >> (7 - col)) & 1 != 0 {
                set_pixel(canvas, stride, height, x + col, y + row as i32, color);
            }
        }
    }
}

/// Return an 8x8 bitmap pattern for a character.
fn char_pattern(ch: char) -> [u8; 8] {
    match ch {
        '0' => [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00],
        '1' => [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        '2' => [0x3C, 0x66, 0x06, 0x0C, 0x30, 0x60, 0x7E, 0x00],
        '3' => [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
        '4' => [0x06, 0x0E, 0x1E, 0x66, 0x7F, 0x06, 0x06, 0x00],
        '5' => [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
        '6' => [0x3C, 0x66, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00],
        '7' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00],
        '8' => [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
        '9' => [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x66, 0x3C, 0x00],
        'x' => [0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00, 0x00],
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '|' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        'c' => [0x00, 0x3C, 0x66, 0x60, 0x60, 0x66, 0x3C, 0x00],
        'a' => [0x00, 0x3C, 0x06, 0x3E, 0x66, 0x66, 0x3E, 0x00],
        'n' => [0x00, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x00],
        'e' => [0x00, 0x3C, 0x66, 0x7E, 0x60, 0x66, 0x3C, 0x00],
        'l' => [0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x7E, 0x00],
        'D' => [0x7C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x7C, 0x00],
        'g' => [0x00, 0x3C, 0x66, 0x66, 0x3E, 0x06, 0x3C, 0x00],
        't' => [0x18, 0x18, 0x7E, 0x18, 0x18, 0x18, 0x0E, 0x00],
        'o' => [0x00, 0x3C, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        's' => [0x00, 0x3E, 0x60, 0x3C, 0x06, 0x06, 0x7C, 0x00],
        'r' => [0x00, 0x7C, 0x66, 0x60, 0x60, 0x60, 0x60, 0x00],
        'd' => [0x00, 0x3E, 0x66, 0x66, 0x66, 0x66, 0x3E, 0x00],
        'S' => [0x3E, 0x60, 0x60, 0x3C, 0x06, 0x06, 0x7C, 0x00],
        'C' => [0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
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

        // Pre-allocate and warm-up double-buffered slots to avoid page-faults on every draw.
        let stride = self.width as i32 * 4;
        let buf_size = (stride * self.height as i32) as usize;
        if self.pool.len() < buf_size * 2 {
            let _ = self.pool.resize(buf_size * 2);
        }
        if self.slot_a.is_none() {
            if let Ok(slot) = self.pool.new_slot(buf_size) {
                if let Some(data) = self.pool.raw_data_mut(&slot).get_mut(..buf_size) {
                    data.fill(0x00);
                }
                self.slot_a = Some(slot);
            }
        }
        if self.slot_b.is_none() {
            if let Ok(slot) = self.pool.new_slot(buf_size) {
                if let Some(data) = self.pool.raw_data_mut(&slot).get_mut(..buf_size) {
                    data.fill(0x00);
                }
                self.slot_b = Some(slot);
            }
        }

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
