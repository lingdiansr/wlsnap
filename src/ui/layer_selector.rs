//! Layer-shell based interactive region selector.
//!
//! Creates a fullscreen overlay surface using `zwlr_layer_shell_v1`.
//! The user drags to select a region; the selected rectangle is returned
//! in logical coordinates.

use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{
        Shm, ShmHandler,
        slot::{Slot, SlotPool},
    },
};
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};

use crate::platform::output_info::{LogicalPoint, LogicalRect};

/// Minimum selection size in logical pixels.
const MIN_SELECTION_SIZE: f64 = 10.0;

/// ARGB color constants.
const _MASK_COLOR: u32 = 0x8000_0000; // semi-transparent black
const _HIGHLIGHT_COLOR: u32 = 0x4000_0000; // lighter inside selection
const _BORDER_COLOR: u32 = 0xFFFF_FFFF; // white border
const _TEXT_COLOR: u32 = 0xFFFF_FFFF; // white text

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

    /// Throttle redraws during motion.
    last_draw_time: Option<Instant>,
    last_drawn_pos: Option<(f64, f64)>,

    // Double-buffered slots to avoid page-faults and pool growth.
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
        layer.set_exclusive_zone(-1);
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
            last_draw_time: None,
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

        // Ensure pool is big enough for two slots.
        if self.pool.len() < buf_size * 2 {
            let _ = self.pool.resize(buf_size * 2);
        }

        // Pre-allocate slots on first configure.
        #[allow(clippy::collapsible_if)]
        if self.slot_a.is_none() {
            if let Ok(slot) = self.pool.new_slot(buf_size) {
                self.slot_a = Some(slot);
            }
        }
        #[allow(clippy::collapsible_if)]
        if self.slot_b.is_none() {
            if let Ok(slot) = self.pool.new_slot(buf_size) {
                self.slot_b = Some(slot);
            }
        }

        // Simple round-robin between the two slots.
        let slot_ref = if self.use_slot_b {
            self.slot_b.as_ref().or(self.slot_a.as_ref())
        } else {
            self.slot_a.as_ref().or(self.slot_b.as_ref())
        };
        let Some(slot_ref) = slot_ref else {
            return;
        };
        self.use_slot_b = !self.use_slot_b;

        let buffer = self
            .pool
            .create_buffer_in(
                slot_ref,
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .expect("create buffer in slot");
        let canvas = self.pool.raw_data_mut(slot_ref);

        // Use tiny-skia to render the overlay.
        render_selector(
            &mut canvas[..buf_size],
            width,
            height,
            self.drag_start,
            self.drag_current,
        );

        // Damage entire surface and present.
        self.layer
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);
        // No frame callback needed — we only redraw on user interaction.
        buffer
            .attach_to(self.layer.wl_surface())
            .expect("buffer attach");
        self.layer.commit();

        self.last_draw_time = Some(Instant::now());
        self.last_drawn_pos = Some(self.drag_current);
    }

    /// Check if we should redraw based on time and distance throttling.
    fn should_redraw(&self) -> bool {
        const MIN_TIME_BETWEEN_DRAWS: Duration = Duration::from_millis(16);
        const MIN_DISTANCE: f64 = 2.0;

        if let Some(last_time) = self.last_draw_time
            && Instant::now().duration_since(last_time) < MIN_TIME_BETWEEN_DRAWS
        {
            return false;
        }

        if let Some(last_pos) = self.last_drawn_pos {
            let dx = self.drag_current.0 - last_pos.0;
            let dy = self.drag_current.1 - last_pos.1;
            if dx.abs() < MIN_DISTANCE && dy.abs() < MIN_DISTANCE {
                return false;
            }
        }

        true
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
    draw_label(
        &mut pixmap,
        width as f32 / 2.0,
        height as f32 - 30.0,
        "Esc cancel | Drag to select",
    );

    // Copy from pixmap (RGBA) to canvas (BGRA for Wayland ARGB8888)
    // tiny-skia stores as RGBA; Wayland ARGB8888 expects BGRA in memory.
    let data = pixmap.data();
    unsafe {
        let src = data.as_ptr();
        let dst = canvas.as_mut_ptr();
        let count = (width * height) as usize;
        for i in 0..count {
            let s = src.add(i * 4);
            let d = dst.add(i * 4);
            // B G R A
            d.write(s.add(2).read());
            d.add(1).write(s.add(1).read());
            d.add(2).write(s.read());
            d.add(3).write(s.add(3).read());
        }
    }
}

