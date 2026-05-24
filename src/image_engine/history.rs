//! Undo/redo history stack using the Command pattern.

use crate::image_engine::PhysicalRect;

/// A reversible image-editing command.
pub trait Command: Send + Sync {
    /// Apply the command to the canvas.
    fn execute(&self, canvas: &mut tiny_skia::Pixmap);
    /// Revert the command on the canvas.
    fn undo(&self, canvas: &mut tiny_skia::Pixmap);
    /// Human-readable description for UI/debugging.
    fn describe(&self) -> &'static str;
    /// Region affected by this command for dirty-rect optimisation.
    fn affected_region(&self) -> Option<PhysicalRect> {
        None
    }
}

/// History stack supporting unlimited undo/redo up to `max_depth`.
pub struct HistoryStack {
    commands: Vec<Box<dyn Command>>,
    undone: Vec<Box<dyn Command>>,
    max_depth: usize,
}

impl HistoryStack {
    /// Create a new stack with the given maximum depth.
    pub fn new(max_depth: usize) -> Self {
        Self {
            commands: Vec::new(),
            undone: Vec::new(),
            max_depth,
        }
    }

    /// Execute `cmd` on `canvas`, push it onto the stack, and clear the redo queue.
    pub fn push(&mut self, cmd: Box<dyn Command>, canvas: &mut tiny_skia::Pixmap) {
        cmd.execute(canvas);
        self.commands.push(cmd);
        self.undone.clear();

        if self.max_depth > 0 && self.commands.len() > self.max_depth {
            self.commands.remove(0);
        }
    }

    /// Undo the most recent command.
    ///
    /// Returns `Some(&dyn Command)` if a command was undone, otherwise `None`.
    pub fn undo(&mut self, canvas: &mut tiny_skia::Pixmap) -> Option<&dyn Command> {
        let cmd = self.commands.pop()?;
        cmd.undo(canvas);
        let ptr: *const dyn Command = &*cmd;
        self.undone.push(cmd);
        // SAFETY: we just pushed it, so the reference is valid as long as the
        // caller does not mutate the stack.
        Some(unsafe { &*ptr })
    }

    /// Redo the most recently undone command.
    ///
    /// Returns `Some(&dyn Command)` if a command was redone, otherwise `None`.
    pub fn redo(&mut self, canvas: &mut tiny_skia::Pixmap) -> Option<&dyn Command> {
        let cmd = self.undone.pop()?;
        cmd.execute(canvas);
        let ptr: *const dyn Command = &*cmd;
        self.commands.push(cmd);
        Some(unsafe { &*ptr })
    }

    /// Whether there is at least one command that can be undone.
    pub fn can_undo(&self) -> bool {
        !self.commands.is_empty()
    }

    /// Whether there is at least one command that can be redone.
    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Number of commands in the undo stack.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether the undo stack is empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Clear both the undo and redo stacks.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.undone.clear();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Fills a rectangle with opaque red.
    struct FillRedCommand {
        rect: tiny_skia::IntRect,
    }

    impl Command for FillRedCommand {
        fn execute(&self, canvas: &mut tiny_skia::Pixmap) {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(255, 0, 0, 255);
            canvas.fill_rect(
                self.rect.to_rect(),
                &paint,
                tiny_skia::Transform::identity(),
                None,
            );
        }

        fn undo(&self, canvas: &mut tiny_skia::Pixmap) {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(0, 0, 0, 0);
            paint.blend_mode = tiny_skia::BlendMode::Source;
            canvas.fill_rect(
                self.rect.to_rect(),
                &paint,
                tiny_skia::Transform::identity(),
                None,
            );
        }

