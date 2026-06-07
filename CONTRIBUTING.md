# wlsnap 贡献指南

感谢您对 wlsnap 的兴趣！本文档涵盖开发环境搭建、代码规范、贡献流程和项目架构，帮助您快速上手。

---

## 许可证

本项目采用 MIT 许可证。您的所有贡献都将被纳入本项目，并遵循相同的许可证。

---

## 贡献流程

1. **Fork 本仓库**：在 GitHub 上 fork 本项目到您的个人账户。

2. **创建分支**：从 `master` 分支拉取最新代码，并基于此创建功能分支。分支命名建议：
   - `feature/xxx` — 新功能
   - `fix/xxx` — 缺陷修复
   - `refactor/xxx` — 代码重构
   - `docs/xxx` — 文档更新

3. **开发与提交**：按照[代码规范](#代码规范)进行开发，确保代码格式、质量和测试覆盖率达标。建议每次提交前运行：
   ```bash
   cargo fmt
   cargo clippy
   cargo test
   ```

4. **推送分支并发起 PR**：将分支推送到 fork 仓库，并发起 Pull Request。PR 标题和描述需遵循 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/) 规范，简明扼要说明变更内容和动机。

5. **代码评审与修改**：项目维护者会进行代码评审并提出修改建议。

6. **合并与发布**：通过评审后，PR 会以 squash 方式合并。

### 注意事项

- 避免直接向 `master` 分支提交代码，始终通过 PR 进行贡献。
- 提交 PR 前，确保自己的分支已经与 `master` 同步，避免合并冲突。
- 对于较大或影响范围广的变更，建议先在 Issue 中充分讨论方案。
- 欢迎任何形式的贡献，包括文档、测试、CI 配置等。

---

## 开发环境

### 环境要求

- **Rust 工具链**：`1.95.0`（由 `rust-toolchain.toml` 锁定）。
  ```bash
  rustup show  # 自动安装正确版本
  ```
- **系统依赖**（Linux）：
  ```bash
  # Debian/Ubuntu
  sudo apt-get install libwayland-dev libwayland-bin libxkbcommon-dev pkg-config

  # Arch Linux
  sudo pacman -S wayland libxkbcommon pkgconf

  # Fedora
  sudo dnf install wayland-devel libxkbcommon-devel pkgconfig
  ```

### 构建项目

```bash
# 调试构建
cargo build

# 发布构建
cargo build --release

# 检查编译（不生成二进制，更快）
cargo check
```

### 运行测试

```bash
# 运行所有测试（headless 环境下约 125 个测试）
cargo test

# 运行特定测试
cargo test <测试名称>

# 运行被忽略的测试（需要 Wayland compositor）
cargo test -- --ignored
```

### 运行应用

```bash
# 查看帮助（需要 Wayland 会话）
cargo run -- --help

# 截图当前屏幕并保存
cargo run -- --screen

# 截图并输出到剪贴板
cargo run -- --screen --clipboard

# 截图所有屏幕并拼接
cargo run -- --all-screen -o ~/screenshot.png
```

---

## 项目架构

### 目录结构

```
src/
  main.rs              # 应用入口：CLI 解析 → 配置加载 → eframe::run_native
  lib.rs               # 库根，pub mod 导出所有子模块
  app.rs               # WlsnapApp：全局状态机 + eframe::App 实现
  cli.rs               # clap v4 命令行定义（CaptureMode）
  cli_action.rs        # CLI headless 截图与输出分发逻辑
  config.rs            # TOML 配置解析、默认值、占位符展开
  error.rs             # 统一错误类型 WlsnapError（thiserror）
  constants.rs         # 常量：APP_NAME、默认路径、默认配置值

  backend/             # 截图后端抽象层
    mod.rs             # CaptureBackend trait、probe_all() 便捷函数
    capabilities.rs    # CaptureCapabilities bitflags
    protocol.rs        # Wayland globals 探测
    wlr.rs             # wlr-screencopy-unstable-v1 实现

  capture/             # 截图流程编排
    mod.rs             # 截图请求/响应类型
    output.rs          # 单屏/多屏捕获、DPI 处理、拼接
    region.rs          # 区域选择逻辑（坐标解析、裁剪）

  image_engine/        # 图像处理引擎
    mod.rs             # 几何类型：LogicalPoint、LogicalRect、Color
    history.rs         # 撤销栈（Command 模式）
    pixmap.rs          # image::RgbaImage ↔ tiny_skia::Pixmap 互操作
    transform.rs       # OutputTransform 旋转/翻转图像

  output_manager/      # 输出与分发
    mod.rs             # OutputAction 枚举 + dispatch 路由
    save.rs            # 文件保存（PNG/JPEG/WebP，占位符展开）
    clipboard.rs       # 剪贴板写入（arboard）
    pipe.rs            # stdout PNG 输出

  platform/            # 平台抽象
    mod.rs             # 类型重导出
    output_info.rs     # OutputInfo、OutputTransform
    wayland.rs         # sctk 枚举 wl_output

  ui/                  # GUI 层
    mod.rs             # 子模块导出
    editor.rs          # 标注编辑器骨架（viewport zoom/pan）
    eframe_selector.rs # eframe 全屏选区（GNOME 降级方案）
    layer_selector.rs  # layer-shell overlay 选区（wlroots 首选）
```

