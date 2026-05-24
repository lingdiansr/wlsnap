use crate::image_engine::{LogicalPoint, pixmap::rgba_to_pixmap};

/// Annotation editor panel.
pub struct Editor {
    pub image: image::RgbaImage,
    pub pixmap: tiny_skia::Pixmap,
    pub texture: Option<egui::TextureHandle>,
    pub viewport: EditorViewport,
}

/// Viewport state for the editor canvas.
#[derive(Debug, Clone, Copy)]
pub struct EditorViewport {
    pub offset: LogicalPoint,
    pub zoom: f64,
    pub min_zoom: f64,
    pub max_zoom: f64,
}

impl Default for EditorViewport {
    fn default() -> Self {
        Self {
            offset: LogicalPoint { x: 0.0, y: 0.0 },
            zoom: 1.0,
            min_zoom: 0.1,
            max_zoom: 10.0,
        }
    }
}

impl EditorViewport {
    /// Apply a zoom factor centered on a screen point.
    pub fn zoom_at(&mut self, factor: f64, screen_point: egui::Pos2) {
        let new_zoom = (self.zoom * factor).clamp(self.min_zoom, self.max_zoom);
        let canvas_pos = screen_to_canvas(screen_point, self);
        self.zoom = new_zoom;
        let new_screen = canvas_to_screen(canvas_pos, self);
        self.offset.x += (screen_point.x - new_screen.x) as f64;
        self.offset.y += (screen_point.y - new_screen.y) as f64;
    }

    /// Pan the viewport by a screen-space delta.
    pub fn pan(&mut self, delta: egui::Vec2) {
        self.offset.x += delta.x as f64;
        self.offset.y += delta.y as f64;
    }
}

impl Editor {
    /// Create a new editor from an RGBA image.
    pub fn new(image: image::RgbaImage) -> Self {
        let pixmap = rgba_to_pixmap(&image).expect("Failed to create pixmap from image");
        Self {
            image,
            pixmap,
            texture: None,
            viewport: EditorViewport::default(),
        }
    }

    /// Render the editor panel.
    pub fn show(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.update_texture(ctx);

        let available = ui.available_size();
        let status_height = 24.0f32;
        let canvas_size = egui::vec2(available.x, (available.y - status_height).max(1.0));

        let canvas_response = ui.allocate_response(canvas_size, egui::Sense::click_and_drag());

        if let Some(texture) = &self.texture {
            let zoom = self.viewport.zoom as f32;
            let img_size = egui::vec2(
                self.image.width() as f32 * zoom,
                self.image.height() as f32 * zoom,
            );
            let img_rect = egui::Rect::from_min_size(
                egui::pos2(self.viewport.offset.x as f32, self.viewport.offset.y as f32),
                img_size,
            );
            ui.put(img_rect, egui::Image::new((texture.id(), img_size)));
        }

        self.handle_input(ctx, &canvas_response);

        ui.horizontal(|ui| {
            ui.label(format!("Zoom: {:.0}%", self.viewport.zoom * 100.0));
            ui.separator();
            ui.label(format!("{}x{}", self.image.width(), self.image.height()));
        });
    }

