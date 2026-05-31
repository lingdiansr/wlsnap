# eframe 区域选择器回退方案实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 GNOME 等不支持 `zwlr_layer_shell_v1` 的环境实现 eframe 回退区域选择器，保持与 `LayerSelector::run()` 完全一致的接口。

**架构：** 独立 `EframeSelector` 结构体实现 `eframe::App`，在 `main.rs` 中通过协议探测自动选择 layer-shell 或 eframe 路径。选择器返回 `Option<LogicalRect>` 后走 headless 捕获流程。

**Tech Stack:** Rust, eframe/egui, smithay-client-toolkit (输出枚举)

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `src/ui/eframe_selector.rs` | 新增：eframe 区域选择器实现 |
| `src/ui/mod.rs` | 修改：添加 `pub mod eframe_selector;` |
| `src/main.rs` | 修改：在 layer-shell 不可用时调用 `EframeSelector::run()` |

---

## Task 1: 创建 `EframeSelector` 核心结构

**Files:**
- Create: `src/ui/eframe_selector.rs`
- Modify: `src/ui/mod.rs`

**背景：** `LayerSelector` 使用 sctk 的 `SlotPool` + `wl_shm` 进行软件渲染，而 `EframeSelector` 使用 egui 的 `Painter` 进行绘制。两者都需要处理鼠标拖拽、键盘 Esc、选区计算。

- [ ] **Step 1: 创建 `src/ui/eframe_selector.rs` 文件头**

```rust
//! eframe-based interactive region selector fallback.
//!
//! Used when `zwlr_layer_shell_v1` is unavailable (e.g. GNOME).
//! Provides the same `run() -> Option<LogicalRect>` interface as `LayerSelector`.

use std::sync::Arc;

use egui::{Color32, FontId, Pos2, Rect, Stroke, Vec2};

use crate::platform::output_info::{LogicalPoint, LogicalRect, OutputInfo};

/// Minimum selection size in logical pixels.
const MIN_SELECTION_SIZE: f64 = 10.0;
```

- [ ] **Step 2: 定义 `EframeSelector` 结构体**

```rust
/// Interactive region selector using an eframe fullscreen window.
pub struct EframeSelector {
    /// Current output info (for global coordinate conversion).
    output: OutputInfo,
    /// Drag start position (screen coordinates).
    drag_start: Option<Pos2>,
    /// Current mouse position.
    drag_current: Pos2,
    /// Final selected region in global logical coordinates.
    selected_region: Option<LogicalRect>,
    /// Whether the user cancelled.
    cancelled: bool,
    /// Whether selection is complete (signals event loop to exit).
    done: bool,
}
```

- [ ] **Step 3: 实现 `new` 构造函数**

```rust
impl EframeSelector {
    fn new(output: OutputInfo) -> Self {
        Self {
            output,
            drag_start: None,
            drag_current: Pos2::ZERO,
            selected_region: None,
            cancelled: false,
            done: false,
        }
    }
}
```

- [ ] **Step 4: 在 `src/ui/mod.rs` 中添加模块声明**

```rust
pub mod eframe_selector;
```

- [ ] **Step 5: 验证编译**

Run: `cargo check`
Expected: 编译通过（可能有未使用字段警告，忽略）

- [ ] **Step 6: Commit**

```bash
git add src/ui/eframe_selector.rs src/ui/mod.rs
git commit -m "feat(ui): add EframeSelector structure for GNOME fallback"
```

---

## Task 2: 实现 `eframe::App` 渲染与输入处理

**Files:**
- Modify: `src/ui/eframe_selector.rs`

**背景：** eframe 的 `App` trait 有 `update` 方法，每帧调用。我们需要：
1. 绘制全屏半透明遮罩
2. 处理鼠标拖拽绘制选框
3. 处理 Esc 取消
4. 选区完成后退出事件循环

- [ ] **Step 1: 实现 `eframe::App` trait**

在 `src/ui/eframe_selector.rs` 末尾添加：

