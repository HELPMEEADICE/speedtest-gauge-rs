# Slint 1.18.1 + Rust 可行性探针（PROBE）

> ⚠️ 本文是阶段 A 的 API 探针记录，探针脚手架已在收尾时移除；文中提到的示例文件不再存在。

本文件是**实测结论**，不是文档摘抄。每一条都有可编译代码 + 实际渲染/运行输出作证据。
探针 crate 根：`C:\oss\Project\Rust\speedtest-rs\slint`

> 探针脚手架（探针 .slint、探针 example、截图工具、探针字体、`tools/probe_out/` 临时产物）
> **已在收尾时移除**：保留下来的 4 项证据（`raster/`、`slices/`、`needle_zoom_raster.png`、
> `needle_zoom_slices.png`）迁到 `comparison/evidence/`。本文只保留结论，行号引用均为历史记录。

---

## 0. 环境与版本

| 项 | 值 |
|---|---|
| `rustc -vV` host | `x86_64-pc-windows-gnu`（MSYS2 构建，`rustc 1.98.1 (48a229cea 2026-09-01)`，LLVM 22.1.8） |
| cargo | `cargo 1.98.1 (797e8a9bc 2026-08-05)` |
| 没有 rustup | 全流程未调用 rustup |
| `slint` | **1.18.1** |
| `slint-build` / `slint-macros` | 1.18.1 / 1.18.1 |
| `i-slint-core` / `-common` / `-compiler` / `-renderer-software` | 全部 1.18.1 |
| 字体/文本栈 | `fontique 0.11.1`, `parley 0.11.1`, `skrifa 0.44.0`, `swash 0.2.10` |
| 路径栈 | `lyon_path 1.0.19` / `lyon_geom 1.0.20` / `lyon_algorithms 1.0.22` / `lyon_extra 1.1.0` / `zeno 0.3.3` |
| `tiny-skia` | **0.11.4**（仅用作 PNG 编码；锁文件里另有 0.12.0 是其它依赖带入的） |

**构建耗时（注意口径）**：本 crate 的 `target/` 被同目录其它工作预热过，干净的冷启动数字拿不到。
实测：首次 `cargo build` 走到 `.slint` 编译报错 = **64.2 s**；`cargo build --release`（仍需编译
`i-slint-renderer-software`/`slint`/`slint-build`/`slint-macros` + 本 crate）= **32.4 s**；
全缓存后 `cargo build` = 0.23 s、`cargo build --release` = 0.24 s、探针 example 增量 = 1.8~3.0 s。

**产物体积**（探针可执行文件已随脚手架删除，数字仅存档）：release **15.73 MB**
（16,490,278 B）、debug 130.89 MB。

**Cargo.toml 关键点**：headless 探针必须 `default-features = false`，否则会拉 `backend-default`(winit) +
`renderer-femtovg`(OpenGL)。实测可用组合：

```toml
[dependencies]
slint = { version = "=1.18.1", default-features = false, features = [
    "std", "compat-1-2", "renderer-software", "software-renderer-path", "unstable-fontique-011",
] }
tiny-skia = "0.11.4"
[build-dependencies]
slint-build = "=1.18.1"
```

> `std` 必须有：它连带打开 `i-slint-renderer-software/std` → `systemfonts`（fontique/skrifa/swash），
> 没有它文字画不出来。

---

## 1. 结论总表

