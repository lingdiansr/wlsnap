# wlsnap

[![CI](https://github.com/lingdiansr/wlsnap/actions/workflows/ci.yml/badge.svg)](https://github.com/lingdiansr/wlsnap/actions/workflows/ci.yml)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

纯 Rust 编写的 Wayland 截图工具，面向 Hyprland、Sway、Niri、GNOME、KDE 等主流 Linux 桌面环境。

## 功能

| 功能 | 状态 | 说明 |
|------|------|------|
| 全屏截图 | ✅ | 当前屏幕 / 所有屏幕拼接 |
| 区域截图 | ✅ | 坐标直输 / 交互式选区 |
| 窗口截图 | 🚧 | 交互式窗口选择（v0.2.0）|
| 剪贴板输出 | ✅ | `-c` / `--clipboard` |
| 管道输出 | ✅ | `--stdout`，grim 风格 |
| 文件保存 | ✅ | `-o <PATH>` / 配置默认路径 |
| 标注编辑 | 🚧 | 画笔、矩形、箭头、文字、马赛克（v0.2.0）|
| Pin 贴图 | 🚧 | 无边框置顶浮窗（v0.3.0）|
| 长截图 | 🚧 | Auto / Manual 双模式（v0.3.0）|

## 安装

### 从源码编译

```bash
git clone https://github.com/lingdiansr/wlsnap.git
cd wlsnap
cargo build --release
```

**系统依赖：**

```bash
# Debian/Ubuntu
sudo apt-get install libwayland-dev libwayland-bin libxkbcommon-dev pkg-config

# Arch Linux
sudo pacman -S wayland libxkbcommon pkgconf

# Fedora
sudo dnf install wayland-devel libxkbcommon-devel pkgconfig
```

### AUR（计划）

```bash
yay -S wlsnap-bin
```

## 快速开始

```bash
# 截图当前屏幕并保存到默认目录
wlsnap

# 截图所有屏幕并拼接
wlsnap --all-screen

# 截图并复制到剪贴板
wlsnap -c

# 截图并输出到 stdout（grim 风格管道传输）
wlsnap --stdout | wl-copy

# 截图并保存到指定路径
wlsnap -o ~/screenshot.png

# 区域截图（交互式选区）
wlsnap --range

# 区域截图（直接坐标）
wlsnap -r 100,200,500,400

# 列出所有显示器
wlsnap --list-outputs

# 查看支持的 Wayland 协议
wlsnap --debug-protocol
```

## CLI 参数

```
Usage: wlsnap [OPTIONS]

Capture Mode:
      --screen          当前屏幕全屏 [aliases: --full]
  -a, --all-screen      所有屏幕拼接 [aliases: --full-all]
  -r, --range [X,Y,W,H] 区域截图（无值=交互选区，有值=直接坐标）
      --window          窗口截图（交互式，需 GUI）
      --pin [PATH]      Pin 贴图（需 GUI）
      --scroll-auto     自动长截图（需 GUI）
      --scroll-manual   手动长截图（需 GUI）

Output:
      --stdout          PNG 输出到 stdout（管道传输）
  -o, --output <PATH>   保存到指定路径
  -c, --clipboard       复制到剪贴板

Other:
      --cursor          包含鼠标光标
      --list-outputs    列出显示器并退出
      --debug-protocol  打印协议探测信息
  -h, --help            帮助
  -V, --version         版本
```

## 配置

配置文件路径：`~/.config/wlsnap/config.toml`

首次启动若不存在，自动生成默认配置。示例见 [`config/config.example.toml`](config/config.example.toml)。

```toml
[general]
post_capture = "save"          # save / clipboard / pipe / edit / ask
save_dir = "{HOME}/Pictures/Screenshots"
filename_template = "wlsnap_{date}_{time}_{mode}"
format = "png"                 # png / jpeg / webp
jpeg_quality = 90

[advanced]
include_cursor = false
log_level = "info"
```

## 截图后端优先级

运行时自动探测，按以下顺序选择：

1. `ext-image-copy-capture-v1`（GNOME 46+、KDE 6.3+、新版 wlroots）
2. `xdg-desktop-portal`（所有现代桌面环境）
3. `wlr-screencopy-unstable-v1`（Hyprland、Sway、Niri 等）

## 快捷键绑定示例

wlsnap 不提供全局热键，请在 compositor 配置中自行绑定：

**Hyprland:**
```conf
bind = , Print, exec, wlsnap --screen -c
bind = SHIFT, Print, exec, wlsnap --all-screen
bind = CTRL, Print, exec, wlsnap --range
```

**Sway:**
```conf
bindsym Print exec wlsnap --screen -c
bindsym Shift+Print exec wlsnap --all-screen
bindsym Ctrl+Print exec wlsnap --range
```

**Niri:**
```conf
binds {
    Mod+Shift+S { spawn "wlsnap" "--range" "-c"; }
}
```

## 开发

```bash
# 编译
cargo build

# 运行测试
cargo test

# 格式化 + 静态检查
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
```

详细开发指南见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 路线图

| 版本 | 目标 |
|------|------|
| v0.1.0 | CLI 截图闭环（当前）|
| v0.2.0 | 区域选择 UI + 标注编辑器 |
| v0.3.0 | Pin 贴图 + 长截图 |
| v0.4.0 | GNOME / KDE Portal 兼容完善 |
| v1.0.0 | 生产就绪 |

## 许可证

[GPL-3.0](LICENSE)
