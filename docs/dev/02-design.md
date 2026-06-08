# wlsnap 详细设计方案

> 基于 `01-tech-spec.md` 技术选型方案，覆盖 Phase 1 ~ Phase 4 的完整架构设计。  
> 版本: 0.2.0 | 日期: 2026-05-06

---

## 1. 设计目标与约束

### 1.1 目标
- 构建一个仅支持 Wayland 的 Linux 截图工具，覆盖 KDE、GNOME、Hyprland、Sway、Niri、COSMIC 等主流桌面环境。
- 提供基础截图（当前屏幕全屏、区域、窗口、所有屏幕拼接）、标注编辑、Pin 贴图及长截图（Auto/Manual）功能。
- 截图后行为高度可配置：内置编辑、直接保存、复制剪贴板、管道输出、调用外部程序。
- 零全局热键依赖，纯 CLI 驱动，由用户 compositor 配置绑定。

### 1.2 核心约束
- **GNOME 限制**: Mutter 不实现 `wlr-layer-shell`，Pin 贴图退化为 `always_on_top` 无边框小窗口（体验降级但可用）；窗口截图不可行。
- **GNOME/KDE Auto 长截图**: 缺少输入注入协议，仅支持 Manual 模式。
- **长截图通用限制**: 仅垂直滚动、静态内容、帧间必须有 overlap。
- **单实例**: 为避免多个选区窗口冲突，应用采用单实例模式（Unix domain socket）。
- **多屏简化**: 默认所有交互（选区、编辑入口）仅在**当前指针所在屏幕**上进行；全屏拼接所有屏幕作为独立入口提供。

---

## 2. 架构总览：纯 eframe + 独立 sctk 后端

### 2.1 为什么选择纯 eframe 方案

经过对现有 egui/Wayland 生态的调研：
- `eframe` 原生支持 Wayland（通过 winit），但不支持 `layer-shell`。
- 将 egui 嵌入 `layer-shell` surface 需要绕过 eframe/winit 的窗口管理，手动创建 `wl_surface` 并绑定渲染上下文，实现复杂度高且 compositor 兼容性差。
- `egui_overlay` 等项目主要依赖 X11 或 GLFW，Wayland 支持有限。

**最终方案：所有 UI 窗口均通过 `eframe` 创建，Wayland 协议交互（截图、输出枚举）在独立线程通过 `sctk` 完成。**

| 功能 | 实现方式 | 说明 |
|------|---------|------|
| 标注编辑器 | `eframe` 普通窗口 | 标准 egui 应用模式 |
| 区域选择 | `eframe` **全屏无边框**窗口 | `with_decorations(false)` + `set_fullscreen(true)`，在当前指针所在屏幕上绘制半透明遮罩与选框 |
| Pin 贴图 | `eframe` **无边框小窗口** | `with_decorations(false)` + `with_always_on_top(true)` + 透明背景 |
| 长截图预览/对话框 | `eframe` 普通窗口 | 模态对话框或独立小窗 |
| 截图后端 | `tokio` 线程 + `sctk` | 异步执行 screencopy/portal，结果通过 channel 回传 UI |

### 2.2 异步事件循环集成

```
┌─────────────────────────────────────────────────────────────┐
│                        UI Thread (eframe)                    │
│  ┌─────────────┐    ┌──────────────┐    ┌─────────────────┐ │
│  │  egui update │◄───│  mpsc::Receiver│◄──│  tokio runtime  │ │
│  │   (~60fps)   │    │   (backend    │    │  (sctk/ashpd)   │ │
│  └─────────────┘    │    events)    │    └─────────────────┘ │
│         │           └──────────────┘             ▲           │
│         │ request_repaint()                      │           │
│         ▼                                        │           │
│  ┌─────────────┐                                 │           │
│  │  AppState   │────► 发起截图请求 ───────────────┘           │
│  │   machine   │          (tokio::spawn)                      │
│  └─────────────┘                                            │
└─────────────────────────────────────────────────────────────┘
```

- UI 线程运行 eframe 事件循环，所有状态更新和渲染在此线程。
- 后台线程运行 `tokio` runtime，执行 Wayland 协议往返（screencopy、portal D-Bus 调用）。
- 使用 `tokio::sync::mpsc::unbounded_channel::<BackendEvent>()` 将后端事件（捕获完成、错误、进度）传回 UI 线程。
- UI 线程在 `update()` 中 `try_recv()` 消息，处理完后调用 `ctx.request_repaint()` 触发重绘。

---

## 3. 项目目录结构

