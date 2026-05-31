# eframe 区域选择器回退方案设计文档

> 为 GNOME 等不支持 `zwlr_layer_shell_v1` 的桌面环境提供交互式 `--area` 回退实现。  
> 版本: 1.0 | 日期: 2026-05-29

---

## 1. 背景与目标

### 1.1 背景

wlsnap 的交互式区域截图 (`--area` 不带坐标) 当前使用 `LayerSelector`，基于 `smithay-client-toolkit` 的 `zwlr_layer_shell_v1` Overlay 层实现。该方案在 wlroots 系 compositor（Hyprland、Sway、Niri 等）上体验最佳，但 **GNOME/Mutter 不实现 `wlr-layer-shell` 协议**，因此需要回退方案。

### 1.2 目标

- 在 GNOME 等无 layer-shell 的环境中，`--area` 仍能正常工作
- 回退方案与 layer-shell 方案保持**完全一致的外部接口**
- 用户体验尽可能接近：全屏覆盖、半透明遮罩、拖拽选框、Esc 取消
- 不改动 `WlsnapApp` 状态机或主流程逻辑

---

## 2. 架构设计

### 2.1 整体流程

```
main.rs
│
├─ 解析 CLI: --area (无坐标)
├─ 探测协议: has_layer_shell()?
│   ├─ true  → LayerSelector::run()  → Option<LogicalRect>
│   └─ false → EframeSelector::run() → Option<LogicalRect>
│
└─ 两者返回相同的 Option<LogicalRect>
   ├─ Some(region) → 转换为 "x,y,w,h" → run_cli_capture() → 退出
   └─ None         → 用户取消 → 退出
```

### 2.2 模块位置

```
src/ui/
├── mod.rs                 # pub mod eframe_selector;
├── layer_selector.rs      # LayerSelector (sctk + layer-shell)
└── eframe_selector.rs     # EframeSelector (eframe + egui)  ← 新增
```

### 2.3 接口定义

两个选择器实现完全一致的公共接口：

```rust
/// 交互式区域选择器 trait（由 LayerSelector 和 EframeSelector 实现）
pub trait RegionSelector {
    /// 运行选择器，返回用户选择的逻辑坐标区域，或 None（取消）。
    fn run() -> Option<LogicalRect>;
}
```

实际代码中不引入 trait（避免复杂度），仅通过相同的函数签名保证一致性：

```rust
// src/ui/layer_selector.rs
impl LayerSelector {
    pub fn run() -> Option<LogicalRect> { ... }
}

// src/ui/eframe_selector.rs
impl EframeSelector {
    pub fn run() -> Option<LogicalRect> { ... }
}
```

---

## 3. EframeSelector 详细设计

### 3.1 核心结构

```rust
pub struct EframeSelector {
    /// 当前输出信息（用于确定全屏尺寸）
    output: OutputInfo,
    /// 选区起点（逻辑坐标）
    drag_start: Option<egui::Pos2>,
    /// 当前鼠标位置
    drag_current: egui::Pos2,
    /// 最终选区结果
    selected_region: Option<LogicalRect>,
    /// 是否取消
    cancelled: bool,
    /// 是否完成（用于退出事件循环）
    done: bool,
}
```

### 3.2 运行流程

```rust
impl EframeSelector {
    pub fn run() -> Option<LogicalRect> {
        // 1. 获取当前指针所在的输出
        let output = Self::current_output()?;
        
        // 2. 创建全屏无边框 eframe 窗口
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_fullscreen(true)
                .with_always_on_top(true),
            ..Default::default()
        };
        
        // 3. 运行 eframe 事件循环
        let mut selector = Self::new(output);
        eframe::run_native("wlsnap-area", options, Box::new(|_cc| Ok(Box::new(selector))));
        
        // 4. 返回结果
        if selector.cancelled {
            None
        } else {
            selector.selected_region
        }
    }
}
```

### 3.3 渲染逻辑

在 `eframe::App::update()` 中：

1. **背景遮罩**：使用 `ui.painter_at(screen_rect)` 绘制全屏半透明黑色 (`Color32::from_black_alpha(128)`)
2. **选区高亮**：拖拽时绘制白色边框矩形 + 内部半透明高亮
3. **尺寸标签**：在选框右下角显示 `Width x Height`
4. **提示文字**：底部居中显示 "Esc 取消 | 拖拽选择"

