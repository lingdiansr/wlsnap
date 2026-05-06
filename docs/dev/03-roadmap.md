# wlsnap 开发路线图与实现排期

> 基于 `02-design.md` v0.2.0 的详细任务分解、依赖分析与排期建议。

---

## 总体策略

**核心原则**: 先打通「截图 → 显示」的最小闭环，再逐步叠加功能；同层无依赖的任务尽量并行；每完成一个可交付单元即手动验证。

```
Phase 1 (MVP)        Phase 2 (兼容)        Phase 3 (进阶)        Phase 4 (Polish)
─────────────────────────────────────────────────────────────────────────────────
基础骨架 ──► 截图闭环 ──► 编辑器 ──► 高级功能 │ Portal/GNOME │ ORB/EIS   │ 测试/打包
                                           │ KDE 兼容     │ 自适应间隔 │
```

---

## Phase 1: MVP（wlroots 系全功能）

**目标**: 在 Hyprland / Sway / Niri 上，用户能通过 CLI 完成「区域截图 → 编辑 → 保存/复制/贴图/长截图」的完整闭环。

### 任务分解与依赖关系

```
T1 项目骨架 ───────────┬──► T2 平台层 ──► T3 协议探测 ──► T4 wlr-screencopy 后端
(Cargo.toml/          │    (sctk 连接/    (globals 枚举)      (单帧捕获)
 error/config)        │     output 信息)
                      │                           │
                      │    T6 CLI 解析 ───────────┤
                      │    (clap)                 │
                      │                           ▼
                      │                        T5 截图编排
                      │                        (单屏/区域/全屏-all)
                      │                           │
                      └───────────────────────────┤
                                                  ▼
                                               T8 eframe 骨架
                                               (App/update/状态机)
                                                  │
                       ┌──────────────────────────┤
                       ▼                          ▼
                    T7 单实例                  T9 事件桥接
                    (Unix socket)              (tokio mpsc)
                       │                          │
                       └───────────┬──────────────┘
                                   ▼
                         T10 区域选择 UI
                         (eframe 全屏遮罩+选框)
                                   │
                    ┌──────────────┴──────────────┐
                    ▼                             ▼
              T11 图像引擎基础               T12 编辑器骨架
              (Pixmap/RgbaImage互操作            (窗口/画布/texture
               /坐标转换/transform)               /zoom/pan)
                    │                             │
                    ▼                             ▼
              T13 撤销栈                      T14 标注绘制
              (Command模式)                   (pen/rect/arrow/text
                    │                          /mosaic/blur)
                    └──────────────┬──────────────┘
                                   ▼
                              T15 字体系统
                              (fontdb/rustybuzz
                               /widgets字体选择)
                                   │
                    ┌──────────────┴──────────────┐
                    ▼                             ▼
              T16 基础输出                      T17 外部程序调用
              (save/clipboard/stdout)           (--exec/shell-words)
                    │                             │
                    └──────────────┬──────────────┘
                                   ▼
                              T18 Pin 贴图
                              (eframe无边框置顶窗)
                                   │
                    ┌──────────────┴──────────────┐
                    ▼                             ▼
              T19 长截图Auto                     T20 长截图预览
              (virtual-pointer                   (缩略图/帧数/高度)
               + stitcher)
```

### 详细任务说明