```
wlsnap/
├── Cargo.toml
├── config/
│   └── config.example.toml      # 配置示例参考文件
├── docs/
│   ├── README.md                # 文档导航入口
│   ├── dev/                     # 开发者文档
│   │   ├── 01-tech-spec.md      # 技术选型方案
│   │   ├── 02-design.md         # 详细架构设计
│   │   └── 03-roadmap.md        # 开发路线图与排期
│   ├── user/                    # 用户文档（待补充）
│   └── project/                 # 项目元信息（待补充）
└── src/
    ├── main.rs                  # 应用入口: CLI 解析 → 单实例检查 → eframe::run_native
    ├── cli.rs                   # clap v4 命令行定义
    ├── app.rs                   # 全局 App 结构体（状态机 + 常驻资源）
    ├── config.rs                # 配置解析、默认值、占位符展开
    ├── error.rs                 # 统一错误类型 (thiserror)
    ├── single_instance.rs       # 单实例守护（Unix domain socket）
    ├── constants.rs             # 常量: 版本号、协议名、默认路径
    │
    ├── backend/                 # 截图后端抽象层（运行在 tokio 线程）
    │   ├── mod.rs               # CaptureBackend trait、自动探测、三层降级
    │   ├── protocol.rs          # Wayland globals 探测（含可用性缓存）
    │   ├── capabilities.rs      # CaptureCapabilities 标志位
    │   ├── wlr.rs               # wlr-screencopy-unstable-v1 实现
    │   ├── portal.rs            # xdg-desktop-portal + ashpd 实现
    │   ├── ext_capture.rs       # ext-image-copy-capture-v1 实现
    │   ├── virtual_pointer.rs   # wlr-virtual-pointer 滚动注入
    │   ├── toplevel.rs          # wlr-foreign-toplevel / plasma-window-management
    │   └── kde_eis.rs           # KWin EIS (Emulated Input Server) 实验性接口（Phase 3）
    │
    ├── capture/                 # 截图流程编排
    │   ├── mod.rs               # 截图请求/响应类型、工作流编排器
    │   ├── region.rs            # 区域选择逻辑（eframe 全屏窗口内 egui 绘制）
    │   ├── output.rs            # 单屏/多屏捕获、DPI 处理、transform
    │   ├── window.rs            # 窗口截图（枚举 → 选择 → 捕获）
    │   └── scrolling/           # 长截图子模块
    │       ├── mod.rs           # 长截图公共接口、Auto/Manual 分发
    │       ├── auto.rs          # Auto 模式: virtual-pointer 滚动 + 定时捕获
    │       ├── manual.rs        # Manual 模式: 用户手动滚动 + 定时捕获
    │       ├── stitcher.rs      # Stitcher trait + Column Sampling 实现
    │       ├── orb.rs           # ORB 特征点 + RANSAC 拼接（Phase 3 可选）
    │       └── preview.rs       # 实时预览（已捕获帧数、预估高度）
    │
    ├── ui/                      # GUI 层 (eframe + egui)
    │   ├── mod.rs               # UI 事件枚举、屏幕/窗口尺寸工具
    │   ├── editor.rs            # 标注编辑器主面板（支持 zoom/pan）
    │   ├── selector.rs          # 区域选择（eframe 全屏窗口内的遮罩+选框）
    │   ├── pinner.rs            # Pin 贴图窗口（无边框置顶小窗）
    │   ├── scroll_dialog.rs     # 长截图模式选择对话框 (Auto/Manual)
    │   ├── widgets.rs           # 自定义 egui 组件（颜色选择器、画笔预设、字体选择）
    │   └── theme.rs             # 主题/样式常量
    │
    ├── image_engine/            # 图像处理引擎
    │   ├── mod.rs               # 图像类型别名、坐标转换、脏矩形管理
    │   ├── annotation.rs        # 标注绘制: 画笔、矩形、箭头、文字、马赛克/模糊
    │   ├── history.rs           # 撤销栈 (Command 模式)
    │   ├── pixmap.rs            # tiny-skia Pixmap ↔ image::RgbaImage 互操作
    │   ├── transform.rs         # 旋转/翻转/缩放（处理 output transform）
    │   ├── blur.rs              # 高斯模糊、像素化（马赛克）
    │   └── font.rs              # 系统字体枚举、加载、选择（fontdb + rustybuzz）
    │
    ├── output_manager/          # 输出与分发
    │   ├── mod.rs               # 分发路由（save / clipboard / pipe）
    │   ├── save.rs              # 文件保存、路径占位符展开、格式选择
    │   ├── clipboard.rs         # 剪贴板操作（arboard → wl-copy）
    │   └── pipe.rs              # stdout / 管道输出
    │
    └── platform/                # 平台抽象
        ├── mod.rs               # 平台能力探测
        ├── wayland.rs           # sctk 初始化、event queue 管理（tokio 线程内）
        └── output_info.rs       # wl_output 信息封装（name, geometry, scale, transform, position）
```

---

## 4. 核心类型定义

### 4.1 几何与坐标类型

```rust
// src/image_engine/mod.rs 或 src/platform/output_info.rs

/// 逻辑坐标（与屏幕 DPI 无关）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalPoint {
    pub x: f64,
    pub y: f64,
}

/// 逻辑矩形
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalRect {
    pub min: LogicalPoint,
    pub max: LogicalPoint,
}

impl LogicalRect {
    pub fn width(&self) -> f64 { self.max.x - self.min.x }
    pub fn height(&self) -> f64 { self.max.y - self.min.y }
    pub fn is_empty(&self) -> bool { self.width() <= 0.0 || self.height() <= 0.0 }
    pub fn contains(&self, point: LogicalPoint) -> bool { /* ... */ }
    pub fn to_physical(&self, scale: f64) -> PhysicalRect { /* ... */ }
}

/// 物理像素坐标（与 buffer 尺寸对应）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicalPoint { pub x: i32, pub y: i32 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicalRect { pub min: PhysicalPoint, pub max: PhysicalPoint }

/// 颜色（RGBA 8-bit per channel）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }

impl Color {
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };
    pub fn from_hex(hex: &str) -> Result<Self> { /* ... */ }
}
```

### 4.2 输出与屏幕信息

```rust
// src/platform/output_info.rs

#[derive(Debug, Clone)]
pub struct OutputInfo {
    pub name: String,                  // e.g. "DP-1"
    pub description: String,
    pub logical_geometry: LogicalRect, // 在全局坐标空间中的位置
    pub physical_size: (u32, u32),     // 物理像素尺寸
    pub scale_factor: f64,             // 逻辑缩放（如 1.0, 1.5, 2.0）
    pub transform: OutputTransform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTransform {
    Normal,
    Rotated90,
    Rotated180,
    Rotated270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

impl OutputTransform {
    /// 将物理尺寸转换为逻辑方向后的尺寸
    pub fn apply(&self, width: u32, height: u32) -> (u32, u32) { /* ... */ }
}
```

### 4.3 截图后端抽象

```rust
// src/backend/mod.rs

bitflags! {
    pub struct CaptureCapabilities: u32 {
        const FULLSCREEN    = 1 << 0;
        const REGION        = 1 << 1;
        const WINDOW        = 1 << 2;
        const OUTPUT        = 1 << 3;
        const SCROLL_AUTO   = 1 << 4;
        const SCROLL_MANUAL = 1 << 5;
        const CURSOR        = 1 << 6;
    }
}

#[async_trait]
pub trait CaptureBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> CaptureCapabilities;

    /// 捕获当前指针所在屏幕
    async fn capture_current_screen(&self, cursor: bool) -> Result<CapturedFrame>;

    /// 捕获所有屏幕并拼接为一张大图
    async fn capture_all_screens(&self, cursor: bool) -> Result<CapturedFrame>;

    /// 在指定输出上捕获指定区域
    async fn capture_region(&self, region: LogicalRect, output: &OutputInfo, cursor: bool) -> Result<CapturedFrame>;

    /// 窗口截图（若不支持返回 UnsupportedCapability）
    async fn capture_window(&self, window: &WindowInfo, cursor: bool) -> Result<CapturedFrame>;
}

pub struct CapturedFrame {
    pub image: RgbaImage,
    pub source_output: OutputInfo,
    pub physical_size: (u32, u32),
    pub logical_size: (u32, u32),
    pub scale_factor: f64,
}

/// 窗口信息（用于窗口截图）
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: String,
    pub app_id: String,
    pub title: String,
    pub geometry: LogicalRect,
    pub output_name: String,
}
```