| # | 事实 | 结论 | 证据（文件:行 / 命令 / 输出片段） |
|---|---|---|---|
| A1 | feature 名 `renderer-software` | **可用** | `slint-1.18.1/Cargo.toml` `[features]` 有该项；本 crate 启用后编译通过 |
| A2 | feature 名 `renderer-femtovg` | **可用（未编译验证）** | 同名 Cargo.toml 有该项，但 headless 探针不启用（会拉 OpenGL/winit） |
| A3 | feature 名 `backend-winit` | **可用（未编译验证）** | 同上；名称确认存在 |
| A4 | feature `compat-1-2` | **名字不同（旧名）** | 真实存在的是 `compat-1-2` **和** `compat-1-18`，且 `compat-1-2 = ["compat-1-18", ...]`。新项目用 `compat-1-18` 更合适 |
| A5 | slint 默认 features | `std, backend-default, renderer-femtovg, renderer-software, accessibility, compat-1-2, system-tray` | `slint-1.18.1/Cargo.toml` `default = [...]` |
| A6 | build.rs 最小用法 | `slint_build::compile("ui/x.slint")` 和 `compile_with_config(path, CompilerConfiguration)` 都可用，返回 `Result<(), CompileError>`，`.unwrap()` 正常 | `build.rs:5,8`（两种都跑过，见下） |
| A7 | `include_modules!()` 位置 | 放在**要使用生成类型的 crate 根**（example 里就是 `原 examples/probe.rs（已归档）` 顶部）；宏体等价 `include!(env!("SLINT_INCLUDE_GENERATED"))`，路径由 build script 通过 `rustc-env` 下发，所以同一 package 的 example 也能用 | `原 examples/probe.rs（已归档）:11`；生成文件 `target/debug/build/slint-probe-*/out/probe.rs`（名字 = .slint 文件名 stem） |
| A8 | feature `unstable-fontique-011` / `software-renderer-path` | **可用** | 已启用并编译通过 |
| B1 | `Path.commands` 支持 M/L/A/Z | **可用** | `原 ui/probe.slint（已归档）:66,77,88,99` 四条路径全部渲染（截图 `probe_286.png`） |
| B2 | `A` 参数顺序 | `A rx ry x-axis-rotation large-arc-flag sweep-flag x y`（标准 SVG）；坐标支持空格与逗号；支持相对 `m`/`l` | `原 ui/probe.slint（已归档）:99` `commands: "m 10,10 l 12,0"` 渲染出绿色小刻度 |
| B3 | `A` 的 flag 语义 | 实测正确：同一端点对，`1 1` = 270° 弧、`0 0` = 余下 90° 弧 | 沿 r=76 圆周采样：`0-90/90-180/180-270` 象限为蓝(#4cc9f0) 45/45/45 个采样点，`270-360` 象限为橙(#f4a261) 45 个 |
| B4 | 描边属性名 | `stroke`、`stroke-width`(length)、**`stroke-line-cap`**、**`stroke-line-join`**、`stroke-miter-limit`、`fill`、`fill-rule`(nonzero\|evenodd) | 写成 `stroke-linecap`/`stroke-linejoin` 会报 `Unknown property`（首次构建的 CompileError 原文见 §5） |
| B5 | `fill` 默认 | 默认无填充（不设置 `fill` 时不填充）；显式 `fill: transparent` 等价 | `原 ui/probe.slint（已归档）:69` 等 |
| B6 | 270° 弧 stroke 渲染 | **可用** | `probe_286.png` 蓝色 270° 弧 + 橙色 90° 补弧 |
| C1 | 变换属性真名 | `transform-rotation`(angle)、`transform-scale-x`/`transform-scale-y`(percent)、**`transform-origin`(Point)** | `i-slint-compiler-1.18.1/builtin_elements.rs:980-983` |
| C2 | `transform-rotation-angle` / `transform-origin-x/y` / `rotation-angle` | **不可用**（编译报 Unknown property） | 见 §5 首次构建错误；`builtin_elements.rs:836,1048` 的 `rotation-angle` 只是**文档生成指令**，不是真属性 |
| C3 | 绕 (143,143) 旋转的正确写法 | 语法：`transform-origin: { x: 2px, y: 62px }; transform-rotation: root.pointer-angle * 1deg;`（origin 是元素**局部**坐标的 Point） | `原 ui/probe.slint（已归档）:126-127` 编译通过 |
| C4 | **软件渲染器是否应用 rotate/scale** | **不应用**（静默忽略） | `i-slint-renderer-software-1.18.1/lib.rs:3096` `fn rotate(...) { // TODO (#6068) }`、`:3100` `fn scale(...) { // TODO }`；实测 116.37° 与 0° 两张图里那条 transform 指针**仍然竖直**（竖直窄条 108..112×50..100 里有 122 个亮点），两个对照 Rectangle 也都没转 |
| C5 | 那么指针怎么转 | **烘焙几何**：Rust 算三角形 → 赋 `commands` 字符串 + 元素包围盒 | 尖端实测 (164,137)，r=60.37，角度 **116.57°**（目标 116.37°） |
| D1 | `@linear-gradient` 语法 | `@linear-gradient(<angle>deg, <color> <pos>%, ...)`；`deg` 后缀、`%` 位置、`rgba(r,g,b,a)`（a 为 0..1 **浮点**）、`transparent` 全可用 | `原 ui/probe.slint（已归档）:131,150,154,158` |
| D2 | 角度语义 | **CSS 同义**：`0deg` = stop0 在**下**方（向上走）；`90deg` = stop0 在**左**（向右走）；`180deg` = stop0 在**上**（向下走） | `bar1(0deg, transparent→white)` 顶(231,231,231)/底(35,39,43)；`bar3(90deg, red→blue)` 左(231,0,23)/右(47,0,207)。源码同源：`graphics/brush.rs:755 line_for_angle = (angle+90).to_radians()` |
| D3 | 能否用于 `Path.fill` | **可用**（SLint 字面量 与 Rust 构造的 brush 都可） | `原 ui/probe.slint（已归档）:131`；Rust 侧 `原 examples/probe.rs（已归档）` `Brush::LinearGradient(...)` 赋给 `needle-fill`，截图指针根部暗/尖端亮 |
| D4 | 渐变的坐标系 | 按**元素包围盒**铺（不是路径自身 bbox）→ 要让渐变贴合指针，元素尺寸必须等于指针包围盒 | `原 ui/probe.slint（已归档）:118-121` 烘焙指针的 w/h 由 Rust 算 |
| D5 | 指针渐变（上端 60% 白 → 下端透明） | `fill: @linear-gradient(180deg, rgba(255,255,255,0.6) 0%, transparent 100%);` + 元素 = 指针紧包围盒 | `原 ui/probe.slint（已归档）:131` 渲染正常 |
| E1 | `font-family` 逗号回退列表 | **不可用**：整个字符串被当成一个 family 名，找不到就退默认字体 | `"No Such Family Here, Segoe UI"` 墨迹 57×10 / 274 px；`"Segoe UI"` 墨迹 53×9 / 239 px → 不是同一字体 |
| E2 | `font-weight` 类型 | **int**：`font-weight: 700;` | `i-slint-core-1.18.1/items/text.rs` `pub font_weight: Property<i32>`；`原 ui/probe.slint（已归档）:210` 编译通过 |
| E3 | `letter-spacing` | **存在**（length）：`letter-spacing: 1px;` | `text.rs` `pub letter_spacing: Property<LogicalLength>`；`原 ui/probe.slint（已归档）:211` |
| E4 | 对齐/换行/溢出 | `horizontal-alignment`(start\|end\|center\|left\|right)、`vertical-alignment`(top\|bottom\|center)、`wrap`(no-wrap\|word-wrap\|char-wrap)、`overflow`(clip\|elide) | `原 ui/probe.slint（已归档）:206-213` |
| E5 | 文字视觉居中最佳写法 | **固定 width/height 容器 + Text 撑满 + 两个 alignment: center**；不要用 Text 直接 `x/y`（Text 尺寸由内容决定，无法自身居中） | 目标中心 (226,210)，实测墨迹中心 **(225.0, 209.5)**，偏差 (−1.0, −0.5) px（13px 字号；误差来自 letter-spacing 尾随间距与字形侧边距） |
| E6 | `slint::register_font_from_path` | **不存在**（1.18.1 无此自由函数），`register_font_from_memory` 同样没有 | 全 crate 源码搜索只命中 `i_slint_core::renderer::Renderer` 的 trait 方法 |
| E7 | 运行时注册字体的可用路径 | `slint::fontique_011::shared_collection()`（feature `unstable-fontique-011`）+ `collection.register_fonts(Blob::new(Arc<Vec<u8>>), None)` | `原 examples/probe.rs（已归档）` `[E] register_fonts -> 1 个 face`；注册版 vs 未注册版 E3 文字区域差 **505** 像素 → 生效 |
| F1 | `in property <image>` + `Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(..))` | **可用**，但 `clone_from_slice` 收的是**字节切片 `&[u8]`**（长度 w*h*4），传 `&[Rgba8Pixel]` 编译不过 | `原 examples/probe.rs（已归档）:stripe_frame()`；注释里记了编译错误原文 |
| F2 | `image-fit` 取值 | `fill` \| `contain` \| `cover` \| `preserve` | `i-slint-common-1.18.1` `enum ImageFit`；`原 ui/probe.slint（已归档）:257,263` 用了 `fill` 与 `contain` |
| F3 | N×N 像素图显示在 M×M 逻辑尺寸 | 按逻辑尺寸布局，物理像素 = M × DPR（**不是物理 1:1**） | 40 逻辑 px 的图：sf=1/2/4 下占 40/80/160 物理 px，条纹宽 6/11/22 px |
| F4 | `image-rendering: smooth` 是否真插值 | **无差别**：`pixelated` 与 `smooth` 输出**逐像素完全相同** | 40×40 区域 1600/1600 像素一致；亮度值只有 {0,255} 两档 |
| G1 | `slint::Timer` API | `Timer::default()` / `start(TimerMode::Repeated, Duration, cb)` / `stop()` / `restart()` 全部存在 | `原 examples/probe.rs（已归档）` `[G] Timer 16ms Repeated：200ms 后 ticks=11~12；stop 后 ticks 不涨；restart 后 +120ms → 18~19` |
| G2 | Timer 靠什么驱动 | headless 下必须手动泵 `slint::platform::update_timers_and_animations()` | `原 examples/probe.rs（已归档）:pump()`；不泵则 ticks 恒为 0 |
| G3 | `ModelRc`/`VecModel` 配 `in property <[Struct]>` | **可用**：`ModelRc::new(VecModel::from(vec![..]))`；`.slint` 侧 `rows.length` 可得 3 | `[G] rows.row_count()=3`、`[G] out property row-count=3`；注意 `VecModel` **不是 Clone**，要复用就传 `ModelRc`（Clone） |
| G4 | `ComponentHandle::window()` / `scale_factor()` | **可用** | `[G] window().scale_factor()=1`；`window().size()` 在未 `set_size` 时是 `{0,0}` |
| H1 | headless 无窗口渲染 | **可用** | 见 §2；三张 PNG 全部产出 |
| H2 | 渲染目标像素类型 | **`Rgba8Pixel` 不可用**（未实现 `TargetPixel`）；可用 `Rgb8Pixel` / `PremultipliedRgbaColor` / `Rgb565Pixel` / `Rgb565BigEndianPixel` | 首次编译错误；`i-slint-renderer-software-1.18.1/draw_functions.rs:861+` 的 impl 列表 |
| H3 | 设置 DPR | `set_const_scale_factor` 是内部 API（在 `WindowInner` 上）；公开途径只有 `window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor })` | `原 src/probe_shots.rs（已归档）:97`（注释记了 E0599 原文） |
| H4 | 三档缩放 | 286→286、286→572、286→1144，全部成功；非背景像素 17651 / 67876 / 266088（≈×3.85 / ×3.93） | `[H] probe_286/572/1144` 输出 + Pillow 统计 |
| H5 | PNG 编码 | 用 tiny-skia 自带 `Pixmap::encode_png()`（feature `png-format` 默认开），**不必**再加 `png`/`image` crate | `原 src/probe_shots.rs（已归档）:save_png()` |
| X1a | `viewbox-x/y/width/height` | **存在**，名称就是如此（width/height ≤ 0 时改用路径隐式包围盒） | `builtin_elements.rs` Path 定义；`原 ui/probe.slint（已归档）` ProbeX 两处 viewbox |
| X1b | 元素放大时 stroke 是否跟着放大 | **不放大**（≈non-scaling-stroke 语义）：几何半径按 viewbox 比例缩放，`stroke-width` 仍是逻辑 px | 同一 `commands`(r=130, stroke 23) 装进 100×100 与 180×180：实测几何半径 **45.5 / 81.8**（= 130×0.3497 / ×0.6294 ✓），而描边带宽 **22px / 23px**（若按 SVG 语义应为 8.04 / 14.48） |
| X1c | `A` 是否被正确处理成圆弧 | **是**，端点与圆心角都对 | `probe_x.png` 两条 270° 弧；探针 §B 的象限采样同样证明 |
| X2 | `font-size: 14px * root.k` | **可编译**，且 k=2 时与写死 `28px` **逐像素一致** | 墨迹 bbox 都是 32×22、墨迹像素 293，完全相同 |
| X3 | 父级 `transform-scale` 对文字的影响 | **不能用**（软件渲染器下完全无效） | `transform-scale-x/y: 400%` 的 14px 文字与未缩放 14px 文字**逐像素差异 0/1856**，墨迹高都是 10px；真 56px 对照墨迹高 42px |
| X4a | `for seg[i] in root.model-segments` 索引写法 | **可用**，`i` 是内建索引，`visible: i < root.visible-count` 生效 | `原 ui/probe.slint（已归档）:398-408` 编译 + `probe_gauge.png`（240 段连续环） |
| X4b | 只改 `visible-count` 会不会重建 repeater | **量级结论：不重建**（即便重建，成本也远小于整帧） | release / 286×286 / 240 个 Path，n=10 中位：全显重绘 **12.68 ms**、只改 visible-count **12.73 ms**、每次换整个 model **12.72 ms**（差异 < 0.1 ms，噪声内） |
| X4c | 240 个 Path 的渲染耗时 | release **≈12.7 ms/帧**，debug **≈164~176 ms/帧** | 同上；debug 明细 `[176.13, 176.90, ...]` |
| X5 | Rust 侧给 `Path.fill` 赋渐变 brush | **可用**，但 `LinearGradientBrush`/`GradientStop` **不在 `slint` crate 根**，要走 `slint::private_unstable_api::re_exports::{..}` | `原 examples/probe.rs（已归档）:20`；构造 `Brush::LinearGradient(LinearGradientBrush::new(angle, [GradientStop{color, position}]))` 成功后指针渐变渲染正常 |

---

## 2. H：headless 渲染管线（复用蓝图）

```rust
// 1) 平台：只需要实现 create_window_adapter（Platform 其它方法都有默认实现）
struct HeadlessPlatform { slot: WindowSlot }
impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        *self.slot.inner.borrow_mut() = Some(window.clone());
        Ok(window)
    }
}
slint::platform::set_platform(Box::new(HeadlessPlatform { slot: slot.clone() }))?; // 进程内只能一次

// 2) 每帧：新建组件 → 设 DPR → 设逻辑尺寸 → show → 强制重画 → 渲染进缓冲
window.window().dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: sf });
window.set_size(LogicalSize::new(286.0, 286.0));   // WindowSize 是 enum{Physical,Logical}
ui.show()?;
window.window().request_redraw();
let drew = window.draw_if_needed(|renderer| {
    let mut buffer = SharedPixelBuffer::<PremultipliedRgbaColor>::new(pw, ph);
    let _region = renderer.render(buffer.make_mut_slice(), buffer.width() as usize);
    out = Some(buffer);
});
```

要点/坑：

1. **渲染目标类型**：`Rgba8Pixel`（=`rgb::Rgba<u8>`）没有实现 `TargetPixel`，`render()` 直接编译不过。
   要 RGBA 就用 `PremultipliedRgbaColor`（字段 `red/green/blue/alpha`），它**是预乘 alpha**，
   写 PNG 前要反预乘（探针画布根背景不透明，实际是恒等变换，但代码里保留反预乘）。
2. **DPR**：`set_const_scale_factor()` 是内部 API。公开途径只有
   `window.dispatch_event(WindowEvent::ScaleFactorChanged{..})`，且必须在 `set_size` **之前**发。
   每个 shot 都新建组件（= 新 Window），所以不存在"缩放因子被锁死"的问题。
3. **`draw_if_needed` 的返回值**只表示"是否真的重画了"；先 `request_redraw()` 保证为 true。
4. **PNG 编码**：`tiny_skia::Pixmap::from_vec(bytes, IntSize::from_wh(w,h))` + `encode_png()`，
   省掉 `png`/`image` 依赖。
5. **视觉回归复用**：把 `shots::render()` 的 `Shot` 与基准 PNG 做逐像素差（Pillow 或
   Rust 侧直接比 `buffer.as_slice()`）。DPR 三档（1/2/4）分别存基准，就能同时锁住
   "布局是否随 DPR 正确缩放" 和 "抗锯齿差异"。240 个 Path 的场景单帧 ≈12.7 ms（release），
   做 CI 完全够用。

**产物**（`tools/probe_out/`）：

| 文件 | 尺寸 | 非背景像素 |
|---|---|---|
| `probe_286.png` | 286×286 | 17651 |
| `probe_572.png` | 572×572 | 67876 |
| `probe_1144.png` | 1144×1144 | 266088 |
| `probe_286_angle0.png` | 286×286 | 指针 0° 对照 |
| `probe_286_nofont.png` 等 | — | `PROBE_SKIP_FONT=1` 对照组（验证字体注册） |
| `probe_x.png` | 420×300 | X1/X2/X3 |
| `probe_gauge.png` | 286×286 | X4/X5（240 个 Path） |

---

## 3. 推荐写法（后续实现直接用）

### 3.1 量表弧 + 指针（软件渲染器下唯一能转的做法）

```slint
// 弧：元素尺寸 = viewbox 尺寸 ⇒ 1:1，stroke 宽度按逻辑 px 直接给
Path {
    x: 0; y: 0; width: 286px; height: 286px;
    viewbox-x: 0; viewbox-y: 0; viewbox-width: 286; viewbox-height: 286;
    commands: "M 143 13 A 130 130 0 1 1 13 143";   // 270°，起 12 点、顺时针到 9 点
    fill: transparent; stroke: #26273b; stroke-width: 23px; stroke-line-cap: round;
}

// 指针：几何由 Rust 按角度烘焙（软件渲染器忽略 transform-rotation）
Path {
    x: root.needle-x * 1px; y: root.needle-y * 1px;
    width: root.needle-w * 1px; height: root.needle-h * 1px;
    viewbox-x: 0; viewbox-y: 0;
    viewbox-width: root.needle-w; viewbox-height: root.needle-h;
    commands: root.needle-commands;              // "M 0,62 L 2,0 L 4,62 Z" 之类
    fill: root.needle-fill;                      // Brush::LinearGradient(...)，见 3.3
    stroke: #ffb347; stroke-width: 1px;
}
```

```rust
// 角度 0 = 正上方，正值顺时针；输出紧包围盒 + 局部坐标 commands
let (cx, cy, len, half) = (143.0f32, 143.0f32, 120.0f32, 4.0f32);
let (s, c) = angle_deg.to_radians().sin_cos();
let tip = (cx + s * len, cy - c * len);
let (px, py) = (c, s);
let a = (cx - px * half, cy - py * half);
let b = (cx + px * half, cy + py * half);
// …取三点 AABB(+1px pad) → 换算成局部坐标 → format!("M {:.3} {:.3} L …")
```

> **如果你改用 femtovg/skia 后端**，`transform-origin: { x, y }` + `transform-rotation: Ndeg`
> 语法是对的（编译通过），但本探针**无法**在 headless 下验证其渲染效果——
> 软件渲染器的 `rotate()`/`scale()` 是空实现（`lib.rs:3096/3100`，TODO #6068）。

### 3.2 文字精确居中

```slint
Rectangle {          // 容器：唯一负责"位置 + 尺寸"
    x: 180px; y: 200px; width: 92px; height: 20px;
    Text {
        x: 0; y: 0; width: 100%; height: 100%;
        text: "CENTER";
        horizontal-alignment: center;
        vertical-alignment: center;
        font-size: 13px; font-weight: 700; letter-spacing: 1px;
        font-family: "Segoe UI";     // 不要写逗号列表（无效）
    }
}
```

实测墨迹中心与容器中心偏差 (−1.0, −0.5) px。若要 0 误差：去掉 `letter-spacing`
（它会在最后一个字后面也加间距，导致整体左移半个间距），或用 `Text` 的
`width/height` 微调 +1px 补偿。

### 3.3 指针渐变（objectBoundingBox 纵向，上 60% 白 → 下全透明）

```slint
// 元素 = 指针紧包围盒，渐变按元素包围盒铺
fill: @linear-gradient(180deg, rgba(255, 255, 255, 0.6) 0%, transparent 100%);
```

Rust 侧等价（角度会跟着指针走）：

```rust
use slint::private_unstable_api::re_exports::{GradientStop, LinearGradientBrush};
let fill = Brush::LinearGradient(LinearGradientBrush::new(
    angle_deg,                                    // 与指针同向
    [ GradientStop { color: Color::from_argb_u8(0, 0, 0, 0), position: 0.0 },        // 根部透明
      GradientStop { color: Color::from_argb_u8(153, 255, 255, 255), position: 1.0 } // 尖端 60% 白
    ],
));
```

### 3.4 240 段标尺

```slint
export struct Seg { d: string, c: color, }
for seg[i] in root.model-segments : Path {
    x: 0; y: 0; width: 286px; height: 286px;
    viewbox-x: 0; viewbox-y: 0; viewbox-width: 286; viewbox-height: 286;
    commands: seg.d; stroke: seg.c; stroke-width: 3px; stroke-line-cap: round;
    fill: transparent;
    visible: i < root.visible-count;
}
```

改 `visible-count` 只脏化各 item 的 `visible`，实测开销与"什么都不改"同级（12.7 ms vs 12.68 ms），
可以放心用它做"只点亮已走过的段"。

---

## 4. 复现命令

```powershell
cd C:\oss\Project\Rust\speedtest-rs\slint
cargo build                      # Finished `dev` profile ... in 0.23s
cargo build --release            # Finished `release` profile ... in 0.24s
# （探针 example 已归档：原命令产出 3 档 PNG + 全部 [C]/[E]/[G]/[H]/[X4] 证据行）

# 字体注册对照组
# 字体注册对照组：$env:PROBE_SKIP_FONT='1' + 探针 example（已归档）
```

Pillow 统计（本次用的就是这些量）：

```python
from PIL import Image
BG = (11, 15, 20)
def nonbg(p):
    im = Image.open(p).convert("RGB"); b = im.tobytes()
    return sum(1 for i in range(0, len(b), 3)
               if (b[i], b[i+1], b[i+2]) != BG)
```

---

## 5. 遇到的坑与真实报错

1. **`.slint` 属性名全错第一版**（编译期就炸，报错很直白）：

   ```
   error: Unknown property stroke-linecap in Path          → 正确：stroke-line-cap
   error: Unknown property stroke-linejoin in Path         → 正确：stroke-line-join
   error: Unknown property transform-origin-x in Path      → 正确：transform-origin（Point）
   error: Unknown property transform-rotation-angle in Path→ 正确：transform-rotation
   error: Unknown property rotation-angle in Rectangle     → 该名字只是文档生成指令，无真属性
   ```

2. **`Path` 默认 `fit: contain` 会缩放路径**：viewbox 未定义时用"路径隐式包围盒"当 viewport，
   于是 `commands` 里的绝对坐标会被拉伸。**所有 Path 都显式写 viewbox**（= 元素尺寸）才能 1:1。

3. **`SharedPixelBuffer::clone_from_slice` 要字节切片**：

   ```
   error[E0308]: expected `&[u8]`, found `&Vec<rgb::Rgba<u8>>`
   note: associated function defined here  → i-slint-core/graphics/image.rs:141
   ```

4. **`Rgba8Pixel` 不能当渲染目标**：

   ```
   error[E0277]: the trait bound `rgb::formats::rgba::Rgba<u8>: TargetPixel` is not satisfied
   help: the following other types implement trait `TargetPixel`: Rgb8Pixel, PremultipliedRgbaColor, Rgb565Pixel, Rgb565BigEndianPixel
   ```

5. **`Window::set_const_scale_factor` 不可用**：

   ```
   error[E0599]: no method named `set_const_scale_factor` found for reference `&Window`
   → 它是 i_slint_core::window::WindowInner 的方法；公开途径是 dispatch_event(ScaleFactorChanged)
   ```

6. **`VecModel` 不是 Clone / 缺 `Model` trait**：

   ```
   error[E0599]: no method named `clone` found for struct `VecModel<T>`
   error[E0599]: no method named `row_count` found for struct `ModelRc<T>`  → use slint::Model;
   ```

7. **`slint::GradientStop` / `slint::LinearGradientBrush` 不在根**：

   ```
   error[E0432]: unresolved imports `slint::GradientStop`, `slint::LinearGradientBrush`
   → 用 slint::private_unstable_api::re_exports::{GradientStop, LinearGradientBrush}
   ```

8. **最大的坑（架构级）**：软件渲染器**静默忽略** `transform-rotation` 与 `transform-scale`。
   编译器不报任何警告，只是渲染结果不变。凡是准备用 headless 截图做视觉回归的方案，
   都不要在 `.slint` 里依赖元素旋转/缩放——要么烘焙几何（本探针做法），要么换 GPU 后端截图。