| 编号 | 任务 | 前置依赖 | 工作量 | 验收标准 |
|------|------|---------|--------|---------|
| T1 | 项目骨架：初始化 Cargo.toml（19个依赖）、error.rs、constants.rs、config.rs（TOML解析+默认值） | 无 | 小 | `cargo check` 通过；能读取/生成默认配置 |
| T2 | 平台层：`platform/wayland.rs`（sctk初始化、event queue）、`output_info.rs`（枚举wl_output、scale/transform/position） | T1 | 中 | 运行 `--list-outputs` 能打印当前所有屏幕信息 |
| T3 | 协议探测：`backend/protocol.rs`（枚举globals）、`capabilities.rs`（bitflags） | T2 | 小 | 启动时打印探测到的可用协议列表 |
| T4 | wlr-screencopy后端：`backend/wlr.rs`（绑定协议、捕获单帧到DMA-BUF/SHM） | T3 | 大 | 能在Hyprland上捕获一帧并保存为PNG |
| T5 | 截图编排：`capture/output.rs`（单屏/多屏拼接）、`capture/region.rs`（区域参数校验） | T2 + T4 | 中 | `--full`、`--full-all`、`--area`（硬编码区域）均能输出正确图像 |
| T6 | CLI解析：`cli.rs`（clap v4，所有参数） | T1 | 小 | `wlsnap --help` 显示完整参数；参数组合解析正确 |
| T7 | 单实例：`single_instance.rs`（Unix domain socket绑定+命令转发） | T1 | 中 | 启动第二个实例时，第一个实例能收到命令并触发截图 |
| T8 | eframe骨架：`app.rs`（App结构体+状态机定义）、`main.rs`（eframe::run_native） | T1 | 中 | 能启动一个空白eframe窗口；按Esc退出 |
| T9 | 事件桥接：tokio runtime + `mpsc::unbounded_channel` + `ctx.request_repaint()` | T4 + T6 + T8 | 中 | 后台截图完成后，UI线程能收到事件并弹出编辑器 |
| T10 | 区域选择UI：`ui/selector.rs`（eframe全屏无边框窗口、半透明遮罩、鼠标拖拽选框、尺寸标注） | T8 + T9 | 大 | 在屏幕上看到黑色遮罩；拖拽出现高亮框；释放后进入Capturing状态 |
| T11 | 图像引擎基础：`image_engine/mod.rs`（坐标转换）、`pixmap.rs`（Pixmap↔RgbaImage）、`transform.rs`（OutputTransform旋转/翻转） | T1 | 中 | 给定测试图像，旋转90/180/270后像素位置正确；Pixmap与RgbaImage互转无损 |
| T12 | 编辑器骨架：`ui/editor.rs`（eframe窗口、画布显示、滚轮zoom、中键/空格+拖拽pan、Ctrl+0重置） | T8 + T11 | 大 | 打开一张截图后，能zoom到0.5x~5x；能pan移动画布 |
| T13 | 撤销栈：`image_engine/history.rs`（Command trait、push/undo/redo、affected_region脏矩形） | T11 | 中 | Mock Command测试通过；undo/redo栈深度限制生效 |
| T14 | 标注绘制：`image_engine/annotation.rs`（pen/rect/arrow/text/mosaic/blur）、`blur.rs`（高斯模糊/像素化） | T11 + T13 | 大 | 每种工具在画布上留下正确痕迹；undo能精确回退 |
| T15 | 字体系统：`image_engine/font.rs`（fontdb枚举/rustybuzz shaping）、`ui/widgets.rs`（字体选择下拉框） | T1 | 中 | 列出系统所有字体家族；选择"Noto Sans CJK SC"后中文标注渲染正确 |
| T16 | 基础输出：`output_manager/save.rs`（PNG/JPEG/WebP+占位符）、`clipboard.rs`（arboard）、`pipe.rs`（stdout） | T1 | 中 | 保存路径占位符展开正确；JPEG质量可调；剪贴板能粘贴到GIMP |
| T17 | 外部程序调用：`output_manager/exec.rs`（shell-words解析、临时文件、子进程等待） | T16 | 小 | `--exec "gimp {file}"` 成功打开GIMP；程序不存在时保留临时文件 |
| T18 | Pin贴图：`ui/pinner.rs`（ViewportBuilder无边框置顶窗、图像显示、左键拖动、滚轮缩放、右键菜单） | T12 + T16 | 大 | 贴图窗口置顶显示；拖动/缩放/右键菜单功能正常 |
| T19 | 长截图Auto：`capture/scrolling/auto.rs`（virtual-pointer滚动注入）、`stitcher.rs`（Column Sampling）、`virtual_pointer.rs` | T4 + T5 | 大 | 在网页上框选区域后自动滚动并拼接出完整长图 |
| T20 | 长截图预览：`capture/scrolling/preview.rs`（缩略图生成、帧数/高度显示） | T19 | 小 | 长截图过程中显示实时预览小窗，能看到已拼接高度 |

### Phase 1 并行策略

**可并行组**:
- **A组（后端基础）**: T1 → T2 → T3 → T4 → T5 （串行，依赖强）
- **B组（应用骨架）**: T1 → T6 + T7 + T8 （T6/T7/T8 可并行）
- **C组（图像引擎）**: T1 → T11 → T13 → T14 （T11后T13/T14可部分并行）
- **D组（输出分发）**: T1 → T16 → T17 （串行）

**关键路径**: T1 → T2 → T3 → T4 → T5 → T9 → T10 → T12 → T14 → T18/T19

**建议人力分配**（若多人协作）:
- 开发者A：A组（后端协议+截图）
- 开发者B：B组 + D组（CLI+单实例+eframe+输出）
- 开发者C：C组 + T15（图像引擎+字体）

---

## Phase 2: GNOME / KDE 兼容

**目标**: GNOME 46+ 和 KDE Plasma 6 上基础截图可用，Manual 长截图可用。