### 4.4 截图请求与分发动作

```rust
// src/capture/mod.rs

/// 用户发起的截图请求
#[derive(Debug, Clone)]
pub enum CaptureRequest {
    FullCurrentScreen,            // --screen（当前屏幕）
    FullAllScreens,               // --all-screen（拼接所有屏幕）
    Region { output: OutputInfo }, // --range（在当前屏幕上选区）
    Window,                       // --window（在当前屏幕上选窗口）
    Output { name: String },      // --output NAME（指定输出）
}
```

### 4.5 后端事件（tokio → UI 线程通信）

```rust
// src/backend/mod.rs

/// 后台任务向 UI 线程发送的事件
#[derive(Debug)]
pub enum BackendEvent {
    /// 截图完成
    CaptureFinished {
        request_id: Uuid,
        result: Result<CapturedFrame>,
    },
    /// 窗口枚举完成
    WindowsEnumerated {
        request_id: Uuid,
        windows: Vec<WindowInfo>,
    },
    /// 长截图进度
    ScrollProgress {
        session_id: Uuid,
        frame_count: usize,
        current_height: u32,
        stitched_preview: Option<RgbaImage>, // 缩略图
    },
    /// 长截图完成
    ScrollFinished {
        session_id: Uuid,
        result: Result<CapturedFrame>,
    },
    /// 后端错误
    Error { request_id: Uuid, error: WlsnapError },
}
```

### 4.6 标注编辑器类型

```rust
// src/image_engine/annotation.rs

/// 标注工具种类
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationTool {
    Select,  // 默认：选择和移动
    Pen { width: f32, color: Color },
    Rect { stroke_width: f32, stroke_color: Color, fill_color: Option<Color> },
    Arrow { width: f32, color: Color },
    Text { font_size: f32, color: Color, font_family: Option<String> },
    Mosaic { block_size: u32 },
    Blur { radius: f32 },
}

/// 编辑器视口状态（支持 zoom/pan）
#[derive(Debug, Clone, Copy)]
pub struct EditorViewport {
    pub offset: LogicalPoint,  // 画布偏移（pan）
    pub zoom: f64,             // 缩放比例（1.0 = 原始大小）
    pub min_zoom: f64,         // 0.1
    pub max_zoom: f64,         // 10.0
}

impl Default for EditorViewport {
    fn default() -> Self {
        Self { offset: LogicalPoint { x: 0.0, y: 0.0 }, zoom: 1.0, min_zoom: 0.1, max_zoom: 10.0 }
    }
}
```

### 4.7 撤销栈（Command 模式）

```rust
// src/image_engine/history.rs

pub trait Command: Send + Sync {
    fn execute(&self, canvas: &mut Pixmap);
    fn undo(&self, canvas: &mut Pixmap);
    fn describe(&self) -> &'static str;
    /// 返回此命令影响的区域，用于脏矩形优化
    fn affected_region(&self) -> Option<PhysicalRect>;
}

pub struct HistoryStack {
    commands: Vec<Box<dyn Command>>,
    undone: Vec<Box<dyn Command>>,
    max_depth: usize,
}

impl HistoryStack {
    pub fn push(&mut self, cmd: Box<dyn Command>, canvas: &mut Pixmap) { /* ... */ }
    pub fn undo(&mut self, canvas: &mut Pixmap) -> Option<&dyn Command> { /* ... */ }
    pub fn redo(&mut self, canvas: &mut Pixmap) -> Option<&dyn Command> { /* ... */ }
    pub fn can_undo(&self) -> bool { !self.commands.is_empty() }
    pub fn can_redo(&self) -> bool { !self.undone.is_empty() }
}
```

### 4.8 Pin 贴图窗口

```rust
// src/ui/pinner.rs

pub struct PinWindow {
    pub id: Uuid,
    pub image: RgbaImage,
    pub display_output: String,    // 所在输出名称
    pub position: LogicalPoint,    // 在屏幕上的逻辑坐标
    pub scale: f64,
    pub opacity: f64,
}

pub enum PinAction {
    None,
    Move(LogicalPoint),
    Scale(f64),
    SetOpacity(f64),
    Close,
    Save(PathBuf),
    CopyClipboard,
    OpenEditor,
}
```

---

## 5. 状态机与事件循环

### 5.1 全局 App 结构

```rust
// src/app.rs

pub struct WlsnapApp {
    /// 主流程状态机（互斥状态）
    pub state: AppState,

    /// 常驻资源：Pin 窗口列表（可与任何主状态共存）
    pub pin_windows: Vec<PinWindow>,

    /// 共享配置（Arc 避免多处克隆）
    pub config: Arc<Config>,

    /// 后端事件接收器
    pub backend_rx: UnboundedReceiver<BackendEvent>,

    /// 后端任务发送器（克隆给后台 tokio 任务）
    pub backend_tx: UnboundedSender<BackendEvent>,

    /// 当前指针所在输出（由 platform 模块定期更新）
    pub current_output: Option<OutputInfo>,

    /// 所有已连接的输出
    pub all_outputs: Vec<OutputInfo>,

    /// 已发起的后台请求（用于匹配响应）
    pub pending_requests: HashMap<Uuid, PendingRequest>,
}

/// 主流程状态（互斥）
pub enum AppState {
    Idle,

    /// 区域选择中（eframe 全屏无边框窗口）
    SelectingRegion {
        output: OutputInfo,
        selection: Option<LogicalRect>,
        start_pos: Option<LogicalPoint>,
    },

    /// 窗口选择中（在当前屏幕上列出可选窗口）
    SelectingWindow {
        output: OutputInfo,
        windows: Vec<WindowInfo>,
        selected: Option<WindowInfo>,
    },

    /// 后端正在捕获
    Capturing {
        request_id: Uuid,
        request: CaptureRequest,
    },

    /// 标注编辑器打开
    Editing {
        request_id: Uuid,
        image: RgbaImage,
        pixmap: Pixmap,
        texture: Option<egui::TextureHandle>,  // 缓存，增量更新
        history: HistoryStack,
        active_tool: AnnotationTool,
        viewport: EditorViewport,
        dirty_rect: Option<PhysicalRect>,      // 脏矩形，用于增量 texture 更新
    },

    /// 长截图进行中
    Scrolling {
        session_id: Uuid,
        mode: ScrollMode,
        frame_count: usize,
        current_height: u32,
        preview: Option<RgbaImage>,  // 缩略图预览
    },

    /// 截图后选择动作（当配置为 Ask 时）
    ChoosingAction {
        image: RgbaImage,
    },
}

/// 等待中的请求元数据
pub struct PendingRequest {
    pub request: CaptureRequest,
    pub started_at: Instant,
}
```