```rust
fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
    let screen_rect = ctx.screen_rect();
    
    // 全屏遮罩
    ui.painter_at(screen_rect).rect_filled(
        screen_rect, 0.0, egui::Color32::from_black_alpha(128)
    );
    
    // 选区绘制
    if let Some(start) = self.drag_start {
        let rect = egui::Rect::from_two_pos(start, self.drag_current);
        ui.painter_at(screen_rect).rect_stroke(rect, 0.0, egui::Stroke::new(2.0, egui::Color32::WHITE));
        ui.painter_at(screen_rect).rect_filled(rect, 0.0, egui::Color32::from_white_alpha(32));
        
        // 尺寸标签
        let label = format!("{} x {}", rect.width() as i32, rect.height() as i32);
        ui.painter_at(screen_rect).text(
            rect.right_bottom() + egui::vec2(-8.0, -8.0),
            egui::Align2::RIGHT_BOTTOM,
            label,
            egui::FontId::proportional(14.0),
            egui::Color32::WHITE,
        );
    }
    
    // 提示文字
    ui.painter_at(screen_rect).text(
        screen_rect.center_bottom() + egui::vec2(0.0, -30.0),
        egui::Align2::CENTER_BOTTOM,
        "Esc 取消 | 拖拽选择",
        egui::FontId::proportional(14.0),
        egui::Color32::WHITE,
    );
}
```

### 3.4 输入处理

| 输入 | 行为 |
|------|------|
| 左键按下 | 记录 `drag_start` |
| 鼠标移动 | 更新 `drag_current`，请求重绘 |
| 左键释放 | 计算选区，若尺寸 ≥ 10px 则保存，标记 `done = true` |
| Esc 键 | 标记 `cancelled = true`，`done = true` |

### 3.5 坐标转换

eframe 的坐标是逻辑像素（与 Wayland logical coordinates 一致），因此无需额外转换：

```rust
// eframe Pos2 直接对应 LogicalPoint
let region = LogicalRect {
    min: LogicalPoint { x: rect.min.x as f64, y: rect.min.y as f64 },
    max: LogicalPoint { x: rect.max.x as f64, y: rect.max.y as f64 },
};
```

但需要注意：**多屏环境下 eframe 全屏窗口只会覆盖当前屏幕**。通过 `OutputInfo.logical_geometry` 获取屏幕偏移量，将 eframe 坐标转换为全局逻辑坐标：

```rust
let global_region = LogicalRect {
    min: LogicalPoint {
        x: rect.min.x as f64 + output.logical_geometry.min.x,
        y: rect.min.y as f64 + output.logical_geometry.min.y,
    },
    max: LogicalPoint {
        x: rect.max.x as f64 + output.logical_geometry.min.x,
        y: rect.max.y as f64 + output.logical_geometry.min.y,
    },
};
```

---

## 4. 主流程集成

`main.rs` 中只需在选择器调用处增加协议探测分支：

```rust
if cli.mode.area.as_ref().is_some_and(|s| s.is_empty()) {
    let probe = wlsnap::backend::probe_all();
    let region = if probe.has_layer_shell() {
        wlsnap::ui::layer_selector::LayerSelector::run()
    } else {
        wlsnap::ui::eframe_selector::EframeSelector::run()
    };
    
    match region {
        Some(r) => { /* 转换为坐标，headless 捕获 */ }
        None => { /* 取消 */ }
    }
}
```

---

## 5. 边界条件处理

| 场景 | 处理策略 |
|------|---------|
| 无可用输出 | `EframeSelector::run()` 返回 `None`，主流程报错退出 |
| 选区宽度/高度 < 10px | 视为取消，返回 `None` |
| 多屏环境 | 仅覆盖当前指针所在屏幕，坐标转换为全局逻辑坐标 |
| 用户按 Esc | 返回 `None` |
| eframe 初始化失败 | 返回 `None`，主流程报错退出 |

---

## 6. 测试策略

| 测试类型 | 内容 | 方式 |
|---------|------|------|
| 单元测试 | 坐标转换逻辑 | 模拟 `OutputInfo` 和 `egui::Rect` |
| 手动测试 | GNOME 下验证选区正确性 | 在 GNOME 虚拟机/会话中运行 `--area` |
| 手动测试 | 多屏环境下坐标正确性 | 连接多个显示器，验证选区位置 |

---

## 7. 风险与缓解

| 风险 | 缓解措施 |
|------|---------|
| eframe 全屏窗口在某些 compositor 上无法真正覆盖所有内容 | 文档说明此限制；这是已知约束，与 eframe 方案一致 |
| 多屏混合 DPI 下坐标偏移 | 使用 `OutputInfo.logical_geometry` 进行全局坐标转换 |
| eframe 启动慢于 layer-shell | 可接受；仅在无 layer-shell 时使用 |

---

## 8. 关键决策记录 (ADR)

| 决策 | 选择 | 理由 |
|------|------|------|
| 回退方案架构 | 独立 `EframeSelector::run()` | 与 `LayerSelector::run()` 接口完全一致，主流程零改动 |
| 选择完成后行为 | 立即捕获并退出 | 与 layer-shell 路径保持一致 |
| 覆盖范围 | 仅当前指针所在屏幕 | 与 layer-shell 当前实现一致，简化多屏处理 |
| 渲染方式 | egui `Painter` + `Shape` | 利用现有 egui 能力，无需引入新依赖 |
