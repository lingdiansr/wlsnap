# Wayland 截图工具技术选型方案

> **项目定位**：仅支持 Wayland 的 Linux 截图工具，覆盖 KDE、GNOME、Hyprland、Sway、Niri 等主流桌面环境，提供基础截图、标注编辑、Pin 贴图及长截图功能。

---

## 1. GUI 框架

| 组件 | 选型 | 理由 |
|------|------|------|
| **GUI 框架** | `egui` + `eframe` | 即时模式、轻量、低延迟；通过 `winit` 原生支持 Wayland `text-input-v3`，Fcitx5 / IBus 均可正常工作，满足标注时的文本输入需求 |
| **标注绘制** | `tiny-skia` | 2D 画笔、矩形、箭头、文字渲染，与 egui 集成成本低 |
| **图像处理** | `image` crate | PNG / WebP / JPEG 等格式编解码，裁剪、模糊、像素化 |
| **Wayland 客户端** | `smithay-client-toolkit` (sctk) | layer-shell 表面创建、输出管理、协议探测 |

**IME 实现细节**：
- eframe 的 winit 后端在 Wayland 下自动绑定 `zwp_text_input_v3`。
- egui 的 `TextEdit` 组件直接继承 IME 能力，无需额外处理。
- 若特定 compositor 下 IME 回退异常，可通过 `imekit` 手动补全 `text-input-v3` 绑定作为兜底。

---

## 2. 截图后端：三层自动降级

启动时通过 `smithay-client-toolkit` 枚举 Wayland globals，按以下优先级自动选择最优后端：

| 优先级 | 协议 / 接口 | 适用 DE | 功能覆盖 |
|--------|-------------|---------|----------|
| **P0** | `ext-image-copy-capture-v1` | GNOME 46+、KDE 6.3+、新版 wlroots | 基础截图、区域选择、窗口截图 |
| **P1** | `xdg-desktop-portal` + PipeWire | 所有现代 DE | 基础截图（GNOME / KDE 主要入口） |
| **P2** | `wlr-screencopy-unstable-v1` | Hyprland、Sway、Niri、river 等 | 基础截图、**Auto 长截图**、Pin 贴图 |

---

## 3. 截取范围与多屏处理

### 3.1 区域截图（最通用）
- 创建 `wlr-layer-shell` surface，铺满所有输出：`layer = overlay`，`keyboard-interactivity = none`。
- 背景填充半透明黑色，监听鼠标拖拽计算逻辑坐标。
- **GNOME 降级**：Mutter 不支持 layer-shell，退化为普通全屏窗口（`eframe` `with_fullscreen`），配合 always-on-top 请求。

### 3.2 屏幕 / 输出截图（多屏核心）
- 通过 sctk 枚举 `wl_output`，获取每个显示器的 `name`、`logical_geometry`、`scale_factor`、`transform`、`position`。
- **逐个输出单独捕获**，按 `position` 自行拼接到一张大 canvas 上，避免混合 DPI 导致的拉伸模糊。
- 处理 `transform`（旋转 90°/180°/270°），将物理像素 buffer 转换为逻辑方向后再拼入 canvas。

### 3.3 窗口截图（协议碎片化）
- **wlroots 系**：`wlr-foreign-toplevel-management-unstable-v1` 枚举窗口，获取 `app_id`、`title`、全局坐标；配合 `wlr-toplevel-capture` 直接捕获窗口内容。
- **KDE**：`plasma-window-management` 提供类似能力。
- **GNOME**：`/org/gnome/shell/Introspect` 默认禁用且只读，**窗口截图不可行**，回退到区域截图。

---

## 4. 核心功能

### 4.1 基础截图与编辑
- **范围**：全屏、区域、窗口（尽力而为）、屏幕（多屏）。
- **编辑**：`tiny-skia` 提供画笔、矩形、箭头、文字、马赛克 / 模糊。
- **撤销栈**：在 `image::RgbaImage` 或 `tiny_skia::Pixmap` 层面维护操作历史。