设计文档位于 `docs/dev/`：
- `01-tech-spec.md` — 技术选型方案
- `02-design.md` — 详细架构设计
- `03-roadmap.md` — 开发路线图与任务排期
- `v0.1.0-gap-analysis.md` — v0.1.0 差距分析

配置示例：`config/config.example.toml`

---

## 代码规范

### 格式化

使用 stable 版 `rustfmt` 格式化代码。提交前请运行：

```bash
cargo fmt
```

### 质量检查

使用 `cargo clippy` 检查代码质量：

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

任何警告在 CI 中都会导致失败。不可避免的警告请用 `#[allow]` 或 `#[expect]` 注明原因。

### Unsafe

尽量避免 `unsafe`。如必须使用，请添加 `SAFETY:` 注释解释原因。

### 错误处理

- 库代码使用 `thiserror` 定义结构化错误类型（`crate::error::WlsnapError`）。
- 顶层/二进制代码可使用 `anyhow` 进行错误传播。
- 避免 `unwrap` 和 `expect`，如必须使用请注释说明原因。

### 测试规范

- 新代码应尽量编写测试，修复旧代码时请添加相关 bug 测试。
- 涉及 Wayland 的测试在 headless 环境下应安全降级（如 `enumerate_outputs_without_wayland_display`）。
- 需真实 Wayland 会话的测试请用 `#[ignore]` 标记，避免在 CI 中运行。
- 当前测试策略：纯逻辑模块使用模拟数据做单元测试；需要实际 Wayland 连接的模块不做自动化测试，依赖手动验证。

### 依赖管理

- **Rust crate 依赖只能通过命令行 `cargo add` 管理，禁止直接编辑 `Cargo.toml` 文件。**
- 若需指定 features，使用 `cargo add <crate> --features <feature1>,<feature2>`。
- 若需指定版本，使用 `cargo add <crate>@<version>`。
- 如需新增依赖，请优先考虑社区活跃、维护良好的库，并在 PR 说明中注明用途。

---

## 文档规范

- 命令及配置的新增或修改需同步更新文档。
- 修改代码时，同步更新 `docs/dev/` 下对应的设计文档。
- 技术选型变更时，在 `docs/dev/02-design.md` 的 ADR 章节记录决策。
- 模块级文档注释使用 `//!`，公共 API 使用 `///`。

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
- Wayland 环境变量（`WAYLAND_DISPLAY`）缺失时，所有模块均安全降级，不会 panic。

---

## 截图后端优先级（运行时自动探测）

1. `ext-image-copy-capture-v1`（GNOME 46+、KDE 6.3+、新版 wlroots）
2. `xdg-desktop-portal`（所有现代 DE，GNOME/KDE 主要入口）
3. `wlr-screencopy-unstable-v1`（Hyprland、Sway、Niri 等）

探测结果缓存于 `ProtocolProbe`，启动时一次性完成。

---

## 版本策略

| 版本 | 对应里程碑 | 核心能力 |
|------|-----------|---------|
| **v0.1.0** | M1 | CLI 截图 + 保存/剪贴板/stdout |
| **v0.2.0** | M2 | 区域选择 UI + 标注编辑器 |
| **v0.3.0** | M3 | Pin 贴图 + 长截图（Auto/Manual）|
| **v0.4.0** | M4 | GNOME / KDE Portal 兼容 |
| **v1.0.0** | M5 | 测试覆盖 + 打包 + 稳定 API |

---

## 获取帮助

- 对现有代码有疑问？在 [GitHub Issues](https://github.com/lingdiansr/wlsnap/issues) 中提出。
- 发现 bug？请提交 Issue 并附上复现步骤和 `RUST_LOG=debug` 日志。
- 想讨论新功能？先开 Issue 描述使用场景，避免与路线图冲突。