### 5.2 状态转换规则

```
Idle ──► SelectingRegion ──► Capturing ──► Editing ──► ChoosingAction ──► (Save/Clipboard/Pipe)
  │           │                  │            │            │
  │           │ (Esc 取消)       │ (Esc 取消) │ (Esc 取消) │
  │           ▼                  ▼            ▼            ▼
  │────────────────────────────────────────────────────────────────────► Idle
  │
  ├──► SelectingWindow ──► Capturing ──► ... (同上)
  │
  ├──► Capturing (直接 --full / --full-all / --screen) ──► ...
  │
  ├──► Scrolling ──► Editing / Save / Clipboard / Pipe ──► Idle
  │
  └──► Pinning (打开已有 Pin 窗口，不影响主状态)

> **v0.1.0 状态机简化**: v0.1.0 不涉及 `SelectingRegion`、`SelectingWindow`、`Editing`、
> `Scrolling`、`ChoosingAction` 状态。截图完成后直接执行输出动作（Save/Clipboard/Pipe），
> 不进入编辑器。`Capturing` 状态用于显示进度/等待提示，完成后立即回到 `Idle`。
```

### 5.3 eframe 事件循环模板

```rust
// src/app.rs

impl eframe::App for WlsnapApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // 1. 处理后端事件
        while let Ok(event) = self.backend_rx.try_recv() {
            self.handle_backend_event(ctx, event);
        }

        // 2. 根据当前状态渲染不同 UI
        match &mut self.state {
            AppState::Idle => {
                // 空闲状态：若从 CLI 收到命令，在 Idle 处理时立即发起
                // 正常情况下 Idle 不显示主窗口，或显示一个小的托盘/指示器
                frame.close();
            }
            AppState::SelectingRegion { .. } => {
                self.render_selector(ctx, frame);
            }
            AppState::SelectingWindow { .. } => {
                self.render_window_selector(ctx, frame);
            }
            AppState::Editing { .. } => {
                self.render_editor(ctx, frame);
            }
            AppState::Capturing { .. } => {
                self.render_capturing_overlay(ctx, frame);
            }
            AppState::Scrolling { .. } => {
                self.render_scroll_dialog(ctx, frame);
            }
            AppState::ChoosingAction { .. } => {
                self.render_action_chooser(ctx, frame);
            }
        }

        // 3. 渲染所有 Pin 窗口（通过 eframe Viewport API）
        for pin in &self.pin_windows {
            self.render_pin_viewport(ctx, pin);
        }
    }
}
```

**关于 Pin 窗口的多窗口实现**: eframe 支持通过 `ctx.show_viewport_immediate()` 或 `ViewportBuilder` 创建多个独立窗口。每个 Pin 窗口对应一个 viewport：

```rust
fn render_pin_viewport(&self, ctx: &egui::Context, pin: &PinWindow) {
    let viewport_id = egui::ViewportId::from(pin.id);
    ctx.show_viewport_immediate(viewport_id, ViewportBuilder::default()
        .with_title("wlsnap pin")
        .with_decorations(false)
        .with_always_on_top(true)
        .with_transparent(true)
        .with_inner_size([width, height]),
        |ctx, class| {
            // 绘制贴图图像 + 右键菜单
        }
    );
}
```


---

## 6. 配置系统

### 6.1 配置结构

配置使用 **TOML**，路径 `~/.config/wlsnap/config.toml`。首次启动若不存在则自动生成。

```rust
// src/config.rs

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub pin: PinConfig,
    #[serde(default)]
    pub scrolling: ScrollingConfig,
    #[serde(default)]
    pub shortcuts: ShortcutsConfig,
    #[serde(default)]
    pub advanced: AdvancedConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeneralConfig {
    pub save_dir: String,
    pub filename_template: String,
    pub format: ImageFormat,        // Png / Jpeg / WebP
    pub jpeg_quality: u8,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EditorConfig {
    pub stroke_color: String,
    pub stroke_width: f32,
    pub undo_depth: usize,
    pub mosaic_size: u32,
    pub font_family: Option<String>,
    pub font_size: f32,             // default: 16.0
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PinConfig {
    pub default_scale: f64,
    pub opacity: f64,
    pub show_context_menu: bool,
    pub enable_drag: bool,
    pub enable_scroll_zoom: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScrollingConfig {
    pub auto_scroll_interval_ms: u64,
    pub manual_capture_interval_ms: u64,
    pub stitch_algorithm: StitchAlgorithm, // ColumnSampling / Orb
    pub idle_stop_threshold: usize,
    pub preview_enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShortcutsConfig {
    pub save: String,
    pub undo: String,
    pub redo: String,       // 新增: Ctrl+Shift+Z / Ctrl+Y
    pub copy: String,
    pub cancel: String,
    pub confirm: String,
    pub switch_mode: String,
    pub zoom_in: String,    // 新增: Ctrl++ / Ctrl+滚轮
    pub zoom_out: String,   // 新增: Ctrl+- / Ctrl+滚轮
    pub reset_zoom: String, // 新增: Ctrl+0
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdvancedConfig {
    pub debug: bool,
    pub log_level: String,
    pub include_cursor: bool,
    pub persist_portal_token: bool,
    pub portal_restore_token_path: Option<String>,
}
```

### 6.2 CLI 设计（clap v4）

```rust
// src/cli.rs

#[derive(Parser)]
#[command(name = "wlsnap")]
#[command(version, about = "Wayland screenshot utility")]
pub struct Cli {
    #[command(flatten)]
    pub mode: CaptureMode,

    #[arg(long)]
    pub stdout: bool,

    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    #[arg(short, long)]
    pub clipboard: bool,

    #[arg(long)]
    pub cursor: bool,

    #[arg(long)]
    pub list_outputs: bool,

    #[arg(long)]
    pub debug_protocol: bool,
}

#[derive(Args, Clone)]
#[group(required = false, multiple = false)]
pub struct CaptureMode {
    #[arg(long, visible_alias = "full")]
    pub screen: bool,          // 当前屏幕全屏

    #[arg(short, long, visible_alias = "full-all")]
    pub all_screen: bool,      // 拼接所有屏幕

    #[arg(short, long, value_name = "X,Y,W,H", num_args = 0..=1, default_missing_value = "")]
    pub range: Option<String>, // 区域选择（交互式或坐标）

    #[arg(long)]
    pub window: bool,          // 窗口截图

    #[arg(long, value_name = "PATH")]
    pub pin: Option<Option<PathBuf>>,

    #[arg(long)]
    pub scroll_auto: bool,

    #[arg(long)]
    pub scroll_manual: bool,
}
```

