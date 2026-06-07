# wlsnap 项目规则

## 项目概述

wlsnap 是一款仅支持 Wayland 的 Linux 截图工具，面向 KDE、GNOME、Hyprland、Sway、Niri 等主流桌面环境。项目基于 Rust 编写，使用 `egui` + `eframe` 构建 GUI，`smithay-client-toolkit` (sctk) 进行 Wayland 协议交互。当前处于早期开发阶段（v0.1.0），已实现项目骨架、协议探测、基础截图后端、图像引擎、输出管理和编辑器骨架，尚未完成完整用户交互闭环。

核心功能规划：
- 基础截图：全屏、区域、窗口、多屏拼接
- 标注编辑：画笔、矩形、箭头、文字、马赛克/模糊
- Pin 贴图：无边框置顶浮窗
- 长截图：Auto（自动滚动+拼接）/ Manual（手动滚动+定时捕获）双模式

项目不提供全局热键，纯 CLI 驱动，由用户在 compositor 配置中自行绑定快捷键。

---

## 技术栈

| 用途 | 依赖 |
|------|------|
| GUI 框架 | `egui` + `eframe`（即时模式，Wayland 原生支持） |
| Wayland 客户端 | `smithay-client-toolkit` (sctk 0.20) |
| Wayland 协议生成 | `wayland-client`, `wayland-protocols`, `wayland-protocols-wlr` |
| Portal D-Bus | `ashpd` |
| 图像处理 | `image`, `tiny-skia` |
| 字体 | `fontdb`, `rustybuzz` |
| 剪贴板 | `arboard` |
| 异步运行时 | `tokio` (full features) |
| CLI | `clap` v4 (derive) |
| 配置/目录 | `dirs`, `toml`, `serde` |
| 日志 | `tracing`, `tracing-subscriber` |
| 其他 | `anyhow`, `thiserror`, `bitflags`, `chrono`, `uuid`, `tempfile`, `memmap2`, `shell-words`, `libc` |

Rust 工具链：`1.95.0`，组件包含 `rustfmt`、`clippy`、`rust-src`，目标平台 `x86_64-unknown-linux-gnu`。Edition 为 `2024`。

---

## 项目结构

```
src/
  main.rs              # 应用入口：初始化 tracing → 加载配置 → eframe::run_native
  lib.rs               # 库根，pub mod 导出所有子模块
  app.rs               # WlsnapApp：全局状态机 + eframe::App 实现 + tokio runtime
  cli.rs               # clap v4 命令行定义（CaptureMode / PostCaptureAction）
  config.rs            # TOML 配置解析、默认值、占位符展开
  error.rs             # 统一错误类型 WlsnapError（thiserror）
  constants.rs         # 常量：APP_NAME、默认路径、默认配置值

  backend/             # 截图后端抽象层
    mod.rs             # CaptureBackend trait、probe_all() 便捷函数
    capabilities.rs    # CaptureCapabilities bitflags
    protocol.rs        # Wayland globals 探测（含 Portal D-Bus ping）
    wlr.rs             # wlr-screencopy-unstable-v1 实现（SHM 捕获 + BGRA→RGBA）

  image_engine/        # 图像处理引擎
    mod.rs             # 几何类型：LogicalPoint、LogicalRect、PhysicalPoint、PhysicalRect、Color
    history.rs         # 撤销栈（Command 模式，基于 tiny_skia::Pixmap）
    pixmap.rs          # image::RgbaImage ↔ tiny_skia::Pixmap 互操作
    transform.rs       # OutputTransform 旋转/翻转图像

  output_manager/      # 输出与分发
    mod.rs             # OutputAction 枚举 + dispatch 路由
    save.rs            # 文件保存（PNG/JPEG/WebP，占位符展开，Unix 权限 0o600）
    clipboard.rs       # 剪贴板写入（arboard）
    pipe.rs            # stdout PNG 输出
    exec.rs            # --exec 外部命令调用（shell-words 解析 + 临时文件）

  platform/            # 平台抽象
    mod.rs             # 类型重导出
    output_info.rs     # OutputInfo、OutputTransform、LogicalPoint、LogicalRect
    wayland.rs         # sctk 枚举 wl_output（scale、transform、position）

  ui/                  # GUI 层
    mod.rs             # 子模块导出
    editor.rs          # 标注编辑器骨架（viewport zoom/pan、texture 缓存）
```

设计文档位于 `docs/dev/`：
- `01-tech-spec.md` — 技术选型方案
- `02-design.md` — 详细架构设计
- `03-roadmap.md` — 开发路线图与任务排期

配置示例：`config/config.example.toml`

---

## 构建与测试命令

```bash
# 编译
cargo build

# 运行（需要 Wayland 会话）
cargo run -- --help

# 运行测试（共 70 个单元测试，其中 1 个被 ignore，需在 wlr compositor 下运行）
cargo test

# 代码格式化检查
cargo fmt -- --check

# 静态分析
cargo clippy
```