        fn describe(&self) -> &'static str {
            "fill red"
        }
    }

    /// Fills a rectangle with opaque blue.
    struct FillBlueCommand {
        rect: tiny_skia::IntRect,
    }

    impl Command for FillBlueCommand {
        fn execute(&self, canvas: &mut tiny_skia::Pixmap) {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(0, 0, 255, 255);
            canvas.fill_rect(
                self.rect.to_rect(),
                &paint,
                tiny_skia::Transform::identity(),
                None,
            );
        }

        fn undo(&self, canvas: &mut tiny_skia::Pixmap) {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(0, 0, 0, 0);
            paint.blend_mode = tiny_skia::BlendMode::Source;
            canvas.fill_rect(
                self.rect.to_rect(),
                &paint,
                tiny_skia::Transform::identity(),
                None,
            );
        }

        fn describe(&self) -> &'static str {
            "fill blue"
        }
    }

    fn black_pixmap(width: u32, height: u32) -> tiny_skia::Pixmap {
        let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
        pixmap
    }

    fn pixel(pixmap: &tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * pixmap.width() + x) * 4) as usize;
        let data = pixmap.data();
        [data[idx], data[idx + 1], data[idx + 2], data[idx + 3]]
    }

    #[test]
    fn push_changes_canvas() {
        let mut canvas = black_pixmap(4, 4);
        let mut stack = HistoryStack::new(10);

        let rect = tiny_skia::IntRect::from_xywh(1, 1, 2, 2).unwrap();
        stack.push(Box::new(FillRedCommand { rect }), &mut canvas);

        assert_eq!(pixel(&canvas, 1, 1), [255, 0, 0, 255]);
        assert_eq!(pixel(&canvas, 2, 2), [255, 0, 0, 255]);
        // Outside the rect should stay black
        assert_eq!(pixel(&canvas, 0, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn undo_restores_canvas() {
        let mut canvas = black_pixmap(4, 4);
        let mut stack = HistoryStack::new(10);

        let rect = tiny_skia::IntRect::from_xywh(1, 1, 2, 2).unwrap();
        stack.push(Box::new(FillRedCommand { rect }), &mut canvas);
        assert_eq!(pixel(&canvas, 1, 1), [255, 0, 0, 255]);

        stack.undo(&mut canvas);
        assert_eq!(pixel(&canvas, 1, 1), [0, 0, 0, 0]); // undo fills transparent
    }

    #[test]
    fn redo_changes_canvas_again() {
        let mut canvas = black_pixmap(4, 4);
        let mut stack = HistoryStack::new(10);

        let rect = tiny_skia::IntRect::from_xywh(1, 1, 2, 2).unwrap();
        stack.push(Box::new(FillRedCommand { rect }), &mut canvas);
        stack.undo(&mut canvas);
        assert_eq!(pixel(&canvas, 1, 1), [0, 0, 0, 0]);

        stack.redo(&mut canvas);
        assert_eq!(pixel(&canvas, 1, 1), [255, 0, 0, 255]);
    }

    #[test]
    fn push_after_undo_clears_redo_stack() {
        let mut canvas = black_pixmap(4, 4);
        let mut stack = HistoryStack::new(10);

        let r1 = tiny_skia::IntRect::from_xywh(1, 1, 1, 1).unwrap();
        let r2 = tiny_skia::IntRect::from_xywh(2, 2, 1, 1).unwrap();

        stack.push(Box::new(FillRedCommand { rect: r1 }), &mut canvas);
        stack.push(Box::new(FillBlueCommand { rect: r2 }), &mut canvas);

        stack.undo(&mut canvas);
        assert!(stack.can_redo());

        // Push a new command – redo stack should be cleared
        let r3 = tiny_skia::IntRect::from_xywh(0, 0, 1, 1).unwrap();
        stack.push(Box::new(FillRedCommand { rect: r3 }), &mut canvas);
        assert!(!stack.can_redo());
        assert_eq!(stack.len(), 2);
    }

    #[test]
    fn max_depth_evicts_oldest() {
        let mut canvas = black_pixmap(8, 8);
        let mut stack = HistoryStack::new(2);

        let r1 = tiny_skia::IntRect::from_xywh(0, 0, 1, 1).unwrap();
        let r2 = tiny_skia::IntRect::from_xywh(1, 1, 1, 1).unwrap();
        let r3 = tiny_skia::IntRect::from_xywh(2, 2, 1, 1).unwrap();

        stack.push(Box::new(FillRedCommand { rect: r1 }), &mut canvas);
        stack.push(Box::new(FillRedCommand { rect: r2 }), &mut canvas);
        stack.push(Box::new(FillRedCommand { rect: r3 }), &mut canvas);

        assert_eq!(stack.len(), 2);
        // The oldest command (r1) was evicted, so undo twice should leave us
        // with only r3 undone and r2 still applied? No, let's think:
        // After 3 pushes with max_depth=2: commands = [r2, r3]
        // Undo once -> commands = [r2], undone = [r3]
        // Undo again -> commands = [], undone = [r3, r2]
        assert!(stack.can_undo());
        stack.undo(&mut canvas);
        assert!(stack.can_undo());
        stack.undo(&mut canvas);
        assert!(!stack.can_undo());
    }

    #[test]
    fn can_undo_and_can_redo_state() {
        let mut canvas = black_pixmap(2, 2);
        let mut stack = HistoryStack::new(10);

        assert!(!stack.can_undo());
        assert!(!stack.can_redo());

        let rect = tiny_skia::IntRect::from_xywh(0, 0, 1, 1).unwrap();
        stack.push(Box::new(FillRedCommand { rect }), &mut canvas);
        assert!(stack.can_undo());
        assert!(!stack.can_redo());

        stack.undo(&mut canvas);
        assert!(!stack.can_undo());
        assert!(stack.can_redo());

        stack.redo(&mut canvas);
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn clear_empties_both_stacks() {
        let mut canvas = black_pixmap(2, 2);
        let mut stack = HistoryStack::new(10);

        let r1 = tiny_skia::IntRect::from_xywh(0, 0, 1, 1).unwrap();
        let r2 = tiny_skia::IntRect::from_xywh(1, 1, 1, 1).unwrap();

        stack.push(Box::new(FillRedCommand { rect: r1 }), &mut canvas);
        stack.push(Box::new(FillBlueCommand { rect: r2 }), &mut canvas);
        stack.undo(&mut canvas);

        assert!(stack.can_undo());
        assert!(stack.can_redo());

        stack.clear();
        assert!(!stack.can_undo());
        assert!(!stack.can_redo());
        assert!(stack.is_empty());
    }

    #[test]
    fn undo_returns_command_reference() {
        let mut canvas = black_pixmap(2, 2);
        let mut stack = HistoryStack::new(10);

        let rect = tiny_skia::IntRect::from_xywh(0, 0, 1, 1).unwrap();
        stack.push(Box::new(FillRedCommand { rect }), &mut canvas);

        let cmd = stack.undo(&mut canvas);
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().describe(), "fill red");
    }

    #[test]
    fn redo_returns_command_reference() {
        let mut canvas = black_pixmap(2, 2);
        let mut stack = HistoryStack::new(10);

        let rect = tiny_skia::IntRect::from_xywh(0, 0, 1, 1).unwrap();
        stack.push(Box::new(FillRedCommand { rect }), &mut canvas);
        stack.undo(&mut canvas);

        let cmd = stack.redo(&mut canvas);
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().describe(), "fill red");
    }

    #[test]
    fn affected_region_default_is_none() {
        let rect = tiny_skia::IntRect::from_xywh(0, 0, 1, 1).unwrap();
        let cmd = FillRedCommand { rect };
        assert!(cmd.affected_region().is_none());
    }
}