### 6.3 分发路由优先级

CLI 显式参数 > 默认值：

1. `--stdout` → stdout（管道传输）
2. `--clipboard` / `-c` → 剪贴板
3. `--output PATH` / `-o` → 保存到指定路径
4. 无参数 → 保存到默认目录

---

## 7. 截图工作流

### 7.1 多屏简化策略

**默认行为：仅操作当前指针所在屏幕。**

| 命令 | 行为 |
|------|------|
| `--screen` / `-s` | 捕获当前屏幕 |
| `--all-screen` / `-a` | 捕获所有输出并拼接为一张大图 |
| `--range` / `-r` | 在当前屏幕上进入区域选择 |
| `--window` | 在当前屏幕上枚举窗口并选择 |

**当前屏幕判定**：通过 `wl_pointer` 的 `wl_surface::enter` 事件或查询 pointer 所在 `wl_output` 确定。平台初始化时缓存所有 output 的几何信息，根据指针全局坐标匹配。

### 7.2 区域选择（layer-shell overlay 方案）

交互式 `--range` 使用原生 `zwlr_layer_shell_v1` Overlay 层实现真正的全屏覆盖，体验最佳：

1. 通过 sctk 创建 `LayerSurface`，设置 `Layer::Overlay` + `Anchor::ALL` + `KeyboardInteractivity::Exclusive`。
2. 使用 `wl_shm` + `SlotPool` 进行软件渲染：
   - 全屏填充半透明黑色遮罩（ARGB `#80000000`）
   - 拖拽时绘制高亮矩形（ARGB `#40000000`）+ 白色边框
   - 实时显示选区尺寸标签
3. 监听输入事件：
   - `wl_pointer::Press { button: 272 }` → 记录选区起点
   - `wl_pointer::Motion` → 实时更新选区，请求 `wl_surface::frame` 重绘
   - `wl_pointer::Release { button: 272 }` → 确认选区，退出事件循环
   - `wl_keyboard::Escape` → 取消选择
4. 返回 `LogicalRect` 后，主流程将其转换为 `--range x,y,w,h` 并走 headless 捕获路径。

**GNOME 降级**：GNOME 不实现 `wlr-layer-shell`，交互式 `--range` 退化为 eframe 全屏无边框窗口方案（无边框 + `set_fullscreen(true)` + 半透明遮罩）。

```rust
// src/ui/layer_selector.rs 核心逻辑

impl LayerSelector {
    pub fn run() -> Option<LogicalRect> {
        // 1. 连接 Wayland，绑定 layer_shell
        // 2. 创建 Overlay 全屏 surface
        // 3. 运行 blocking_dispatch 事件循环
        // 4. 返回 selected_region 或 None（取消）
    }
}
```

### 7.3 全屏拼接（`--full-all`）

1. 后端通过 sctk 枚举所有 `wl_output`，获取每个 output 的 `logical_geometry`、`scale_factor`、`transform`。
2. 逐个输出调用后端捕获，得到每张物理 buffer 图像。
3. 根据 `transform` 旋转/翻转每张图像到逻辑方向。
4. 计算所有 `logical_geometry` 的并集作为全局 canvas 尺寸。
5. 将各图像按 `logical_geometry.position` 拼入 canvas。
6. 在**当前指针所在屏幕**上打开编辑器或执行后续动作。

---

## 8. 标注编辑器（支持 Zoom / Pan）

### 8.1 画布架构

```
┌─────────────────────────────────────────────┐
│  EditorViewport (zoom + offset)             │
│  ┌───────────────────────────────────────┐  │
│  │  Base Layer: RgbaImage (原始截图)      │  │
│  │  ───────────────────────────────────── │  │
│  │  Annotation Layer: tiny_skia::Pixmap   │  │
│  │  （所有标注绘制在此层，Command 模式管理）│  │
│  └───────────────────────────────────────┘  │
│  ─► 合并为 egui::TextureHandle（增量更新）   │
└─────────────────────────────────────────────┘
```

### 8.2 Zoom / Pan 交互

| 操作 | 行为 |
|------|------|
| 滚轮 | 以鼠标指针为中心缩放画布（`zoom *= 1.1` / `zoom /= 1.1`） |
| 中键拖拽 | Pan 移动画布（修改 `viewport.offset`） |
| 空格 + 左键拖拽 | 同中键拖拽（备选方案，兼容绘图软件习惯） |
| `Ctrl+0` | 重置缩放为 1.0，居中画布 |
| `Ctrl++` / `Ctrl+-` | 逐步缩放 |

**坐标转换**：
```rust
/// 屏幕坐标 → 画布坐标（考虑 zoom/pan）
fn screen_to_canvas(screen: LogicalPoint, viewport: &EditorViewport) -> LogicalPoint {
    LogicalPoint {
        x: (screen.x - viewport.offset.x) / viewport.zoom,
        y: (screen.y - viewport.offset.y) / viewport.zoom,
    }
}

/// 画布坐标 → 屏幕坐标
fn canvas_to_screen(canvas: LogicalPoint, viewport: &EditorViewport) -> LogicalPoint {
    LogicalPoint {
        x: canvas.x * viewport.zoom + viewport.offset.x,
        y: canvas.y * viewport.zoom + viewport.offset.y,
    }
}
```

### 8.3 增量 Texture 更新

为避免每次撤销都全量重新上传 GPU texture：
1. 维护 `Editing.dirty_rect: Option<PhysicalRect>`。
2. 每次 `Command::execute/undo` 后，更新 `dirty_rect` 为命令影响的区域与现有脏矩形的并集。
3. 在 `update()` 末尾，若 `dirty_rect` 存在，仅将该区域从 `Pixmap` 上传到 `TextureHandle`（`ctx.tex_manager().set_partial()` 或重新分配 sub-region）。
4. 若脏矩形过大（超过 50% 图像面积），回退到全量更新。

---

## 9. Pin 贴图（eframe 无边框置顶窗口）

### 9.1 窗口创建

使用 eframe `ViewportBuilder` 创建独立无边框窗口：