### 4.2 Pin 贴图（置顶浮窗）
- **wlroots 系 / KDE**：`wlr-layer-shell-unstable-v1` 的 `overlay` layer 创建无装饰浮窗，`keyboard-interactivity = none`。
- **GNOME**：Mutter 明确不实现 layer-shell，**Pin 功能完全禁用**，UI 层面不显示按钮。

### 4.3 长截图：Auto / Manual 双模式

| 模式 | 触发条件 | 交互流程 | 适用 DE |
|------|----------|----------|---------|
| **Auto** | 探测到 `wlr-virtual-pointer` + `wlr-screencopy` | 用户框选区域 → 自动发送滚轮事件 → 逐帧捕获 → 实时拼接 → 检测到底自动停止 | Hyprland、Sway、Niri、river |
| **Manual** | 仅 Portal 可用（无 virtual pointer） | 用户框选区域 → 后台以固定间隔 Portal 截图 → 提示用户手动滚动 → 检测到位移自动拼接 → 按 Esc 完成 | **GNOME**、**KDE**、以及用户主动选择的 wlroots 系 |

**拼接算法**（两种模式共用 `Stitcher` 模块）：
- **默认**：Column Sampling（三列灰度 MAD 匹配），复杂度 O(9×H)。
- **可选**：ORB 特征点 + RANSAC，抗表格 / 重复内容干扰，CPU 开销大，作为配置项。

**Manual 模式关键细节**：
- 捕获间隔默认 500ms，避免 Portal 弹窗疲劳（GNOME 46+ restore token 可静默后续授权）。
- 检测"用户已停止滚动"：连续 3 帧无位移时提示"是否完成？"。
- 提供实时预览：显示已捕获帧数和预估输出高度。

---

## 5. 桌面环境兼容性矩阵

| DE / Compositor | 基础截图 | 区域选择 | 窗口截图 | 屏幕截图 | Pin 贴图 | Auto 长截图 | Manual 长截图 |
|-----------------|----------|----------|----------|----------|----------|-------------|---------------|
| **Hyprland** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Sway / river / dwl** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Niri** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **KDE Plasma 6** | ✅ Portal / KWin | ✅ | ✅ | ✅ | ✅ layer-shell | ❌ 禁用 | ✅ |
| **GNOME 46+** | ✅ Portal | ⚠️ 普通窗口 | ❌ | ✅ | ❌ **不支持** | ❌ 禁用 | ✅ |
| **COSMIC** | ✅ | ✅ | ⚠️ 实验 | ✅ | ✅ | ⚠️ 实验 | ✅ |

---

## 6. Rust 依赖清单

| 功能 | Crate | 说明 |
|------|-------|------|
| Wayland 客户端 | `smithay-client-toolkit` | 协议探测、layer-shell、输出管理 |
| Wayland 协议生成 | `wayland-client`, `wayland-protocols` | 含 `wlr-screencopy`、`ext-image-copy-capture`、`wlr-virtual-pointer`、`foreign-toplevel` |
| Portal D-Bus | `ashpd` | xdg-desktop-portal 安全封装，支持 ScreenCast / Screenshot / restore token |
| GUI + IME | `egui`, `eframe` | 即时模式 GUI，winit Wayland IME 原生支持 |
| 图像处理 | `image`, `tiny-skia` | 解码、拼接、标注绘制 |
| 剪贴板 | `arboard` | 自动调用 `wl-copy` / `wl-paste` |
| 配置 / 目录 | `dirs` | 配置文件、缓存路径 |
| 日志 | `tracing` | 结构化日志，便于排查 DE 兼容问题 |
| IME 兜底（可选） | `imekit` | 手动绑定 `text-input-v3`，应对边缘 case |

---

## 7. 项目架构建议