| 编号 | 任务 | 前置依赖 | 工作量 | 验收标准 |
|------|------|---------|--------|---------|
| T21 | Portal后端：`backend/portal.rs`（ashpd ScreenCast/Screenshot、restore token） | T3 + T5 | 大 | 在GNOME上 `--full` 触发Portal弹窗，授权后能截图 |
| T22 | ext-image-copy-capture后端：`backend/ext_capture.rs` | T3 + T5 | 中 | 在支持该协议的compositor上自动优先使用 |
| T23 | UI降级：`ui/mod.rs` 根据 `capabilities()` 动态隐藏不可用的按钮（GNOME隐藏Pin/Auto；KDE隐藏Auto） | T21 + T18 + T19 | 小 | GNOME下看不到Pin按钮和Auto长截图选项 |
| T24 | Manual长截图：`capture/scrolling/manual.rs`（定时Portal捕获、位移检测、Esc完成） | T21 + T19 | 中 | KDE上 `--scroll-manual` 能手动滚动并拼接 |
| T25 | Portal token持久化：`config.rs` 扩展 + `~/.cache/wlsnap/portal_token.json` | T21 | 小 | GNOME第二次截图不再弹授权窗 |

**并行策略**: T21 / T22 可并行；T23 / T24 / T25 依赖 T21。

---

## Phase 3: 进阶优化

**目标**: 降低延迟、提升长截图鲁棒性。

| 编号 | 任务 | 前置依赖 | 工作量 | 验收标准 |
|------|------|---------|--------|---------|
| T26 | ORB拼接：`capture/scrolling/orb.rs`（ORB特征点+RANSAC、Stitcher trait新实现） | T19 | 大 | 在重复内容页面（如表格、代码）上拼接成功率显著高于Column Sampling |
| T27 | KDE EIS：`backend/kde_eis.rs`（实验性接口） | T3 | 大（研究性） | 在KDE上能绕过Portal直接截图 |
| T28 | 自适应捕获间隔：根据帧间位移速度动态调整manual_capture_interval | T24 | 小 | 快速滚动时间隔缩短，慢速时间隔延长 |

**并行策略**: T26 / T27 / T28 完全可并行。

---

## Phase 4: Polish

**目标**: 完善CLI诊断、日志、测试、打包。

| 编号 | 任务 | 前置依赖 | 工作量 | 验收标准 |
|------|------|---------|--------|---------|
| T29 | CLI诊断：`--list-outputs`（彩色表格输出）、`--debug-protocol`（协议探测详情） | T2 + T3 | 小 | 两个参数均输出人类可读的诊断信息 |
| T30 | tracing日志：全模块 `tracing::info!/warn!/error!` 覆盖、环境变量过滤 | T1 | 中 | `RUST_LOG=debug wlsnap --area` 输出详细调试信息 |
| T31 | 单元测试：`image_engine/stitcher`、`history`、`transform`、`font`、`config` 测试覆盖 | 对应模块 | 中 | `cargo test` 全部通过 |
| T32 | 打包：`wlsnap.desktop`（提供App ID）、安装脚本、AUR/PKGBUILD/Flatpak初版 | T1 | 中 | 能从AUR安装并正常运行 |

---

## 关键里程碑检查点

```
M1 ──► 能截图并保存到文件
     达成条件: T1~T5 + T8 + T9 + T16 完成，手动验证 `--full` 输出PNG正确

M2 ──► 能选区并编辑
     达成条件: T10 + T12 + T14 完成，手动验证选区→编辑→保存闭环

M3 ──► MVP功能完整
     达成条件: T18 + T19 完成，手动验证 Pin + Auto长截图可用

M4 ──► 跨DE兼容
     达成条件: T21 + T24 完成，在GNOME和KDE上手动验证基础截图+Manual长截图

M5 ──► 可发布
     达成条件: T31 + T32 完成，单元测试全过，能通过包管理器安装
```

---

## 时间估算（仅供参考）

| Phase | 任务数 | 预估工时 | 说明 |
|-------|-------|---------|------|
| Phase 1 | 20 | 4~6 周 | 核心工作量，T4/T10/T12/T14/T19 是大头 |
| Phase 2 | 5 | 2~3 周 | Portal D-Bus 调试耗时 |
| Phase 3 | 3 | 1~2 周 | ORB 算法调试耗时；KDE EIS 可能无限期搁置 |
| Phase 4 | 4 | 1~2 周 | 主要是测试和打包 |
| **总计** | **32** | **8~13 周** | 单人全职估算；多人可缩短Phase 1 |

---

## 建议的第一次提交 (Initial Commit)

按此路线图，第一次 commit 应包含 **T1（项目骨架）+ T2（平台层基础）+ T6（CLI解析）+ 部分T8（eframe最小可运行窗口）**，即一个能启动、能枚举输出、能退出的空壳程序。这为后续所有模块提供了编译和运行的基础。