/// Global fontdue font (lazily initialized).
static FONT: std::sync::OnceLock<fontdue::Font> = std::sync::OnceLock::new();

fn get_font() -> &'static fontdue::Font {
    FONT.get_or_init(|| {
        let font_data = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf")
            .or_else(|_| std::fs::read("/usr/share/fonts/TTF/DejaVuSansMono.ttf"))
            .or_else(|_| std::fs::read("/usr/share/fonts/dejavu/DejaVuSansMono.ttf"))
            .unwrap_or_else(|_| include_bytes!("../../assets/DejaVuSansMono.ttf").to_vec());
        fontdue::Font::from_bytes(font_data, fontdue::FontSettings::default())
            .expect("failed to load font")
    })
}

/// Cached glyph bitmaps keyed by (char, font_size_as_u8).
use std::collections::HashMap;
use std::sync::Mutex;

type GlyphCache = HashMap<(char, u8), (fontdue::Metrics, Vec<u8>)>;
static GLYPH_CACHE: Mutex<Option<GlyphCache>> = Mutex::new(None);

fn get_glyph(ch: char, font_size: f32) -> (fontdue::Metrics, Vec<u8>) {
    let key = (ch, font_size as u8);
    let mut cache_lock = GLYPH_CACHE.lock().unwrap();
    let cache = cache_lock.get_or_insert_with(HashMap::new);
    if let Some(entry) = cache.get(&key) {
        return entry.clone();
    }
    let font = get_font();
    let result = font.rasterize(ch, font_size);
    cache.insert(key, result.clone());
    result
}

/// Draw a text label using fontdue + tiny-skia.
fn draw_label(pixmap: &mut tiny_skia::Pixmap, x: f32, y: f32, text: &str) {
    let font_size = 14.0;

    // Measure total width.
    let mut total_width = 0.0f32;
    for ch in text.chars() {
        let metrics = get_glyph(ch, font_size).0;
        total_width += metrics.advance_width;
    }

    let mut cx = x - total_width / 2.0;
    let baseline = y + font_size * 0.35;

    for ch in text.chars() {
        let (metrics, bitmap) = get_glyph(ch, font_size);
        let gw = metrics.width;
        let gh = metrics.height;
        let gx = (cx + metrics.xmin as f32).round() as i32;
        let gy = (baseline - metrics.ymin as f32 - gh as f32).round() as i32;

        // Blit glyph bitmap directly into the tiny-skia pixmap.
        let pw = pixmap.width() as i32;
        let ph = pixmap.height() as i32;
        let data = pixmap.data_mut();

        for row in 0..gh as i32 {
            let py = gy + row;
            if py < 0 || py >= ph {
                continue;
            }
            for col in 0..gw as i32 {
                let px = gx + col;
                if px < 0 || px >= pw {
                    continue;
                }
                let alpha = bitmap[(row as usize) * gw + (col as usize)];
                if alpha == 0 {
                    continue;
                }
                let idx = ((py * pw + px) * 4) as usize;
                // Source is white text; blend onto the existing background.
                let a = alpha as f32 / 255.0;
                let dst_a = data[idx + 3] as f32 / 255.0;
                let out_a = a + dst_a * (1.0 - a);
                if out_a > 0.0 {
                    let t = a / out_a;
                    data[idx] = (255.0 * t + data[idx] as f32 * (1.0 - t)) as u8;
                    data[idx + 1] = (255.0 * t + data[idx + 1] as f32 * (1.0 - t)) as u8;
                    data[idx + 2] = (255.0 * t + data[idx + 2] as f32 * (1.0 - t)) as u8;
                    data[idx + 3] = (out_a * 255.0) as u8;
                }
            }
        }
        cx += metrics.advance_width;
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
                    if self.drag_start.is_some() && self.should_redraw() {
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
                    if button == 272
                        && let Some(start) = self.drag_start
                    {
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