```rust
impl eframe::App for EframeSelector {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Handle Esc key
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cancelled = true;
            self.done = true;
            frame.close();
            return;
        }

        // Handle mouse input
        let pointer = ctx.input(|i| i.pointer.clone());
        
        if pointer.any_pressed() {
            if let Some(pos) = pointer.interact_pos() {
                self.drag_start = Some(pos);
                self.drag_current = pos;
            }
        }
        
        if pointer.is_moving() {
            if let Some(pos) = pointer.interact_pos() {
                self.drag_current = pos;
            }
        }
        
        if pointer.any_released() {
            if let Some(start) = self.drag_start {
                let rect = Rect::from_two_pos(start, self.drag_current);
                let w = (rect.max.x - rect.min.x) as f64;
                let h = (rect.max.y - rect.min.y) as f64;
                
                if w >= MIN_SELECTION_SIZE && h >= MIN_SELECTION_SIZE {
                    // Convert screen coordinates to global logical coordinates
                    let offset_x = self.output.logical_geometry.min.x;
                    let offset_y = self.output.logical_geometry.min.y;
                    
                    self.selected_region = Some(LogicalRect {
                        min: LogicalPoint {
                            x: rect.min.x as f64 + offset_x,
                            y: rect.min.y as f64 + offset_y,
                        },
                        max: LogicalPoint {
                            x: rect.max.x as f64 + offset_x,
                            y: rect.max.y as f64 + offset_y,
                        },
                    });
                }
                
                self.done = true;
                frame.close();
                return;
            }
        }

        // Render
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                let screen_rect = ctx.screen_rect();
                let painter = ui.painter_at(screen_rect);

                // Full-screen semi-transparent mask
                painter.rect_filled(
                    screen_rect,
                    0.0,
                    Color32::from_black_alpha(128),
                );

                // Draw selection rectangle if dragging
                if let Some(start) = self.drag_start {
                    let rect = Rect::from_two_pos(*start, self.drag_current);
                    
                    // Highlight fill
                    painter.rect_filled(rect, 0.0, Color32::from_white_alpha(32));
                    
                    // White border
                    painter.rect_stroke(rect, 0.0, Stroke::new(2.0, Color32::WHITE));
                    
                    // Size label at bottom-right of selection
                    let label = format!(
                        "{} x {}",
                        (rect.max.x - rect.min.x).abs() as i32,
                        (rect.max.y - rect.min.y).abs() as i32
                    );
                    painter.text(
                        rect.right_bottom() + Vec2::new(-8.0, -8.0),
                        egui::Align2::RIGHT_BOTTOM,
                        label,
                        FontId::proportional(14.0),
                        Color32::WHITE,
                    );
                }

                // Hint text at bottom center
                painter.text(
                    screen_rect.center_bottom() + Vec2::new(0.0, -30.0),
                    egui::Align2::CENTER_BOTTOM,
                    "Esc cancel | Drag to select",
                    FontId::proportional(14.0),
                    Color32::WHITE,
                );
            });

        // Request continuous repaint while dragging for smooth updates
        if self.drag_start.is_some() {
            ctx.request_repaint();
        }
    }
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check`
Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add src/ui/eframe_selector.rs
git commit -m "feat(ui): implement eframe App trait for area selection"
```

---

## Task 3: 实现 `EframeSelector::run()` 静态方法

**Files:**
- Modify: `src/ui/eframe_selector.rs`

**背景：** `run()` 是公共入口，需要：
1. 获取当前指针所在的输出
2. 创建全屏无边框 eframe 窗口
3. 运行事件循环
4. 返回选区结果

由于 `eframe::run_native` 会消耗 `self`，我们需要用 `Arc<Mutex<>>` 或 `std::sync::mpsc` 来传回结果。这里使用 `std::sync::mpsc` 最简单。

- [ ] **Step 1: 添加必要的 import**

在文件顶部添加：

```rust
use std::sync::mpsc;
```

- [ ] **Step 2: 实现 `run()` 方法**

在 `impl EframeSelector` 中添加：

```rust
impl EframeSelector {
    /// Create and run the selector, returning the selected region or None if cancelled.
    pub fn run() -> Option<LogicalRect> {
        // 1. Get the current output (where the pointer is)
        let outputs = match crate::platform::wayland::enumerate_outputs() {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("Failed to enumerate outputs: {}", e);
                return None;
            }
        };
        
        if outputs.is_empty() {
            tracing::warn!("No outputs detected for eframe selector");
            return None;
        }
        
        // For now, use the first output as the current screen.
        // TODO: In the future, determine which output contains the pointer.
        let output = outputs.into_iter().next().unwrap();
        
        // 2. Create a channel to receive the result
        let (tx, rx) = mpsc::channel::<Option<LogicalRect>>();
        