```
src/
  backend/
    portal.rs           # ashpd + PipeWire 截图
    wlr.rs              # wlr-screencopy 原生截图
    ext_capture.rs      # ext-image-copy-capture-v1
    virtual_pointer.rs  # wlr-virtual-pointer 滚动注入（Auto 模式）
    kde_eis.rs          # KWin EIS (Emulated Input Server) 私有接口（可选实验性）
    toplevel.rs         # foreign-toplevel 窗口枚举
  ui/
    app.rs              # egui 主应用状态
    selector.rs         # 区域选择 overlay（layer-shell）
    editor.rs           # 标注面板（画笔 / 文字 / 裁剪 / 打码）
    pinner.rs           # layer-shell 贴图窗口
    scroll_dialog.rs    # 长截图模式选择（Auto / Manual）
  stitcher/
    sampler.rs          # Column Sampling 拼接
    orb.rs              # ORB 特征点拼接（可选）
  output/
    manager.rs          # wl_output 枚举、scale / transform 处理
  main.rs
```

---

## 8. 快捷键方案

Wayland 无统一全局快捷键协议，**不提供应用内全局热键注册**，而是暴露 CLI 参数，由用户在 compositor 配置中自行绑定：

```bash
wlsnap --screen          # 全屏截图
wlsnap --range           # 区域截图
wlsnap --window          # 窗口截图（如果可用）
wlsnap --all-screen      # 所有屏幕拼接
wlsnap --pin             # 贴图（从剪贴板或文件）
wlsnap --scroll-auto     # 自动长截图
wlsnap --scroll-manual   # 手动长截图
```

**各 DE 配置示例**：
- **Hyprland**：`bind = , Print, exec, wlsnap --range`
- **Sway**：`bindsym Print exec wlsnap --range`
- **KDE**：系统设置 → 快捷键 → 自定义快捷键 → 命令
- **GNOME**：设置 → 键盘 → 自定义快捷键

**应用内快捷键**（仅窗口聚焦时）：
- `Ctrl + S` 保存、`Ctrl + Z` 撤销、`Ctrl + C` 复制到剪贴板
- `Esc` 取消、`Enter` 确认、`Tab` 切换截图模式

---

## 9. 开发路线图

### Phase 1：MVP（wlroots 系全功能）
- egui + eframe 验证 IME 在 Hyprland / Sway 下的表现。
- wlr-screencopy + layer-shell 实现截图、选区、Pin。
- wlr-virtual-pointer 实现 Auto 长截图 + Column Sampling 拼接。
- 多屏输出处理（scale factor、transform、position 拼接）。

### Phase 2：GNOME / KDE 兼容
- 接入 `ashpd` Portal，支持基础截图。
- 实现 Manual 长截图（定时 Portal 捕获 + 用户手动滚动）。
- 协议探测与 UI 降级逻辑（GNOME 隐藏 Pin / Auto；KDE 隐藏 Auto）。

### Phase 3：进阶优化
- ext-image-copy-capture 自动探测，降低 wlroots 系延迟。
- KDE KWin EIS (Emulated Input Server) 私有接口实验（若维护成本可控）。
- ORB 特征点拼接作为配置项（抗重复内容）。
- GNOME restore token 静默授权优化。

### Phase 4： polish
- 手动滚动模式自适应捕获间隔（根据内容变化速度动态调整）。
- 窗口截图支持 wlroots `toplevel-capture` 协议。
- 完善 CLI 与错误处理，提供 `--list-outputs`、`--debug` 等诊断参数。

---

## 10. 关键风险与限制

| 限制项 | 说明 |
|--------|------|
| **GNOME Pin 置顶** | Mutter 不实现 layer-shell，生态硬边界，文档中明确说明不支持 |
| **GNOME / KDE Auto 长截图** | 协议层面缺少输入注入路径（无 wlr-virtual-pointer），Manual 是唯一可行 fallback |
| **长截图通用限制** | 仅支持垂直滚动、静态内容（无动画 / 视频）、帧间必须有 overlap |
| **GNOME 窗口截图** | Introspect D-Bus 默认禁用，无可行协议，回退到区域截图 |
| **KDE 窗口截图** | plasma-window-management 可用，但跨版本稳定性需测试 |