```rust
let viewport_id = ViewportId::from(pin_id);
ctx.show_viewport_immediate(viewport_id, ViewportBuilder::default()
    .with_title("wlsnap-pin")
    .with_decorations(false)
    .with_always_on_top(true)
    .with_transparent(true)
    .with_inner_size([img_width * scale, img_height * scale])
    .with_app_id("wlsnap-pin".to_owned()),
    |ctx, class| {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                // 显示图像 + 处理交互
            });
    });
```

### 9.2 交互

| 操作 | 行为 |
|------|------|
| 左键拖拽 | 移动窗口位置（通过 `ctx.send_viewport_cmd(ViewportCommand::OuterPosition)`） |
| 滚轮 | 缩放图像（调整窗口尺寸 + 重绘） |
| 右键 | 弹出 egui context menu：关闭 / 保存 / 复制 / 打开编辑器 |
| `Esc` | 关闭当前 Pin 窗口 |

**GNOME 降级**：GNOME 下 `always_on_top` 可能不受尊重，但无边框小窗口仍然可以工作。文档中说明体验可能不如 wlroots 系。

---

## 10. 长截图

### 10.1 流程

1. 用户触发 `--scroll-auto` 或 `--scroll-manual`。
2. 先进入区域选择（同普通区域选择，在当前屏幕上框选滚动区域）。
3. 探测后端能力：
   - 若支持 `virtual_pointer` → 进入 `Auto` 模式
   - 否则 → 进入 `Manual` 模式（GNOME/KDE）
4. 启动后台 `tokio::task`：
   - **Auto**: 循环 { 发送滚动事件 → 等待 → 捕获 → 拼接 }，直到检测到底
   - **Manual**: 循环 { 等待 interval → 捕获 → 拼接 }，由用户手动滚动
5. 每完成一帧拼接，通过 `BackendEvent::ScrollProgress` 回传进度到 UI。
6. 用户按 `Esc` 或检测到底 → `BackendEvent::ScrollFinished` → 进入后续动作。

### 10.2 拼接算法

**Column Sampling（默认）**：
```rust
pub struct ColumnSamplingStitcher {
    accumulated: RgbaImage,
    columns: usize,           // 3
    overlap_search_px: u32,   // 最大搜索重叠高度
    threshold: f64,           // MAD 阈值
}

impl Stitcher for ColumnSamplingStitcher {
    fn push_frame(&mut self, frame: &RgbaImage) -> Result<StitchResult> {
        // 1. 将新帧转为灰度
        // 2. 在 accumulated 图像底部 overlap_search_px 范围内，
        //    与新帧顶部 overlap_search_px 范围做滑动窗口比对
        // 3. 取 3 列（左/中/右各 20% 宽度），计算每列的 MAD
        // 4. 平均 MAD 最小且低于 threshold 的偏移量即为拼接位置
        // 5. 若找不到有效重叠 → NoOverlap
    }
}
```

**实时预览优化**：不向 UI 发送完整拼接图，而是发送 `image::imageops::resize` 后的缩略图（宽度 400px，保持比例），降低 channel 传输和 UI 渲染开销。

---

## 11. 字体系统

### 11.1 系统字体枚举

使用 `fontdb` 扫描系统字体：

```rust
// src/image_engine/font.rs

pub struct FontDatabase {
    db: fontdb::Database,
}

impl FontDatabase {
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Self { db }
    }

    /// 枚举所有可用字体（按家族名去重）
    pub fn list_families(&self) -> Vec<String> { /* ... */ }

    /// 根据家族名获取字体数据（用于 rustybuzz shaping）
    pub fn query_font(&self, family: &str, weight: fontdb::Weight) -> Option<fontdb::ID> { /* ... */ }
}
```

### 11.2 文字标注渲染流程

1. 用户选择 Text 工具 → 在画布上点击 → 弹出 `egui::TextEdit` 输入文字。
2. 确认后，使用 `rustybuzz` 对文字进行 shaping（支持中文、日文等复杂脚本）。
3. 使用 `tiny-skia` 将字形光栅化并绘制到 `Pixmap`。
4. 若用户选择的字体不存在，回退到系统默认无衬线字体（fontdb 的 `sans-serif` generic family）。

---

## 12. 输出与分发

### 12.1 保存模块

```rust
// src/output_manager/save.rs

pub fn save_image(image: &RgbaImage, config: &GeneralConfig, mode: &str) -> Result<PathBuf> {
    let dir = expand_placeholders(&config.save_dir)?;
    std::fs::create_dir_all(&dir)?;

    let filename = expand_placeholders(&config.filename_template)?
        .replace("{mode}", mode);

    let path = dir.join(format!("{}.{}", filename, config.format.extension()));

    match config.format {
        ImageFormat::Png => image.save_with_format(&path, image::ImageFormat::Png)?,
        ImageFormat::Jpeg => {
            let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
            let mut buf = Vec::new();
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, config.jpeg_quality);
            encoder.encode_image(&rgb)?;
            std::fs::write(&path, buf)?;
        }
        ImageFormat::WebP => { /* webp 编码 */ }
    }

    Ok(path)
}
```

### 12.2 管道与外部程序

| 触发方式 | 行为 |
|----------|------|
| `--stdout` | PNG 编码后写入 `std::io::stdout()`，通过管道传输给后续程序处理 |
| `--clipboard` | 调用 `arboard` → 底层使用 `wl-copy` |

---

## 13. 错误处理与边界条件

### 13.1 统一错误类型

```rust
// src/error.rs

#[derive(thiserror::Error, Debug)]
pub enum WlsnapError {
    #[error("Wayland connection failed: {0}")]
    WaylandConnect(String),

    #[error("No suitable capture backend available")]
    NoBackendAvailable,

    #[error("Backend '{0}' does not support capability: {1:?}")]
    UnsupportedCapability(&'static str, CaptureCapabilities),

    #[error("Portal request denied or failed: {0}")]
    PortalDenied(#[from] ashpd::Error),

    #[error("Image processing error: {0}")]
    Image(#[from] image::ImageError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Clipboard error: {0}")]
    Clipboard(String),

    #[error("Stitching failed: {0}")]
    Stitching(&'static str),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Another instance is already running")]
    AlreadyRunning,

    #[error("Wayland disconnected")]
    WaylandDisconnected,

    #[error("Save failed: disk full or permission denied at {0}")]
    SaveFailed(PathBuf),

    #[error("No output detected")]
    NoOutputDetected,

    #[error("Font not found: {0}")]
    FontNotFound(String),
}

pub type Result<T> = std::result::Result<T, WlsnapError>;
```

### 13.2 边界条件处理矩阵