        // 3. Create fullscreen eframe window options
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_fullscreen(true)
                .with_always_on_top(true),
            ..Default::default()
        };
        
        // 4. Run the selector
        let selector = Self::new(output);
        
        // We need to send the result back when the app closes.
        // Wrap the selector and transmitter in a struct that implements App.
        let mut app = SelectorApp {
            selector,
            tx: Some(tx),
        };
        
        let _ = eframe::run_native(
            "wlsnap-area",
            options,
            Box::new(|_cc| Ok(Box::new(app))),
        );
        
        // 5. Receive the result
        rx.recv().unwrap_or(None)
    }
}

/// Wrapper to send result back via channel when eframe exits.
struct SelectorApp {
    selector: EframeSelector,
    tx: Option<mpsc::Sender<Option<LogicalRect>>>,
}

impl eframe::App for SelectorApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.selector.update(ctx, frame);
        
        // Send result when done
        if self.selector.done {
            if let Some(tx) = self.tx.take() {
                let result = if self.selector.cancelled {
                    None
                } else {
                    self.selector.selected_region
                };
                let _ = tx.send(result);
            }
        }
    }
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add src/ui/eframe_selector.rs
git commit -m "feat(ui): implement EframeSelector::run() with result channel"
```

---

## Task 4: 在 `main.rs` 中集成 eframe 回退

**Files:**
- Modify: `src/main.rs`

**背景：** 当前 `main.rs` 在 `has_layer_shell()` 为 false 时直接 fall through 到 GUI 模式。需要改为调用 `EframeSelector::run()`。

- [ ] **Step 1: 修改 `main.rs` 中的选择器调用**

找到这段代码：

```rust
if cli.mode.area.as_ref().is_some_and(|s| s.is_empty()) {
    let probe = wlsnap::backend::probe_all();
    if probe.has_layer_shell() {
        match wlsnap::ui::layer_selector::LayerSelector::run() {
            // ...
        }
    }
    // Fall through to eframe fallback if layer-shell is unavailable.
}
```

替换为：

```rust
if cli.mode.area.as_ref().is_some_and(|s| s.is_empty()) {
    let probe = wlsnap::backend::probe_all();
    let region = if probe.has_layer_shell() {
        wlsnap::ui::layer_selector::LayerSelector::run()
    } else {
        wlsnap::ui::eframe_selector::EframeSelector::run()
    };
    
    match region {
        Some(region) => {
            let mut cli = cli;
            cli.mode.area = Some(format!(
                "{},{},{},{}",
                region.min.x.round() as i64,
                region.min.y.round() as i64,
                (region.max.x - region.min.x).round() as i64,
                (region.max.y - region.min.y).round() as i64,
            ));
            match cli_action::run_cli_capture(&cli, &config) {
                Ok(path) => {
                    tracing::info!("Output dispatched: {:?}", path);
                    return Ok(());
                }
                Err(e) => {
                    tracing::error!("Output dispatch failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        None => {
            tracing::info!("Area selection cancelled.");
            return Ok(());
        }
    }
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check`
Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat(main): integrate EframeSelector fallback for --area on GNOME"
```

---

## Task 5: 添加单元测试

**Files:**
- Modify: `src/ui/eframe_selector.rs`

**背景：** 需要测试坐标转换逻辑（屏幕坐标 → 全局逻辑坐标）。由于 eframe 的渲染和输入难以在单元测试中模拟，我们主要测试纯逻辑函数。

- [ ] **Step 1: 提取坐标转换函数以便测试**

在 `impl EframeSelector` 中添加：

```rust
/// Convert an egui Rect (screen coordinates) to a global LogicalRect.
fn rect_to_global_logical(rect: &Rect, output: &OutputInfo) -> LogicalRect {
    let offset_x = output.logical_geometry.min.x;
    let offset_y = output.logical_geometry.min.y;
    
    LogicalRect {
        min: LogicalPoint {
            x: rect.min.x as f64 + offset_x,
            y: rect.min.y as f64 + offset_y,
        },
        max: LogicalPoint {
            x: rect.max.x as f64 + offset_x,
            y: rect.max.y as f64 + offset_y,
        },
    }
}
```

然后在 `update` 方法中的选区保存处替换为调用此函数：

```rust
self.selected_region = Some(rect_to_global_logical(&rect, &self.output));
```

- [ ] **Step 2: 添加测试模块**

在文件末尾添加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::output_info::{LogicalPoint, LogicalRect, OutputTransform};

    fn make_output(x: f64, y: f64, w: f64, h: f64) -> OutputInfo {
        OutputInfo {
            name: "test".to_string(),
            description: String::new(),
            logical_geometry: LogicalRect {
                min: LogicalPoint { x, y },
                max: LogicalPoint { x: x + w, y: y + h },
            },
            physical_size: (w as u32, h as u32),
            scale_factor: 1.0,
            transform: OutputTransform::Normal,
        }
    }

    #[test]
    fn rect_to_global_logical_at_origin() {
        let output = make_output(0.0, 0.0, 1920.0, 1080.0);
        let rect = Rect::from_min_max(Pos2::new(100.0, 200.0), Pos2::new(500.0, 600.0));
        let region = rect_to_global_logical(&rect, &output);
        
        assert_eq!(region.min.x, 100.0);
        assert_eq!(region.min.y, 200.0);
        assert_eq!(region.max.x, 500.0);
        assert_eq!(region.max.y, 600.0);
    }

    #[test]
    fn rect_to_global_logical_with_offset() {
        let output = make_output(1920.0, 0.0, 1920.0, 1080.0);
        let rect = Rect::from_min_max(Pos2::new(100.0, 200.0), Pos2::new(500.0, 600.0));
        let region = rect_to_global_logical(&rect, &output);
        
        assert_eq!(region.min.x, 2020.0); // 1920 + 100
        assert_eq!(region.min.y, 200.0);
        assert_eq!(region.max.x, 2420.0); // 1920 + 500
        assert_eq!(region.max.y, 600.0);
    }

    #[test]
    fn rect_to_global_logical_negative_coords() {
        // Rect where min > max (dragged from bottom-right to top-left)
        let output = make_output(0.0, 0.0, 1920.0, 1080.0);
        let rect = Rect::from_min_max(Pos2::new(500.0, 600.0), Pos2::new(100.0, 200.0));
        let region = rect_to_global_logical(&rect, &output);
        
        // egui::Rect::from_two_pos normalizes, so min is (100, 200), max is (500, 600)
        assert_eq!(region.min.x, 100.0);
        assert_eq!(region.min.y, 200.0);
        assert_eq!(region.max.x, 500.0);
        assert_eq!(region.max.y, 600.0);
    }
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test ui::eframe_selector::tests --lib`
Expected: 所有测试通过

- [ ] **Step 4: Commit**

```bash
git add src/ui/eframe_selector.rs
git commit -m "test(ui): add coordinate conversion tests for eframe selector"
```

---

## Task 6: 运行完整测试套件

- [ ] **Step 1: 运行所有测试**

Run: `cargo test`
Expected: 所有测试通过（包括新添加的 eframe_selector 测试和现有测试）

- [ ] **Step 2: 运行 clippy**

Run: `cargo clippy --all-targets --all-features`
Expected: 无警告

- [ ] **Step 3: Commit**

```bash
git commit --allow-empty -m "chore: verify full test suite passes after eframe selector integration"
```

---

## 自检清单

### Spec 覆盖检查

| Spec 要求 | 实现任务 |
|-----------|---------|
| 与 `LayerSelector::run()` 接口一致 | Task 3: `EframeSelector::run() -> Option<LogicalRect>` |
| 全屏无边框窗口 | Task 3: `with_decorations(false) + with_fullscreen(true)` |
| 半透明遮罩 | Task 2: `Color32::from_black_alpha(128)` |
| 拖拽选框 | Task 2: 鼠标事件处理 + 白色边框 + 高亮填充 |
| 尺寸标签 | Task 2: 右下角显示 `Width x Height` |
| Esc 取消 | Task 2: `key_pressed(egui::Key::Escape)` |
| 坐标转换（全局逻辑坐标） | Task 3/5: `rect_to_global_logical` |
| 主流程集成 | Task 4: `main.rs` 协议探测分支 |
| 边界条件（< 10px 取消） | Task 2: `MIN_SELECTION_SIZE` 检查 |

### Placeholder 扫描

- [x] 无 "TBD"/"TODO"（除了一个合理的 TODO：未来支持指针所在屏幕检测）
- [x] 无模糊描述
- [x] 所有代码步骤包含完整代码

### 类型一致性

- [x] `LogicalRect` / `LogicalPoint` 与现有代码一致
- [x] `OutputInfo` 与现有代码一致
- [x] `eframe::App` trait 签名正确