> 注意：`cargo fmt --check` 当前会报告部分文件存在格式差异（主要是 `src/backend/wlr.rs`、`src/output_manager/save.rs`、`src/platform/output_info.rs`、`src/platform/wayland.rs`、`src/ui/editor.rs`、`src/app.rs`），建议执行 `cargo fmt` 统一风格。

---

## 代码风格规范

- 使用 `cargo fmt` 统一格式化，不要手动调整 import 顺序或换行。
- 模块级文档注释使用 `//!`，公共 API 使用 `///`。
- 错误处理统一使用 `crate::error::{Result, WlsnapError}`，不要混用 `anyhow` 在库代码中（`anyhow` 目前仅在顶层使用）。
- `unsafe` 代码需附带 `SAFETY:` 注释（参考 `history.rs` 中的 undo/redo 指针用法）。
- 测试覆盖要求：每个公共模块均包含 `#[cfg(test)]` 测试子模块，优先用纯逻辑测试（不依赖 Wayland 显示）。
- 涉及 Wayland 的测试在 headless 环境下应安全降级（如 `enumerate_outputs_without_wayland_display`）。

---

## 测试策略

- **单元测试**：每个 `.rs` 文件底部均包含 `mod tests`，覆盖核心逻辑（坐标转换、颜色解析、撤销栈、配置序列化、CLI 解析、图像格式保存等）。
- **集成测试**：`backend/wlr.rs` 中的 `capture_real_output` 被 `#[ignore]`，需在真实 wlr compositor 会话下手动运行。
- **CI 友好**：所有非 ignore 测试均可在无 Wayland 显示、无剪贴板服务的环境下通过。
- **当前状态**：`cargo test` 共 70 个测试，66 passed + 1 ignored + 3 来自 binary（全部通过）。

---

## 依赖管理

- **Rust crate 依赖只能通过命令行 `cargo add` 管理，禁止直接编辑 `Cargo.toml` 文件。**
- 若需指定 features，使用 `cargo add <crate> --features <feature1>,<feature2>`。
- 若需指定版本，使用 `cargo add <crate>@<version>`。
- `Cargo.toml` 中 `[package]` 段的基础元信息（name, version, edition）除外，可在初始化时配置。

---

## 文档维护

- 修改代码时，同步更新 `docs/dev/` 下对应的设计文档。
- 技术选型变更时，在 `docs/dev/02-design.md` 的 ADR 章节记录决策。

## 贡献指南

- 详细的开发环境搭建、代码规范、贡献流程请参考 [`CONTRIBUTING.md`](CONTRIBUTING.md)。
- 提交前请运行 `cargo fmt`、`cargo clippy` 和 `cargo test` 确保质量。
- PR 标题遵循 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/) 规范。

---

## 配置系统

- 配置文件路径：`~/.config/wlsnap/config.toml`
- 首次启动若不存在，自动生成默认配置（目录权限 `0o700`，文件权限 `0o600`）。
- 支持占位符：`{HOME}`、`{DATE}`、`{TIME}`、`{date}`、`{time}`、`{mode}`、`{timestamp}`
- 配置段：`general`、`editor`、`pin`、`scrolling`、`shortcuts`、`advanced`
- 示例参考 `config/config.example.toml`

---

## 安全注意事项

- 配置文件和截图保存目录在 Unix 下强制设置 `0o700` / `0o600` 权限，避免其他用户读取。
- Portal restore token 默认持久化到 `~/.cache/wlsnap/portal_token.json`，可通过配置关闭。
- `exec.rs` 使用 `shell_words::split` 解析外部命令，避免简单字符串拼接导致的注入风险；临时文件在命令失败时保留以便调试。
- Wayland 环境变量（`WAYLAND_DISPLAY`）缺失时，所有模块均安全降级，不会 panic。

---

## 截图后端优先级（运行时自动探测）

1. `ext-image-copy-capture-v1`（GNOME 46+、KDE 6.3+、新版 wlroots）
2. `xdg-desktop-portal`（所有现代 DE，GNOME/KDE 主要入口）
3. `wlr-screencopy-unstable-v1`（Hyprland、Sway、Niri 等）

探测结果缓存于 `ProtocolProbe`，启动时一次性完成，避免重复与 compositor 往返。

---

## 开发状态速览

| 模块 | 状态 |
|------|------|
| CLI / 配置 / 错误 | 完整 |
| Wayland 输出枚举 | 完整 |
| 协议探测 | 完整 |
| wlr-screencopy 后端 | 完整（单帧 SHM 捕获） |
| 图像引擎（坐标/颜色/变换/互操作） | 完整 |
| 撤销栈 | 完整 |
| 输出管理（保存/剪贴板/管道/执行） | 完整 |
| 编辑器骨架（zoom/pan/texture） | 骨架完成，标注工具待实现 |
| 区域选择 UI | 尚未实现 |
| Pin 贴图 | 尚未实现 |
| 长截图（Auto/Manual） | 尚未实现 |
| Portal 后端 | 尚未实现 |
| ext-image-copy-capture 后端 | 尚未实现 |
| 单实例守护 | 尚未实现 |
