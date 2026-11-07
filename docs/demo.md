# 交互 Demo（`ui/demo.slint` + `examples/demo.rs`）

把 `SpeedGauge` / `GaugeHandle` / `DemoSource` 完全当**外部库**用的可运行 Demo：窗口尺寸
1280×800，左侧主仪表（572×572，随窗口在 440…640 之间等比缩放、恒正方形），右侧 320 宽控制
面板，面板底部另有一个 ≈0.35× 的**独立实例**（固定 Single + Auto + 4200 Mbps）。

## 运行

```bash
# 真窗口（winit + femtovg）。关窗或 Ctrl+C 退出；SPEED_GAUGE_EXIT_MS 可让它在 N ms 后自动退出。
cargo run --release --example demo

# 无窗口：不 run()，手动泵帧后用软件渲染器出图，然后退出（无显示器环境自证 / 文档用图）
SPEED_GAUGE_HEADLESS=1 cargo run --release --example demo
#   → comparison/render/demo-ui.png（1280×800，DPR 1）

# 无窗口：把面板上每个控件各点一遍，打印 ✓/✗（控件联动自测）
SPEED_GAUGE_HEADLESS=1 SPEED_GAUGE_SELFTEST=1 cargo run --release --example demo
```

启动时必打一行：`[demo] window=1280x777 scale_factor=1.00`（真窗口，物理像素 / DPR）。

## 控件 ↔ handle API

| 面板控件 | 回调 | Rust 侧动作 |
|---|---|---|
| `Run Demo` / `Stop Demo` | `run-clicked` | `DemoSource::new` + `set_listener/set_phase_listener`；listener 里 `set_stage` + `set_speed_kbps`；`slint::Timer(100 ms)` 里 `tick(100.0)`；跑完自动停 |
| `Reset` | `reset-clicked` | `stop_demo()` → `set_stage(Idle)` → `reset()`（500 ms easeInOut 归位） |
| `Snap To`（输入框，Mbps） | `snap-clicked` | `snap_to(mbps * 1000.0)`（即时精确）+ 同步滑块/读数 |
| `Speed` 滑块（0–10000 Mbps） | `speed-picked` | `set_speed_kbps(mbps * 1000.0)`；**不改 stage**（manual） |
| `Stage`：Idle/Ping/Down/Up/Done | `stage-picked` | `set_stage(...)` |
| `Connection`：Multi/Single | `connection-picked` | `set_connection_mode(...)` |
| `Scale`：Fixed/Auto | `scale-picked` | `set_scale_mode(...)` |
| `Theme`：Dark/Light | `theme-picked` | 主仪表 + 第二仪表都 `set_theme(...)`；面板底色由 `.slint` 的 `theme-idx` 联动 |
| `Motion`：Spring/Tween | `motion-picked` | `set_motion_mode(...)` |
| `Wobble`（开关） | `wobble-picked` | `set_wobble_enabled(bool)` |
| `Animation`（开关） | `animation-picked` | `set_animation_enabled(bool)` |
| `Pause` / `Resume` | `pause-clicked` | `pause()` / `resume()` |
| 只读状态区 | — | 每 100 ms 读 `angle()` / `number_text()` / `lit_count()` / `ring_visible_count()` / `timer_running()` 格式化写回 |

面板状态一律用**中性类型**（`int`/`bool`/`string`）：同一个 `.slint` 被 slint-build 独立编译两次
会产生两个不同的 Rust 枚举类型，枚举映射只发生在 `examples/demo.rs` 一处
（`stage_from_idx` / `theme_to_idx` / …）。

## 两个仪表怎么进布局的

`SpeedGauge` 的根是 `Rectangle`，而 Slint 的属性类型里没有"组件实例"类型，所以宿主无法从
`.slint` 里拿嵌套实例的把手。这里用官方接法：`ui/demo.slint` 放两个 `ComponentContainer`，
Rust 侧用 `slint::ComponentFactory` 在闭包里 `SpeedGauge::new()` 并把实例存进 `Shared`，
随后 `GaugeHandle::attach` 它。`ComponentContainer` 是 experimental 元素，`build.rs` 在编译
`ui/demo.slint` 时临时置位 `SLINT_ENABLE_EXPERIMENTAL_FEATURES`（编译完立刻撤回，`cargo build`
不依赖外部环境变量）。

## 参数（URL 参数的 CLI/环境变量等价物）

| 参考项目 `?` 参数 | 本 Demo |
|---|---|
| `?static=<mbps>` | `SPEED_GAUGE_STATIC=<mbps>`（冻结读数，先 stage 再 snap） |
| `?stage=<idle\|ping\|download\|upload\|done>` | `SPEED_GAUGE_STAGE=<...>` |
| `?theme=light` | `SPEED_GAUGE_THEME=light` |
| `?mode=single` | `SPEED_GAUGE_MODE=single` |
| `?scale=auto` | `SPEED_GAUGE_SCALE=auto` |
| `?seed=<u32>` | `SPEED_GAUGE_SEED=<u32>`（默认 20260101） |
| `?autoplay=1` | `SPEED_GAUGE_AUTOPLAY=1`（真窗口起来就自动跑一轮） |
| （无） | `SPEED_GAUGE_HEADLESS=1` 无窗口出图；`SPEED_GAUGE_SELFTEST=1` 控件联动自测；`SPEED_GAUGE_EXIT_MS=<ms>` 真窗口到点自动退出 |

截图：`SPEED_GAUGE_HEADLESS=1 [SPEED_GAUGE_STATIC=862 SPEED_GAUGE_STAGE=download …]
cargo run --release --example demo` → `comparison/render/demo-ui.png`（1280×800，DPR 1）。
`shots::render` 只接受正方形逻辑边长，所以示例里用它的公开积木（`WindowSlot` /
`MinimalSoftwareWindow` / `shots::Shot` / `shots::save_png`）拼了一条矩形路径。

## 换成真实数据源

把 `start_demo()` 里的 `DemoSource` 换成你的 `ChannelSource`（引擎线程 `sender.send(kbps, stage)`、
UI 线程在同一个 100 ms 心跳里 `source.poll()`），其余接线不动——顺便把 `demo_source.rs` 的
`DemoOptions` 换成引擎参数。完整步骤见 [`docs/real-backend.md`](real-backend.md)。

## 已知组件 API 缺口（Demo 侧规避，未改组件）

1. `snap_to()` 只改 `angle/target`，**不清** `spring_reset_tween`：`set_stage(Upload|Idle)` 或
   `reset()` 之后立刻 `snap_to()`，指针会在后续帧被那条 500 ms 归位 tween 拽回 -135°。
   公开 API 没有"取消归位"的入口（`pause()` 只停泵，`stop()` 连后续 `set_*` 一起停）。
   Demo 与 `tests/instances.rs` 都靠"先把归位跑完再 snap"规避。
2. `shots::render` 只支持正方形逻辑边长（非正方形窗口要自己拼），见上。
3. `shots::WindowSlot` 只保留**最近一次**创建的窗口适配器：同一进程里再 `SpeedGauge::new()`
   就会覆盖它，宿主若在之后才取 `last_window()` 会拿到仪表的窗口而不是主窗口。