| 场景 | 处理策略 |
|------|---------|
| Wayland 连接断开 | 后端线程发送 `BackendEvent::Error`，UI 线程弹出错误对话框，优雅退出 |
| 截图时输出热插拔断开 | 跳过断开输出，继续捕获其余；若为主目标输出则报错 |
| 区域选择时按 Esc | 关闭全屏窗口，回到 `Idle`，不触发捕获 |
| 区域选择宽度/高度为 0 | 忽略，视为取消 |
| 保存时磁盘满 | 捕获 `std::io::ErrorKind::StorageFull`，弹窗提示用户选择其他路径 |
| 剪贴板被占用 | `arboard` 返回错误后重试 3 次（间隔 100ms），仍失败则弹窗提示 |
| Portal 调用超时 | tokio timeout 10s，取消后尝试下一个后端（如从 Portal 降级到 wlr-screencopy） |
| 长截图时目标窗口关闭 | 捕获失败 → `BackendEvent::Error` → 提示用户，返回已拼接部分 |
| 管道输出失败 | `std::io::ErrorKind::BrokenPipe` → 报错退出 |
| 配置文件中存在未知字段 | `serde` 默认忽略，但启动时 `tracing::warn!` 记录 |
| 图像尺寸过大（> 16384px 高度） | 长截图时检测到则警告用户并停止，避免内存溢出 |
| 多屏拼接时单屏捕获失败 | 记录警告，跳过该屏，继续拼接其余 |
| 无可用输出（headless） | 启动时检测，报错退出并提示 `--debug-protocol` 诊断 |

---

## 14. 测试策略

**原则**：纯逻辑模块使用模拟数据做单元测试；需要实际 Wayland 连接的模块不做自动化测试，依赖手动验证。

### 14.1 单元测试（有测试）

| 模块 | 测试内容 | 模拟方式 |
|------|---------|---------|
| `image_engine/stitcher.rs` | Column Sampling 拼接正确性 | 合成已知位移的渐变/随机图像序列 |
| `image_engine/history.rs` | 撤销/重做一致性 | Mock Command 对象，验证 execute/undo 对称性 |
| `config.rs` | 路径占位符展开 | 注入 `HOME=/tmp/test_home` 等环境变量 |
| `output_manager/save.rs` | 文件名生成与格式选择 | 临时目录 + 内存中的 RgbaImage |
| `image_engine/transform.rs` | OutputTransform 旋转矩阵 | 固定尺寸的测试图像，验证旋转后像素位置 |
| `image_engine/font.rs` | 字体查询回退 | 空 fontdb（无系统字体），验证回退到默认家族 |
| `cli.rs` | 参数解析 | clap 内置的 `try_parse_from` |

### 14.2 集成/手动测试（无自动化测试）

| 模块 | 验证方式 |
|------|---------|
| `backend/wlr.rs` | 在 Hyprland/Sway 上手动运行，验证截图内容正确 |
| `backend/portal.rs` | 在 GNOME/KDE 上手动运行，验证 Portal 弹窗与授权 |
| `ui/selector.rs` | 手动验证多屏环境下选区坐标正确 |
| `ui/pinner.rs` | 手动验证拖动、缩放、右键菜单 |
| `ui/editor.rs` | 手动验证 IME 输入、zoom/pan、各标注工具 |

---

## 15. 安全与权限

| 项 | 措施 |
|----|------|
| 单实例锁文件 | 使用 `~/.cache/wlsnap/instance.sock`（Unix domain socket），而非 `/tmp/wlsnap.lock`，避免其他用户干扰 |
| 截图文件权限 | 保存时设置 `chmod 600`，防止其他用户读取 |
| 配置文件权限 | 创建 `~/.config/wlsnap/` 时设置目录权限 `0o700` |
| 管道安全 | 直接写入 stdout，无 shell 注入风险 |
| Portal restore token | 存储在 `~/.cache/wlsnap/portal_token.json`，目录权限 `0o700` |
| 日志脱敏 | 避免在日志中记录窗口 title（可能包含敏感信息），仅记录 `app_id` 和哈希化 `id` |

---

## 16. 单实例机制

使用 **Unix domain socket** 实现单实例 + 命令转发：

1. 启动时尝试绑定 `~/.cache/wlsnap/instance.sock`。
2. 若绑定成功 → 当前实例为唯一实例，继续启动。
3. 若绑定失败（文件已存在）：
   - 尝试向该 socket 发送当前 CLI 参数（JSON 序列化）。
   - 若发送成功 → 已有实例正在运行，当前进程解析参数后发送并退出。
   - 若发送失败（socket 无主）→ 删除残留 socket，重新绑定并启动。
4. 主实例监听 socket，收到新命令后解析并触发对应状态转换（如从 `Idle` → `SelectingRegion`）。

---

## 17. 各 Phase 里程碑与交付物

### v0.1.0 ── CLI 截图闭环（M1）

| 模块 | 交付内容 | 说明 |
|------|----------|------|
| backend | `wlr-screencopy` 后端；协议探测框架 | 仅支持 wlroots 系 compositor |
| capture | 单屏/全屏捕获；多屏拼接（`--all-screen`）；区域截图（`--range` 坐标 / `--range` 交互选区） | `--range` 支持直接坐标（headless）和交互选区（layer-shell overlay）两种模式 |
| ui | eframe 最小窗口（仅用于事件循环，无实际 GUI 交互） | v0.1.0 不显示编辑器、选区、Pin 窗口 |
| output_manager | 保存（PNG/JPEG/WebP）；剪贴板（arboard）；stdout 输出 | 完整输出分发 |
| config | TOML 解析；默认值；路径占位符展开 | |
| cli | `--screen`, `--all-screen`, `--range`, `--window`, `--stdout`, `-o`, `--clipboard`, `--cursor` | |

### v0.2.0 ── 选区+编辑（M2）

| 模块 | 交付内容 |
|------|----------|
| ui | eframe 全屏选区遮罩；egui 编辑器（zoom/pan/pen/rect/arrow/text/mosaic/blur） |
| image_engine | tiny-skia 绘制；fontdb 系统字体枚举；脏矩形增量更新 |
| capture | eframe 全屏选区（当前屏幕）；窗口截图枚举 |
| single_instance | Unix domain socket 单实例 + 命令转发 |
| cli | 新增 `--window`, `--pin`（此时 `--pin` 打开编辑器中的贴图预览） |

### v0.3.0 ── 高级功能（M3）

