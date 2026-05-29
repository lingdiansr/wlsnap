# 开源项目参考方案：wayscrollshot & mark-shot

> 文档版本: 1.0.0 | 日期: 2026-05-29
> 参考项目:
> - [jswysnemc/wayscrollshot](https://github.com/jswysnemc/wayscrollshot) (MIT License) — Wayland 长截图工具
> - [jswysnemc/mark-shot](https://github.com/jswysnemc/mark-shot) (MIT License) — Qt6 Wayland 截图标注工具
>
> 本地克隆路径: `refs/wayscrollshot/`、`refs/mark-shot/`

---

## 1. 概述

本文档记录 wlsnap 项目从两个开源参考项目中可借鉴的内容，包括:

1. **wayscrollshot** (Rust) — 长截图拼接算法、实时预览流水线、多算法降级策略
2. **mark-shot** (C++/Qt6) — 标注编辑器 UI/UX 设计、工具交互模式、Pin 贴图功能设计

所有代码借鉴遵循 MIT License 要求，在实现文件中保留原始版权声明。

---

## 2. wayscrollshot 参考详情

### 2.1 项目概况

| 属性 | 内容 |
|------|------|
| 技术栈 | Rust + sctk + tiny-skia + `image` crate + OpenCV |
| 功能 | Wayland 长截图（自动滚动+实时拼接） |
| 捕获方式 | 调用外部 `grim` + `slurp` |
| 预览窗口 | `wlr-layer-shell` 浮窗（sctk 实现） |
| 拼接算法 | 5 种: ColSample / Template / Edge / FAST+HNSW / OpenCV ORB+RANSAC |

### 2.2 与 wlsnap 的契合度

| 维度 | wayscrollshot | wlsnap | 契合度 |
|------|--------------|--------|--------|
| 语言 | Rust | Rust | ✅ 完全一致 |
| 图像处理 | `image` crate + tiny-skia | `image` crate + tiny-skia | ✅ 完全一致 |
| Wayland 客户端 | sctk | sctk | ✅ 完全一致 |
| 截图方式 | 外部 grim | 原生 wlr-screencopy + Portal | ⚠️ 架构不同 |
| UI | layer-shell 浮窗 | eframe | ⚠️ 需适配 |

### 2.3 可直接借鉴的模块

#### 2.3.1 Stitcher 拼接算法 (`src/stitch.rs`)

**核心设计:**

```rust
pub struct Stitcher {
    full_image: Option<Arc<RgbaImage>>,
    last_frame: Option<RgbaImage>,
    last_cols: Option<ColSamples>,
    last_fast_index: Option<FastIndex>,
    last_fast_gray: Option<GrayImage>,
    last_offset: i32,
    stats: StitchStats,
    config: MatchConfig,
}

pub enum StitchOutcome {
    FirstFrame,
    Appended { added: u32 },
    NoProgress,
    NoMatch,
}
```

**算法枚举:**

```rust
pub enum Algorithm {
    ColSample,    // 列采样（快速，适用大多数场景）
    Template,     // 模板匹配（较慢，更精确）
    Edge,         // 边缘检测（透明背景）
    Fast,         // FAST 角点 + HNSW 索引（高精度）
    OpenCvOrb,    // OpenCV ORB + RANSAC（默认）
}
```

**wlsnap 移植方案:**

在 `src/capture/scrolling/stitcher.rs` 中实现 `Stitcher` trait:

```rust
// src/capture/scrolling/stitcher.rs

pub trait Stitcher: Send {
    fn push_frame(&mut self, frame: &RgbaImage) -> StitchResult;
    fn full_image(&self) -> Option<&RgbaImage>;
    fn stats(&self) -> StitchStats;
}

pub enum StitchResult {
    FirstFrame,
    Appended { added_height: u32 },
    NoProgress,
    NoMatch,
}

#[derive(Clone, Debug, Default)]
pub struct StitchStats {
    pub frame_count: u32,
    pub total_height: u32,
    pub last_append: u32,
}
```

**Phase 1 实现:** `ColumnSamplingStitcher`（不依赖 OpenCV）
**Phase 3 扩展:** `OrbStitcher`（需引入 OpenCV 或纯 Rust ORB 实现）

#### 2.3.2 Column Sampling 算法

**采样位置定义:**

```rust
fn col_sampling(img: &RgbaImage) -> ColSamples {
    let w = img.width() as usize;
    let h = img.height() as usize;

    // 3 个列组，每组内等距取 3 个采样点
    let groups: Vec<Vec<usize>> = vec![
        linspace(20.min(w - 1), w / 4, 3),        // 左列组
        linspace(w / 2, 5 * w / 8, 3),            // 中列组
        linspace(6 * w / 8, 7 * w / 8, 3),        // 右列组
    ];

    // 返回: Vec<Vec<f32>>，外层索引 = y 坐标，内层索引 = 列组索引
}
```

**重叠搜索策略:**

```rust
fn diff_overlap(
    cols1: &ColSamples,      // 上一帧的列采样
    cols2: &ColSamples,      // 当前帧的列采样
    predict: i32,            // 预测偏移量（基于上一次成功拼接）
    approx_diff: f32,        // 近似阈值
    min_overlap: u32,        // 最小重叠高度
) -> (i32, f32) {
    // 1. 从预测偏移量开始搜索
    // 2. 搜索顺序: [p, p+1, p-1, p+2, p-2, ...]
    // 3. 计算每对重叠区域的平均绝对差 (MAD)
    // 4. 提前终止条件:
    //    - 连续 10 次低于 approx_diff
    //    - 单次低于 approx_diff / 4
}
```

**关键常量（wayscrollshot 默认值）:**

| 常量 | 值 | 说明 |
|------|-----|------|
| `min_overlap` | 100 (ColSample) / 120 (ORB) | 最小重叠像素 |
| `accept_diff` | 5.0 (ColSample) / 3.5 (ORB) | 接受阈值 |
| `min_append` | 15 (ColSample) / 10 (ORB) | 最小追加高度 |
| `approx_diff` | 1.0 | 近似匹配阈值 |

#### 2.3.3 帧签名去重 (`src/session.rs`)

**目的:** 避免内容未滚动时重复捕获相同帧。

```rust
const SIGNATURE_COLS: u32 = 18;
const SIGNATURE_ROWS: u32 = 24;
const DUPLICATE_AVG_DIFF: f32 = 1.1;
const DUPLICATE_MAX_DIFF: u8 = 4;

fn frame_signature(frame: &RgbaImage, cols: u32, rows: u32) -> Vec<u8> {
    // 在帧上均匀采样 18×24 个灰度值作为签名
}

fn is_duplicate_signature(previous: &[u8], current: &[u8]) -> bool {
    // 平均差 ≤ 1.1 且最大差 ≤ 4 视为重复帧
}
```

**wlsnap 移植:** 在 `src/capture/scrolling/auto.rs` 的捕获循环中使用。

#### 2.3.4 预览缩略图生成

```rust
pub fn build_preview(image: &RgbaImage, fixed_width: u32) -> PreviewImage {
    let scale = (fixed_width as f32) / (image.width() as f32).max(1.0);
    let target_height = ((image.height() as f32) * scale).round().max(1.0) as u32;
    let resized = imageops::resize(image, fixed_width, target_height, FilterType::Triangle);
    PreviewImage {
        width: resized.width(),
        height: resized.height(),
        pixels: resized.into_raw(),
    }
}
```

**wlsnap 移植:** 在 `BackendEvent::ScrollProgress` 中发送缩略图而非完整图像。

#### 2.3.5 捕获-拼接-预览流水线架构

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│   Capture   │────>│   Stitcher   │────>│   Preview   │
│   (grim)    │     │ (col-sample) │     │ (layer-shell)│
└─────────────┘     └──────────────┘     └─────────────┘
```

**wlsnap 适配:**

```
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐
│   Capture   │────>│   Stitcher   │────>│  BackendEvent   │
│(wlr-screencopy)   │ (col-sample) │     │ (mpsc channel)  │
└─────────────┘     └──────────────┘     └─────────────────┘
                                                │
                                                ▼
                                        ┌───────────────┐
                                        │  eframe UI    │
                                        │ (缩略图预览)   │
                                        └───────────────┘
```

### 2.4 不借鉴的内容

| 内容 | 原因 |
|------|------|
| 外部 `grim`/`slurp` 调用 | wlsnap 原生 Wayland 捕获 |
| `layer-shell` 预览浮窗 | wlsnap 使用 eframe，需自行实现预览窗口 |
| OpenCV 依赖（Phase 1） | 保持纯 Rust，Phase 3 再考虑 |

---

## 3. mark-shot 参考详情

### 3.1 项目概况

| 属性 | 内容 |
|------|------|
| 技术栈 | C++17 + Qt6 + layer-shell-qt |
| 功能 | 截图选区 + 标注编辑 + Pin 贴图 + OCR/翻译 |
| 捕获方式 | 外部 `grim` |
| 标注渲染 | QPainter |
| 窗口管理 | layer-shell-qt / XDG 全屏窗口 |

### 3.2 与 wlsnap 的契合度

| 维度 | mark-shot | wlsnap | 契合度 |
|------|----------|--------|--------|
| 语言 | C++ | Rust | ❌ 无法直接移植代码 |
| GUI 框架 | Qt6 | egui/eframe | ⚠️ 仅设计思路参考 |
| 渲染 | QPainter | tiny-skia | ⚠️ 算法思路参考 |
| Wayland | layer-shell-qt | eframe + sctk | ⚠️ 架构不同 |

**结论:** mark-shot 仅作为 **UI/UX 设计思路参考**，不移植任何代码。

### 3.3 标注工具设计参考

#### 3.3.1 工具集对照

| 工具 | mark-shot | wlsnap 规划 | 设计亮点 |
|------|-----------|-------------|----------|
| 移动/平移 | `V` | 中键/空格+拖拽 | — |
| 选择 | `S` | 默认工具 | 框选多标注、拖拽调整、Delete 删除 |
| 画笔 | `P` | ✅ | 平滑贝塞尔曲线 |
| 直线 | `L` | ✅ | — |
| 高亮笔 | `H` | ✅ | 半透明叠加 |
| 矩形 | `R` | ✅ | `Ctrl` 约束为正方形 |
| 椭圆 | `E` | ✅ | `Ctrl` 约束为圆形 |
| 箭头 | `A` | ✅ | 6 顶点锐角箭头 |
| 文字 | `T` | ✅ | 双手势缩放、物理宽度缓冲 |
| 序号 | `N` | 待规划 | 自动递增标记 |
| 马赛克 | `M` | ✅ | 亚克力磨砂效果 |
| 激光笔 | `G` | 待规划 | 1800ms 自动淡出 |

#### 3.3.2 箭头绘制算法（QPainter → tiny-skia 思路）

mark-shot 使用 6 顶点锐角箭头:

```cpp
// 箭头由 6 个顶点组成:
// 1. 箭尾左侧
// 2. 箭尾右侧
// 3. 箭身靠近箭头处
// 4. 箭头左侧翼
// 5. 箭头尖端
// 6. 箭头右侧翼
// 使用 QPainterPath 绘制闭合路径
```

**wlsnap 实现思路:** 使用 `tiny_skia::PathBuilder` 构建相同顶点路径。

#### 3.3.3 文字标注交互设计

| 交互 | mark-shot 行为 | wlsnap 建议 |
|------|---------------|-------------|
| 初始放置 | 点击画布放置文本框 | 相同 |
| 字体大小调节 | 滚轮调节 / 属性面板滑块 | 滚轮调节 |
| 边界调整 | 对角线手柄=等比缩放 / 边框=仅调宽度 | 相同 |
| 最大字号 | 1000px | 100px（更合理） |
| 背景色 | 可设半透明背景 | 支持 |

#### 3.3.4 滚轮动态调节设计

mark-shot 的创新设计：激活工具时滚轮不缩放画布，而是调节工具参数:

| 当前工具 | 滚轮行为 |
|---------|---------|
| Pen / Line / Highlighter | 调节笔画宽度 |
| Rectangle / Ellipse / Arrow | 调节描边宽度 |
| Text | 调节字体大小 |
| Number | 调节序号缩放比例 |
| Mosaic | 调节马赛克块大小 |
| Laser | 调节激光笔宽度 |

**wlsnap 建议:** 在 `Select` 工具下滚轮缩放画布，其他工具下滚轮调节参数。

#### 3.3.5 选区+标注一体化流程

```
mark-shot 流程:
1. 启动 → 全屏冻结当前画面
2. 模式: Selecting
   - 拖拽绘制选区矩形
   - 选区有 8 方向调整手柄
   - 按 F 切换全屏/选区范围
3. 确认选区 → 模式切换为 Editing
   - 显示工具栏
   - 可在选区内标注
   - 也可调整选区
4. 保存/复制/贴图
```

**wlsnap 适配:** wlsnap 的 `--area` 流程已类似，但选区确认后进入编辑器而非原地标注。mark-shot 的"原地标注"模式可作为未来增强功能参考。

#### 3.3.6 撤销/重做设计对比

| 维度 | mark-shot | wlsnap (设计文档) |
|------|-----------|------------------|
| 实现方式 | `HistorySnapshot` 保存完整标注列表 | `Command` trait（单命令粒度） |
| 内存占用 | 高（完整列表拷贝） | 低（仅受影响区域） |
| 性能 | 快照恢复 O(n) | 执行/撤销 O(1) |
| 粒度 | 粗（每次操作一个快照） | 细（每个绘制动作） |

**结论:** wlsnap 的 Command 模式更优，无需参考 mark-shot。

### 3.4 Pin 贴图设计参考

#### 3.4.1 功能对照

| 功能 | mark-shot | wlsnap 规划 |
|------|-----------|-------------|
| 窗口类型 | 无边框 + 置顶 | 相同 |
| 左键拖动 | 移动窗口 | 相同 |
| 滚轮缩放 | 缩放窗口 | 相同 |
| 双击关闭 | ✅ | 待规划 |
| Esc 关闭 | ✅ | ✅ |
| 右键菜单 | 旋转/缩放/复制/保存/关闭/透明度 | 保存/复制/关闭/透明度 |
| 透明度范围 | 0.2 ~ 1.0 | 0.2 ~ 1.0 |
| OCR 文字识别 | ✅ (rapidocr/tesseract) | 未来扩展 |
| LLM 翻译 | ✅ (OpenAI API) | 未来扩展 |

#### 3.4.2 贴图窗口交互细节

```
mark-shot PinnedImageWindow:
- 初始大小: 不超过屏幕 90%
- 初始位置: 屏幕中心
- 缩放范围: 0.1x ~ 6.0x
- 最小尺寸: 24×24 像素
- 缩放锚点: 以鼠标位置为锚点缩放
- 双击 Ctrl: 重置原始比例
```

**wlsnap 建议:** 采用相同参数。

#### 3.4.3 防误触设计

mark-shot 的 `LeftClickMenuFilter`：右键菜单弹出时，屏蔽菜单区域内的非左键点击，避免误触发底层操作。

**wlsnap 建议:** egui 的 `Area` + `Sense` 可实现类似效果。

### 3.5 快捷键设计参考

#### 3.5.1 工具切换快捷键

| 快捷键 | mark-shot | wlsnap 建议 |
|--------|-----------|-------------|
| `V` | 移动/平移 | 相同 |
| `S` | 选择 | 相同 |
| `P` | 画笔 | 相同 |
| `L` | 直线 | 相同 |
| `H` | 高亮笔 | 相同 |
| `R` | 矩形 | 相同 |
| `E` | 椭圆 | 相同 |
| `A` | 箭头 | 相同 |
| `T` | 文字 | 相同 |
| `N` | 序号 | 相同 |
| `M` | 马赛克 | 相同 |
| `G` | 激光笔 | 相同 |

#### 3.5.2 全局动作快捷键

| 快捷键 | mark-shot | wlsnap 规划 |
|--------|-----------|-------------|
| `Esc` | 关闭窗口 | 相同 |
| `Ctrl+Z` | 撤销 | 相同 |
| `Ctrl+Y` / `Ctrl+Shift+Z` | 重做 | 相同 |
| `Ctrl+C` | 复制到剪贴板 | 相同 |
| `Ctrl+S` / `Enter` | 保存 | 相同 |
| `Backspace` / `Delete` | 删除选中标注 | 相同 |
| `F` | 切换全屏/选区 | 可作为扩展 |
| `Ctrl+滚轮` | 缩放画布 | 相同 |
| `中键拖拽` | 平移画布 | 相同 |

#### 3.5.3 高级交互技巧

| 技巧 | mark-shot | wlsnap 建议 |
|------|-----------|-------------|
| `Ctrl` + 绘制矩形/椭圆 | 约束为正方形/圆形 | ✅ 实现 |
| 右键单击画布 | 快速切换到选择工具 | ✅ 实现 |
| 双击右键 | 打开径向调色盘 | 可作为扩展 |
| 滚轮（非选择工具） | 调节当前工具参数 | ✅ 实现 |

---

## 4. 实施路线图

### 4.1 Phase 1: 长截图基础（参考 wayscrollshot）

| 任务 | 参考来源 | 工作量 |
|------|---------|--------|
| `Stitcher` trait 定义 | wayscrollshot `Stitcher` | 小 |
| `ColumnSamplingStitcher` 实现 | wayscrollshot `col_sampling` + `diff_overlap` | 中 |
| `EdgeStitcher` 实现 | wayscrollshot `col_sampling_edge` | 小 |
| 帧签名去重 | wayscrollshot `frame_signature` | 小 |
| 预览缩略图生成 | wayscrollshot `build_preview` | 小 |
| 捕获-拼接-预览流水线 | wayscrollshot `session.rs` | 中 |

### 4.2 Phase 1: 标注编辑器增强（参考 mark-shot）

| 任务 | 参考来源 | 工作量 |
|------|---------|--------|
| 快捷键映射（工具切换） | mark-shot 快捷键表 | 小 |
| 滚轮工具参数调节 | mark-shot 滚轮设计 | 中 |
| `Ctrl` 约束绘制 | mark-shot 约束设计 | 小 |
| 箭头 6 顶点算法 | mark-shot 箭头绘制 | 小 |
| 序号标注工具 | mark-shot Number 工具 | 中 |

### 4.3 Phase 3: 高级拼接算法（参考 wayscrollshot）

| 任务 | 参考来源 | 工作量 |
|------|---------|--------|
| `TemplateStitcher` 实现 | wayscrollshot `find_offset_template` | 中 |
| `OrbStitcher` 实现 | wayscrollshot `estimate_orb_offset` | 大（需 OpenCV） |
| `FastStitcher` 实现 | wayscrollshot `find_offset_fast` | 大（需 HNSW） |
| 多算法自动降级 | wayscrollshot 降级链 | 中 |

### 4.4 未来扩展（参考 mark-shot）

| 功能 | 参考来源 | 优先级 |
|------|---------|--------|
| 激光笔工具 | mark-shot Laser | 低 |
| OCR 文字识别 | mark-shot OCR | 低 |
| LLM 翻译 | mark-shot Translation | 低 |
| 扩展命令系统 | mark-shot `extensions.json` | 低 |

---

## 5. 代码移植规范

### 5.1 文件对应关系

```
wayscrollshot/                      wlsnap/
├── src/stitch.rs        ───►      src/capture/scrolling/stitcher.rs
├── src/session.rs       ───►      src/capture/scrolling/auto.rs
├── src/types.rs         ───►      src/capture/scrolling/mod.rs
├── src/capture.rs       ───►      (原生捕获替代)
├── src/overlay.rs       ───►      (eframe 替代)
└── src/overlay/drawing.rs ───►    src/ui/widgets.rs
```

### 5.2 版权声明

在移植代码的文件头部添加:

```rust
// Portions of this file are derived from wayscrollshot by Tadokoro Koji
// (https://github.com/jswysnemc/wayscrollshot), licensed under the MIT License.
// Copyright (c) 2025 Tadokoro Koji
```

### 5.3 常量对照表

| wayscrollshot 常量 | wlsnap 常量名 | 值 | 说明 |
|-------------------|--------------|-----|------|
| `CAPTURE_INTERVAL` | `AUTO_CAPTURE_INTERVAL_MS` | 45ms | 自动捕获间隔 |
| `SIGNATURE_COLS` | `FRAME_SIGNATURE_COLS` | 18 | 签名列数 |
| `SIGNATURE_ROWS` | `FRAME_SIGNATURE_ROWS` | 24 | 签名行数 |
| `DUPLICATE_AVG_DIFF` | `DUPLICATE_AVG_THRESHOLD` | 1.1 | 重复帧平均差阈值 |
| `DUPLICATE_MAX_DIFF` | `DUPLICATE_MAX_THRESHOLD` | 4 | 重复帧最大差阈值 |
| `min_overlap` (ColSample) | `STITCH_MIN_OVERLAP` | 100 | 最小重叠像素 |
| `accept_diff` (ColSample) | `STITCH_ACCEPT_DIFF` | 5.0 | 接受阈值 |
| `min_append` (ColSample) | `STITCH_MIN_APPEND` | 15 | 最小追加高度 |
| `approx_diff` | `STITCH_APPROX_DIFF` | 1.0 | 近似匹配阈值 |

---

## 6. 附录

### 6.1 wayscrollshot 关键算法伪代码

#### Column Sampling

```
function col_sampling(image):
    w = image.width
    h = image.height
    
    groups = [
        linspace(20, w/4, 3),      // 左列组
        linspace(w/2, 5w/8, 3),     // 中列组
        linspace(6w/8, 7w/8, 3),    // 右列组
    ]
    
    result = 二维数组 [h][3]
    
    for each group_idx, cols in groups:
        for y in 0..h:
            sum = 0
            count = 0
            for x in cols:
                if x < w:
                    gray = rgb_to_gray(image[x, y])
                    sum += gray
                    count += 1
            result[y][group_idx] = sum / count
    
    return result
```

#### Overlap Detection

```
function diff_overlap(prev_cols, curr_cols, predict, approx_diff, min_overlap):
    h1 = prev_cols.height
    h2 = curr_cols.height
    max_offset = h1 - min_overlap
    
    best = (0, MAX_FLOAT)
    approach_count = 0
    
    for offset in predict_offset_iter(max_offset, predict):
        diff = compute_col_diff(prev_cols, curr_cols, offset)
        
        if diff < best.diff:
            best = (offset, diff)
        
        if best.diff < approx_diff:
            approach_count += 1
            if approach_count > 10:
                return best
            if diff < approx_diff / 4:
                return best
    
    return best
```

### 6.2 mark-shot 交互流程图

```
启动 mark-shot
    │
    ▼
┌─────────────┐
│ 全屏冻结画面 │
└─────────────┘
    │
    ▼
┌─────────────┐     ┌─────────────┐
│  Selecting  │◄────│  调整选区    │
│  (拖拽选区)  │     │ (8方向手柄)  │
└─────────────┘     └─────────────┘
    │
    │ Enter / 双击确认
    ▼
┌─────────────┐     ┌─────────────┐
│   Editing   │◄────│  标注绘制    │
│  (工具栏+画布)│     │ (多种工具)   │
└─────────────┘     └─────────────┘
    │
    │ Ctrl+S / Ctrl+C / Pin
    ▼
┌─────────────┐
│   结束      │
└─────────────┘
```

---

*本文档由 wlsnap 开发团队维护，随项目进展持续更新。*