    /// Handle zoom, pan and reset input.
    pub fn handle_input(&mut self, ctx: &egui::Context, response: &egui::Response) {
        // Mouse wheel zoom (centered on pointer)
        if response.hovered() {
            let scroll_delta = ctx.input(|i| i.smooth_scroll_delta);
            if scroll_delta.y != 0.0 {
                let factor = if scroll_delta.y > 0.0 { 1.1 } else { 1.0 / 1.1 };
                if let Some(pointer_pos) = response.hover_pos() {
                    self.viewport.zoom_at(factor, pointer_pos);
                }
            }
        }

        // Middle-click drag pan
        if response.dragged_by(egui::PointerButton::Middle) {
            self.viewport.pan(response.drag_delta());
        }

        // Space + left-drag pan
        let space_pressed = ctx.input(|i| i.key_down(egui::Key::Space));
        if space_pressed && response.dragged_by(egui::PointerButton::Primary) {
            self.viewport.pan(response.drag_delta());
        }

        // Ctrl+0 reset zoom
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Num0)) {
            self.viewport.zoom = 1.0;
        }
    }

    /// Sync pixmap to GPU texture. For T12 this only creates the texture once.
    pub fn update_texture(&mut self, ctx: &egui::Context) {
        if self.texture.is_some() {
            return;
        }
        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [self.image.width() as usize, self.image.height() as usize],
            self.image.as_raw(),
        );
        self.texture = Some(ctx.load_texture(
            "editor_image",
            color_image,
            egui::TextureOptions::LINEAR,
        ));
    }
}

/// Convert a screen position to canvas (image-logical) coordinates.
fn screen_to_canvas(screen: egui::Pos2, viewport: &EditorViewport) -> egui::Pos2 {
    egui::pos2(
        (((screen.x as f64) - viewport.offset.x) / viewport.zoom) as f32,
        (((screen.y as f64) - viewport.offset.y) / viewport.zoom) as f32,
    )
}

/// Convert a canvas (image-logical) position to screen coordinates.
fn canvas_to_screen(canvas: egui::Pos2, viewport: &EditorViewport) -> egui::Pos2 {
    egui::pos2(
        (canvas.x as f64 * viewport.zoom + viewport.offset.x) as f32,
        (canvas.y as f64 * viewport.zoom + viewport.offset.y) as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_default() {
        let vp = EditorViewport::default();
        assert_eq!(vp.offset.x, 0.0);
        assert_eq!(vp.offset.y, 0.0);
        assert_eq!(vp.zoom, 1.0);
        assert_eq!(vp.min_zoom, 0.1);
        assert_eq!(vp.max_zoom, 10.0);
    }

    #[test]
    fn screen_canvas_roundtrip_zoom_1() {
        let viewport = EditorViewport {
            offset: LogicalPoint { x: 10.0, y: 20.0 },
            zoom: 1.0,
            min_zoom: 0.1,
            max_zoom: 10.0,
        };
        let screen = egui::pos2(50.0, 70.0);
        let canvas = screen_to_canvas(screen, &viewport);
        let back = canvas_to_screen(canvas, &viewport);
        assert!((back.x - screen.x).abs() < 1e-5);
        assert!((back.y - screen.y).abs() < 1e-5);
    }

    #[test]
    fn screen_canvas_roundtrip_zoom_2() {
        let viewport = EditorViewport {
            offset: LogicalPoint { x: 10.0, y: 20.0 },
            zoom: 2.0,
            min_zoom: 0.1,
            max_zoom: 10.0,
        };
        let screen = egui::pos2(50.0, 70.0);
        let canvas = screen_to_canvas(screen, &viewport);
        let back = canvas_to_screen(canvas, &viewport);
        assert!((back.x - screen.x).abs() < 1e-5);
        assert!((back.y - screen.y).abs() < 1e-5);
    }

    #[test]
    fn zoom_clamping() {
        let mut viewport = EditorViewport::default();
        let point = egui::pos2(100.0, 100.0);

        viewport.zoom_at(100.0, point);
        assert_eq!(viewport.zoom, viewport.max_zoom);

        viewport.zoom_at(0.001, point);
        assert_eq!(viewport.zoom, viewport.min_zoom);
    }

    #[test]
    fn editor_new_with_red_image() {
        let mut img = image::RgbaImage::new(100, 100);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba([255, 0, 0, 255]);
        }
        let editor = Editor::new(img);
        assert_eq!(editor.image.dimensions(), (100, 100));
        assert_eq!(editor.pixmap.width(), 100);
        assert_eq!(editor.pixmap.height(), 100);
        assert!(editor.texture.is_none());
    }
}
