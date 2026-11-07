# speed-gauge-slint

**一句话定位**：把 [speedtest-gauge-svg](https://github.com/HELPMEEADICE/speedtest-gauge-svg)
（TypeScript + SVG 的动态测速仪表盘）移植成**可复用的原生 Slint 组件**——纯矢量、无 WebView、
无 async runtime，286 uu 坐标系，一个 `GaugeHandle` 就能驱动一个仪表实例。

> 原始实现：[`https://github.com/HELPMEEADICE/speedtest-gauge-svg`](https://github.com/HELPMEEADICE/speedtest-gauge-svg)
> **许可**：本移植沿用原项目的 **WTFPL v2**（全文见 [`LICENSE`](LICENSE)，第三方资产声明见
> [`NOTICE.md`](NOTICE.md)）。随包的 Montserrat 字体另有授权（SIL OFL 1.1，见
> `assets/fonts/OFL.txt`）；Gauge Mono 是 Ookla 专有字体，**不随本包分发**。

---

## 1. 特性清单

| 能力 | 说明 |
|---|---|
| **四组角向渐变** | `dl-multi` / `ul-multi` / `dl-single` / `ul-single`，每组 28 条停止点（沿弧中径从 speedtest.net 采样），240 段彩弧拼出无接缝角向渐变 |
| **9 / 10 档非线性刻度** | 标准 9 档（0/5/10/50/100/250/500/750/1000）；`auto` + 大值时自动切 10 档（…2500/5000/10000），档内角度线性插值 |
| **tween / spring 双模型** | 0.2 s `easeOut` 追踪 + 0.5 s `easeInOut` 归位；或 ω=21.4 rad/s、ζ=0.92 的临界阻尼弹簧 |
| **微颤（wobble）** | 振幅跟随指针速度（6.5 Hz，上限 1.1°，增益 0.0032°/s） |
| **完整阶段机** | `idle → ping → download → upload → done`，切阶段重算渐变变体与强调色，`upload`/`idle` 触发归零 tween |
| **多实例** | 一个进程里 N 个 `SpeedGauge` + N 个 `GaugeHandle`，状态互不影响（Demo 底部的小仪表就是第二实例） |
| **HiDPI** | 全部几何按 `k = 逻辑边长 / 286` 缩放；指针光栅画布按 `170 × k × DPR` 定尺寸 |
| **无 WebView / 无 GPU 也可跑** | 组件是原生矢量 + 可选光栅化；`renderer-software` 后端下可无窗口渲染（视觉回归用） |
| **可复用组件** | 组件本身不碰数据源、不碰窗口：`SpeedGauge::new()` → `GaugeHandle::attach()` → 交事件循环 |

---

## 2. 快速开始

```powershell
# 1) 构建（库 + 全部 example）
cargo build --release
# 预期：`Finished \`release\` profile [optimized] target(s) in …`
# 会看到一条无害的 deprecated 警告：`Exported component 'SpeedGauge' doesn't inherit Window.`

# 2) 跑测试（单元 + 集成 + 黄金向量）
cargo test
# 预期：`cargo test --all-targets` = 103 passed / 0 failed

# 3) 完整 Demo（真窗口：左主仪表 + 右控制面板 + 底部第二实例）
cargo run --release --example demo
# 预期控制台：
#   [demo] window=1280x800 scale_factor=1.00
#   [demo] 面板：Run Demo / Reset / Snap To / Speed 滑块 / Stage / Connection / …
#   关闭窗口后：`[demo] 退出：main angle=…° number=… timer_running=…`

# 4) 最小集成示例（单实例，2 秒后自动关闭）
cargo run --release --example minimal
# 预期：
#   [minimal] step 1: stage=Download kbps=120000 -> angle=-135.000° number=120.00
#   …
#   [minimal] 结束：angle=116.364° number=862.00 timer_running=false

# 5) 无窗口截图 / 视觉回归（不需要显示器）
cargo run --release --example shots
# 预期：8 个状态图写到 comparison/render/*.png，并打印
#   == 单帧耗时（286×286 @ DPR 1 …）==  渲染 draw_if_needed 中位 12.50ms
#   == 单帧耗时（572×572 @ DPR 2 …）==  渲染 draw_if_needed 中位 46.82ms
#   收敛于 31 帧泵送后：angle=116.379° settled=true timer_running=false
```

无窗口跑完整 Demo（出 `comparison/render/demo-ui.png`）：

```powershell
$env:SPEED_GAUGE_HEADLESS=1; $env:SPEED_GAUGE_STATIC=862; cargo run --release --example demo
```

性能数字与采样命令见 [`docs/performance.md`](docs/performance.md)。

### 2.1 产物体积与瘦身（重要）

未加优化 profile 时三个 example 的 release 产物都是 **28–29 MiB**——其中库本体（`.rlib`）只有
**3.63 MiB**，其余全是 Slint 运行时 + femtovg/OpenGL + winit + tiny-skia。
如果你的应用在意体积，在自己的 `Cargo.toml` 里加：

```toml
[profile.release]
strip = "symbols"   # 去掉符号表
lto = "thin"        # 跨 crate 内联
codegen-units = 1   # 进一步优化体积/性能，代价是编译变慢
```

**实测**：只加 `strip = "symbols"`（本机 `RUSTFLAGS="-C strip=symbols"`），
`minimal.exe` 从 **28.17 MiB 降到 21.24 MiB（−24.6%）**；剩余体积几乎全是 Slint 运行时 + GL 后端，
不是应用代码。再往下压要靠 `lto = "thin"` + `codegen-units = 1`（本项目未实测这两个组合的具体数字）。

本项目**有意不把这些写进仓库的 `Cargo.toml`**：`docs/performance.md` 里记录的体积是默认 profile 下的，
写死 profile 会让那些数字不可复现。想复核就按默认 profile 构建。

---

## 3. 集成到别人的 Rust + Slint 项目

### 3.1 `Cargo.toml`

```toml
[dependencies]
# 与组件库用同一个 Slint 版本（组件库把 slint 锁在 =1.18.1，版本不一致会导致
# 生成的组件类型不兼容）
slint = { version = "=1.18.1", default-features = false, features = ["std", "compat-1-2"] }

# 只嵌入组件、不需要桌面窗口与内置字体时，关掉默认 feature 只留需要的：
speed-gauge-slint = { git = "https://github.com/HELPMEEADICE/speedtest-gauge-slint",
                      default-features = false,
                      features = ["desktop", "bundled-fonts"] }
```

- `default-features = false` + `["desktop"]`：宿主自己开窗口，不需要截图工具与内置字体。
- 想做**纯无窗口**（服务端出图 / CI 视觉回归）：`features = ["screenshots", "bundled-fonts"]`，
  不要 `desktop`（就不会拉 winit / femtovg）。
- 不想要指针光栅化（省 0.88 MiB 画布，代价是接缝）：去掉 `needle-raster`。

### 3.2 最小代码（已验证可编译，见 §3.4）

```rust
use slint::ComponentHandle;
use speed_gauge_slint::{GaugeHandle, GaugeStage, SpeedGauge};

fn main() -> Result<(), slint::PlatformError> {
    // 1) 建组件实例（Slint 组件就是一个普通 Rust 值）
    let ui = SpeedGauge::new()?;
    // 2) 把控制器挂上去（一个实例一个 handle）
    let gauge = GaugeHandle::attach(&ui);
    // 3) 喂数据：改 stage、改速度（kbps）
    gauge.set_stage(GaugeStage::Download);
    gauge.set_speed_kbps(862_000.0);
    // 4) 交回事件循环（动画泵是 handle 内部的 Timer，宿主不用管）
    ui.run()
}
```

### 3.3 想在**自己的 `.slint` 布局里**放这个仪表

```slint
// 路径指向 crate 里的组件文件（vendored 到你的项目时改成相对路径）
import { SpeedGauge, GaugeStage } from "path/to/speed-gauge.slint";

export component MyPanel inherits Window {
    SpeedGauge {
        width: 286px;
        height: 286px;
        stage: GaugeStage.download;
        speed-kbps: 862000;
    }
}
```

两条路二选一：

1. **Rust 侧建实例**（§3.2）：`SpeedGauge::new()` 直接来自库，**不需要**你的 `build.rs` 编译
   `speed-gauge.slint`。要让它在你的布局里参与排版，用 `ComponentContainer` +
   `component-factory`（见 `ui/demo.slint` 与 `examples/demo.rs`；该元素属 Slint
   **experimental**，需在 `build.rs` 里置位 `SLINT_ENABLE_EXPERIMENTAL_FEATURES`）。
2. **`.slint` 侧声明**（本节）：需要你的 `build.rs` 跑
   `slint_build::compile("path/to/your-panel.slint")`，再由 Rust 侧
   `GaugeHandle::attach(&panel_ui.get_the_gauge())` 拿到实例——注意这时 `SpeedGauge` 是**你的 crate**
   生成的类型（与库里的同名类型不是同一个类型，这正是 Demo 用中性 `int/bool` 传状态的原因）。

### 3.4 这段代码怎么验证的

上面 §3.2 的片段**逐字**放进 `examples/_readme_check.rs`，用
`cargo build --release --example _readme_check` 编译通过（之后删除该临时文件）。
关键点：`ui.run()` 需要 `use slint::ComponentHandle;` 在作用域里。

---

## 4. API 表

### 4.1 `GaugeHandle`（`speed_gauge_slint::GaugeHandle`）

| 方法 | 签名 | 语义 | 触发动画？ |
|---|---|---|---|
| `attach` | `fn attach(ui: &SpeedGauge) -> Self` | 绑定组件；立刻推一次初始状态（刻度表/彩弧/颜色/指针），并读入 attach 前写好的 `stage`/`speed-kbps`/主题 | 否（预置 `speed-kbps > 0` 时按无动画定位） |
| `set_speed_kbps` | `fn set_speed_kbps(&self, kbps: f64)` | 推一个采样值：指针按 `motion-mode` 追踪，数字立刻更新 | **是**（tween/spring）；`animation-enabled=false` 时直接定位 |
| `set_stage` | `fn set_stage(&self, stage: GaugeStage)` | 切阶段：重算渐变变体与强调色；`upload`/`idle` 触发归零 | **是**（归零 500 ms `easeInOut`） |
| `set_theme` | `fn set_theme(&self, theme: GaugeTheme)` | 换主题色与指针渐变，不重置指针 | 否（只重绘） |
| `set_connection_mode` | `fn set_connection_mode(&self, m: ConnectionMode)` | `multi`/`single`：换 4 组渐变之一 + 强调色，不动指针/数字 | 否（只重绘） |
| `set_scale_mode` | `fn set_scale_mode(&self, m: ScaleMode)` | `fixed`/`auto`；`auto` 且 > 1000 Mbps 时切 10 档 | 否（重建刻度后重绘） |
| `set_motion_mode` | `fn set_motion_mode(&self, m: MotionMode)` | 切换 tween / spring 模型 | 只在已有动画在跑时继续泵 |
| `set_animation_enabled` | `fn set_animation_enabled(&self, on: bool)` | `false` 时所有 `set_speed_kbps` 退化为直接定位，并停表 | 关掉会**立即定格**并停表 |
| `set_wobble_enabled` | `fn set_wobble_enabled(&self, on: bool)` | 开关微颤（等价把 `wobbleMax` 设 0） | 否 |
| `snap_to` | `fn snap_to(&self, kbps: f64)` | 无动画精确定位（截图/冻结用），会先取消未完成的归位 tween，然后停表 | **否**（有意偏离，见 `docs/module-map.md` §2） |
| `reset` | `fn reset(&self)` | 500 ms `easeInOut` 归零（不改 stage / labels） | **是** |
| `pause` | `fn pause(&self)` | 暂停动画泵，保留当前视图 | 停表 |
| `resume` | `fn resume(&self)` | 恢复动画泵（只在还有动画要跑时才真正启动） | 可能启表 |
| `stop` | `fn stop(&self)` | 彻底停泵；之后 `set_*` 不会自动重启（直到 `resume`） | 停表 |
| `step` | `fn step(&self, dt_seconds: f64)` | 手动推进一帧（确定性测试/截图） | 由调用方节拍决定 |
| `tick_now` | `fn tick_now(&self)` | 用真实时钟推进一帧（内部 `Timer` 就走这条） | 同上 |
| `angle` | `fn angle(&self) -> f64` | 当前绘制角（含微颤，度） | 只读 |
| `number_text` | `fn number_text(&self) -> String` | 当前读数文本（`formatSpeed` 结果） | 只读 |
| `lit_count` | `fn lit_count(&self) -> usize` | 当前被点亮的刻度数 | 只读 |
| `ring_visible_count` | `fn ring_visible_count(&self) -> usize` | 当前完整画出的彩弧段数（≤ 240） | 只读 |
| `is_settled` | `fn is_settled(&self) -> bool` | 是否已收敛（`isSettled()` 且无归位 tween） | 只读 |
| `timer_running` | `fn timer_running(&self) -> bool` | 动画定时器是否在跑（验收用：收敛后必须 `false`） | 只读 |
| `needle_raster_resizes` | `fn needle_raster_resizes(&self) -> usize` | 光栅画布**重建**次数（feature `needle-raster`）；稳态不增 = 零稳态分配 | 只读 |

### 4.2 Slint 组件属性（`SpeedGauge`，`ui/speed-gauge.slint`）

**宿主可写的意图**（`in-out`，控制器读它）：

| 属性 | 类型 | 默认 | 含义 |
|---|---|---|---|
| `speed-kbps` | `float` | `0` | 目标速度（kbps）；控制器据此算角度与数字 |
| `stage` | `GaugeStage` | `idle` | 阶段，决定渐变变体与强调色 |
| `theme` | `GaugeTheme` | `dark` | 主题 |
| `connection-mode` | `ConnectionMode` | `multi` | 多连接 / 单连接 |
| `scale-mode` | `ScaleMode` | `fixed` | 9 档 / 自动 10 档 |
| `motion-mode` | `MotionMode` | `spring` | 弹簧 / tween |
| `animation-enabled` | `bool` | `true` | 关掉后所有更新都直接定位 |
| `wobble-enabled` | `bool` | `true` | 微颤开关 |

**控制器写的视图态**（`in`，宿主不要直接写）：`display-angle`、`number-text`、`lit-count`、
`ring-visible-count`、`ring-boundary-commands`、`ring-boundary-color`、`ring-segments`、
`labels`、`needle-commands`、`needle-fill`、`needle-image`、`needle-slices`、`track-color`、
`label-on-color`、`label-off-color`、`number-color`、`unit-color`、`accent-color`、
`number-font-family`、`label-font-family`、`number-dy`、`label-dy`、`unit-row-dx`。
（后三个是排版微调：`label-dy = -1.375` 用来抵消 Slint 行盒模型与原 SVG
`dominant-baseline` 的差，见 `docs/known-deviations.md` ②。）

### 4.3 枚举取值

| 枚举 | 取值 |
|---|---|
| `GaugeStage` | `idle` / `ping` / `download` / `upload` / `done` |
| `ConnectionMode` | `multi` / `single` |
| `ScaleMode` | `fixed` / `auto` |
| `MotionMode` | `tween` / `spring` |
| `GaugeTheme` | `dark` / `light` |

数据层（`src/model.rs`）有同名但**是另一套类型**的枚举：`Stage::{Idle,Ping,Download,Upload,Done}`、
`ConnectionMode::{Multi,Single}`、`ScaleMode::{Fixed,Auto}`、`MotionMode::{Spring,Tween}`、
`Theme::{Dark,Light}`，都实现了 `as_str()` / `Display`（小写串与原实现一致）。
两套类型之间的映射只发生在宿主侧（Demo 里是 `examples/demo.rs` 的 `stage_from_idx` /
`stage_to_idx` / `to_gauge_stage` / `theme_to_idx` 这类小函数）。

---

## 5. 数据接入

三层抽象都在 `src/data_source.rs` / `src/demo_source.rs`，**同步**、无 async runtime：
source 在引擎线程 push，宿主在 UI 线程 poll/tick。生产级接入（真实 IO、背压、错误处理）
见 [`docs/real-backend.md`](docs/real-backend.md)。

### 5.1 `ManualSource`——"宿主推什么就是什么"

```rust
use speed_gauge_slint::data_source::{ManualSource, SpeedSource};
use speed_gauge_slint::model::Stage;

let mut source = ManualSource::new();
source.start(Box::new(move |sample| {
    // 回调在 push 的线程上跑：直接喂 handle
    gauge.set_stage(stage_of(sample.stage));
    gauge.set_speed_kbps(sample.kbps);
}));
source.push_download(862_000.0);   // 862 Mbps
source.push(341_000.0, Stage::Upload);
```

### 5.2 `ChannelSource`——跨线程（真正的"数据线程 → UI 线程"缝）

```rust
use speed_gauge_slint::data_source::{channel, SpeedSource};
use speed_gauge_slint::model::Stage;

let (tx, mut source) = channel();          // tx 可 Send 到任意线程
std::thread::spawn(move || {
    loop {
        // …从 socket / curl 解析出 kbps…
        if !tx.send(862_000.0, Stage::Download) { break; }   // 接收端没了就退出
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
});
source.start(Box::new(move |s| { gauge.set_stage(stage_of(s.stage)); gauge.set_speed_kbps(s.kbps); }));
// UI 线程的定时器里：
let n = source.poll();                      // 把队列里的样本排空并回调，返回条数
```

### 5.3 `DemoSource`——确定性假数据（Demo 用的就是它）

```rust
use speed_gauge_slint::demo_source::{DemoOptions, DemoSource};
use speed_gauge_slint::data_source::SpeedSource;

let mut source = DemoSource::new(DemoOptions::default());  // 862/341 Mbps、seed 20260101、hold 4200ms
source.set_listener(Box::new(move |s| { gauge.set_stage(stage_of(s.stage)); gauge.set_speed_kbps(s.kbps); }));
source.start(Box::new(|_| {}));
// 100 ms 心跳里：
source.tick(100.0);        // 推进 100 ms，返回本次产出的样本数
if !source.is_running() { /* 一轮跑完：Idle→Ping→Download→Upload→Done */ }
```

`DemoSource` 与 `pipeline.ts` 的 `mulberry32` 逐位等价，`tests/golden/demo_samples.json` 里
116 个样本是它的黄金向量。

---

## 6. 几何 / 颜色 / 动画常量

全部来自 `src/geometry.rs`（`GEO`）、`src/model.rs`、`src/animation.rs`、`src/gradient_data.rs`。
单位是 SVG 用户单位（uu），坐标系 `viewBox 0 0 286 286`。

| 常量 | 值 | 出处 |
|---|---|---|
| 容器边长 / 圆心 | `286` / `(143, 143)` | `geometry.rs` `GEO.size/center` |
| 弧中心半径 / 描边宽 | `130` / `23`（sweep 裁剪半径 `141.5`） | `GEO.radius/stroke_width`、`SWEEP_RADIUS` |
| 起止角 | `-135° … +135°`（0° 朝上、顺时针为正，跨度 270°） | `GEO.start_angle/end_angle` |
| 刻度文字半径 / 字号 / 字重 | `97` / `14` / `700` | `GEO.label_radius/label_font_size` |
| 指针 根部 / 尖端 / 长度 | `20` / `10` / `85`（元素盒 170×170） | `GEO.needle_*`、`NEEDLE_BOX` |
| 数字 字号 / 字重 / 行高 | `36` / `500` / `40` | `GEO.number_size/…` |
| 单位行 字号 / 行高 / 图标 | `14` / `20` / `16`（间距 `4`） | `GEO.unit_*` |
| 文字块 底边 / 单位行中心 / 数字中心 | `270` / `260` / `226` | `TEXT_BLOCK_*` |
| 轨道色 | `#26273b` | `THEME_DARK.track` |
| 强调色（多连接） | download `#00c5bd`、upload `#b45aed`（实测 P3→sRGB） | `ACCENT_*` |
| 强调色（单连接） | downloadSingle `#e56b4f`、uploadSingle `#7cdb2b`（未经像素校准） | `ACCENT_*_SINGLE` |
| 渐变端点色（4 组） | dl-multi `#00b5fe → #7bfe71`；ul-multi `#7553d0 → #fe70a8`；dl-single `#ff6b7b → #ffb96b`；ul-single `#70ff9f → #ceff70` | `gradient_data.rs` |
| 彩弧分段 | `240` 段，每段 `+0.28°` overlap（末段被 `min(135°)` 截断） | `gradient.rs` `DEFAULT_SEGMENTS/OVERLAP_DEG` |
| tween 追踪 / 归位 | `200 ms` `easeOut` / `500 ms` `easeInOut` | `model.rs` `NEEDLE_TRACKING/NEEDLE_RETURN` |
| spring | `ω = 21.4 rad/s`、`ζ = 0.92` | `animation.rs` `NeedleConfig::default` |
| 微颤 | `6.5 Hz`、上限 `1.1°`、增益 `0.0032 °/s` | 同上 |
| 刻度入场 | 逐条 `0.08 s` 错峰 + `0.08 s` 淡入 | `model.rs` `LABEL_STAGGER/LABEL_FADE_IN` |

---

## 7. 与原始 SVG 实现的对应关系

`tokens.ts → model.rs`、`scale.ts → scale.rs`、`geometry.ts → geometry.rs`、
`gradient-stops.ts → gradient_data.rs + gradient.rs`、`animator.ts → animation.rs`、
`gauge.ts → renderer.rs + needle_raster.rs + ui/speed-gauge.slint`、
`pipeline.ts → data_source.rs + demo_source.rs`、`main.ts → ui/demo.slint + examples/demo.rs`、
`styles.css → docs/font-metrics.md` 的逐条对照（含"逐行等价 / 语义等价 / 有意偏离 / 新增扩展"
四类标注与出处）见 [`docs/module-map.md`](docs/module-map.md)。

设计契约（为什么某些东西必须那么写）见 [`docs/port-spec.md`](docs/port-spec.md)，
像素级目标见 [`docs/visual-targets.md`](docs/visual-targets.md)。

---

## 8. 已知偏差

完整清单（现象 → 量化影响 → 根因机制 → 已尝试的修复与结果 → 是否可接受）见
[`docs/known-deviations.md`](docs/known-deviations.md)。

**7 状态统计结论（一句话）**：7 个冻结状态里可量化的 6 个亮弧终点偏差都 ≤ **0.14°**
（low +0.10 / mid −0.06 / high +0.03 / upload −0.14 / extended −0.05；`idle` 无亮弧、
`done` 与 `high` 逐字节相同），全部落在抗锯齿阈值内，所以验收线定为 `|Δ| ≤ 0.2°`
（`docs/visual-targets.md` §6–§7）。

其它你可能马上会注意到的：字体光栅器差异（FreeType/Chromium vs skrifa/Slint）、
`Mbps` advance 本机比 Chromium 宽 1.25 uu、指针由"32 片切片"改光栅化后的四轮迭代记录。

---

## 9. 许可与第三方资产

| 资产 | 许可 | 是否随包 |
|---|---|---|
| 本移植的代码（`src/**`、`ui/**`、`examples/**`） | **WTFPL v2**（全文 `LICENSE`） | 是 |
| 原始实现 [speedtest-gauge-svg](https://github.com/HELPMEEADICE/speedtest-gauge-svg) | WTFPL v2（原创作者 **HELPMEEADICE**） | — |
| **Montserrat**（`assets/fonts/montserrat.ttf`） | **SIL Open Font License 1.1**（`assets/fonts/OFL.txt`） | 是 |
| **Gauge Mono**（`.local-fonts/gaugemono.ttf`） | **Ookla 专有字体** | **否**——仅本机存在时生效；缺失时回退系统等宽字体，数字与 `Mbps` 的墨迹宽度约 **+42.6%** |
| **Slint**（运行时/编译器） | **GPLv3 或商业双许可** | 否（依赖）——**使用者需自行遵守 Slint 的许可**（闭源分发需购买商业授权） |

原创作者与移植说明、以及上述第三方声明的完整版见 [`NOTICE.md`](NOTICE.md)。

---

## 10. feature flags

`default = ["desktop", "bundled-fonts", "screenshots", "needle-raster"]`

| feature | 默认 | 拉入什么 | 关掉的影响 |
|---|---|---|---|
| `desktop` | ✅ | `slint/backend-winit` + `slint/renderer-femtovg` | 没有窗口后端：`ui.run()` / `show()` 不可用，只剩无窗口软件渲染（截图 / CI）。库本体仍可编译 |
| `bundled-fonts` | ✅ | `slint/unstable-fontique-011` + 内联 Montserrat / Gauge Mono | 运行时字体注册失效 → 回退系统字体，数字与刻度宽度整体走样（见 `docs/known-deviations.md` ①②） |
| `screenshots` | ✅ | `tiny-skia` | `shots` 模块与 `examples/shots.rs` 不参与编译 → 失去无窗口 PNG 视觉回归能力 |
| `needle-raster` | ✅ | `tiny-skia` | 指针退回 **32 片梯形平色路径**：没有 0.88 MiB 光栅画布常驻，但会出现接缝（迭代记录见 `docs/known-deviations.md` ⑥） |

> `screenshots` 与 `needle-raster` 都依赖 `tiny-skia`，两者同时关掉时该可选依赖完全不参与编译。

---

## 11. 目录速览

```
slint/
├─ ui/speed-gauge.slint      组件本体（286 uu 画布，纯矢量）
├─ ui/demo.slint             Demo 窗口（只用组件公开能力）
├─ src/                      数值层（model/scale/geometry/gradient/animation）
│                            视图桥接（renderer/gauge/needle_raster）
│                            数据层（data_source/demo_source）、自证工具（shots）
├─ examples/                 demo / minimal / shots / real_backend
├─ tests/                    单元 + 集成 + 黄金向量（103 个）
├─ docs/                     port-spec / visual-targets / real-backend / demo /
│                            font-metrics / module-map / known-deviations / performance
├─ comparison/               参考图度量（ref-metrics）、本地渲染与证据图
└─ assets/fonts/             Montserrat（OFL 1.1）+ 许可文件
```