| 模块 | 交付内容 |
|------|----------|
| pinner | eframe 无边框置顶贴图窗口 |
| scrolling | Auto 长截图（virtual-pointer + Column Sampling）；Manual 长截图；实时预览 |
| cli | 新增 `--scroll-auto`, `--scroll-manual` |

### Phase 2: GNOME / KDE 兼容（v0.4.0, M4）

### Phase 2: GNOME / KDE 兼容

| 模块 | 交付内容 |
|------|----------|
| backend | `ashpd` Portal 后端；`ext-image-copy-capture-v1` 自动探测 |
| ui | 协议探测驱动的 UI 降级（GNOME/KDE 隐藏 Auto；Pin 降级说明） |
| scrolling | Manual 长截图（定时 Portal 捕获 + 用户手动滚动） |
| config | `persist_portal_token` 实现 |

### Phase 3: 进阶优化

| 模块 | 交付内容 |
|------|----------|
| backend | `ext-image-copy-capture-v1` 作为 P0 自动启用；KDE EIS (Emulated Input Server) 实验 |
| scrolling | ORB 特征点 + RANSAC 拼接算法（配置项） |
| config | 手动滚动模式自适应捕获间隔 |

### Phase 4: Polish

| 模块 | 交付内容 |
|------|----------|
| cli | `--list-outputs`, `--debug-protocol`, `--config` |
| all | tracing 日志完善；性能优化；单元测试覆盖；文档与打包 |

---

## 18. 关键决策记录 (ADR)

| 决策 | 选择 | 理由 |
|------|------|------|
| UI 窗口方案 | eframe + 独立 sctk layer-shell | 区域选择使用原生 layer-shell overlay（体验最佳），编辑器/Pin 仍用 eframe；避免 egui 与 layer-shell 的复杂集成 |
| 区域选择 | **layer-shell overlay**（首选）/ eframe 全屏无边框窗口（GNOME 降级） | 使用 `zwlr_layer_shell_v1` Overlay 层实现真正的全屏覆盖；GNOME 无 layer-shell，退化为 eframe 无边框全屏窗口 |
| 多屏处理 | 默认当前屏幕；`--full-all` 拼接所有 | 简化用户体验，避免跨屏选区的坐标复杂性 |
| 异步架构 | tokio runtime + mpsc channel | 后端 Wayland 协议需要 async，eframe 是同步的，channel 是最简单的桥接 |
| 单实例 | Unix domain socket | 比文件锁更可靠，天然支持命令转发 |
| 撤销栈 | Command 模式 + 脏矩形 | 支持增量 texture 更新，撤销性能不随历史增长而下降 |
| 编辑器视口 | zoom + pan | 长截图或高 DPI 截图下，画布可能超出屏幕，必须支持缩放和平移 |
| 字体 | fontdb + rustybuzz | fontdb 扫描系统字体无依赖；rustybuzz 纯 Rust，支持复杂脚本 shaping |
| 长截图预览 | 缩略图传输 | 避免每帧在 channel 中传递数 MB 的完整图像 |
| eframe 辅助功能 | 禁用 `accesskit` | `ashpd` 启用 zbus 的 tokio 特性后，accesskit 启动的线程无 tokio runtime 导致 panic；禁用后可避免冲突 |
| 管道优先 | `--stdout` 直接输出 PNG 到 stdout | 类似 grim，通过管道传输给后续程序 |
| 版本策略 | v0.1.0 = M1（CLI截图），v0.2.0 = M2（编辑），v0.3.0 = M3（Pin+长截图） | 先发布最小可用版本，快速获取用户反馈；编辑器/标注作为 v0.2.0 核心卖点 |

---

## 19. 依赖清单

| 功能 | Crate | 版本 | 说明 |
|------|-------|------|------|
| GUI 框架 | `egui`, `eframe` | ^0.30 | 即时模式 GUI，多 viewport 支持 |
| CLI 解析 | `clap` | ^4 | 命令行参数解析 |
| Wayland 客户端 | `smithay-client-toolkit` | ^0.19 | 协议探测、layer-shell（预留）、输出管理 |
| Wayland 协议 | `wayland-client`, `wayland-protocols` | ^0.32 | wlr-screencopy、ext-image-copy-capture 等 |
| Portal D-Bus | `ashpd` | ^0.11 | xdg-desktop-portal 安全封装 |
| 异步运行时 | `tokio` | ^1 | 后端任务、timeout、channel |
| 图像处理 | `image` | ^0.25 | PNG/JPEG/WebP 编解码 |
| 2D 绘制 | `tiny-skia` | ^0.11 | 标注绘制、光栅化 |
| 字体数据库 | `fontdb` | ^0.21 | 系统字体枚举 |
| 字体 shaping | `rustybuzz` | ^0.20 | 复杂文本排版 |
| 剪贴板 | `arboard` | ^3 | 自动调用 wl-copy |
| 配置/目录 | `dirs` | ^5 | 配置文件、缓存路径 |
| 日志 | `tracing`, `tracing-subscriber` | ^0.1 | 结构化日志 |
| 错误处理 | `thiserror`, `anyhow` | ^1 | 结构化错误 / 顶层传播 |
| 工具 | `uuid` | ^1 | Pin 窗口标识 |
| 工具 | `bitflags` | ^2 | CaptureCapabilities |
| 工具 | `shell-words` | ^1 | （已移除，无替代需求） |
| 序列化 | `toml`, `serde` | ^1 | 配置读写 |
| 测试（dev）| `tempfile` | ^3 | 单元测试临时目录 |

---

## 20. 风险缓解

| 风险 | 缓解措施 |
|------|----------|
| eframe 全屏窗口在某些 compositor 上无法真正覆盖所有内容 | 文档说明此限制；测试覆盖 Hyprland/Sway/GNOME/KDE |
| Pin 窗口 `always_on_top` 被 compositor 忽略 | GNOME 已预期降级；wlroots 系通常尊重；文档说明 |
| 多屏混合 DPI 图像模糊 | 逐个输出单独捕获，按 logical_geometry 拼接 |
| 长截图拼接失败（动态内容/无 overlap） | 实时预览让用户观察；提供"强制完成"按钮；ORB 后备 |
| fontdb 在某些系统上找不到字体 | 内嵌一份最小 fallback 字体（如 Noto Sans 子集）作为最终回退 |
| Wayland 协议对象生命周期复杂 | sctk 的 RAII 绑定 + `tracing` 详细日志便于调试 |
| 高分辨率长截图内存溢出 | 检测到高度 > 16384px 时自动停止并警告用户 |
