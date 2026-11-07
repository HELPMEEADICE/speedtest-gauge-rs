//! 可运行交互 Demo：把 `SpeedGauge` / `GaugeHandle` / `DemoSource` 完全当**外部库**来用。
//!
//! 运行：
//!   `cargo run --release --example demo`                            # 真窗口（关窗退出）
//!   `SPEED_GAUGE_HEADLESS=1 cargo run --release --example demo`     # 无窗口：出图后退出
//!   `SPEED_GAUGE_STATIC=862 SPEED_GAUGE_STAGE=download …`           # 冻结口径（等价 `?static=&stage=`）
//!   `SPEED_GAUGE_FPS=1 cargo run --release --example demo`          # 窗口帧间隔 p50/p95（3 s 后退出）
//!   `SPEED_GAUGE_FPS=5000 …`                                        # 同上，采样窗口 5000 ms
//!
//! 接法（全是公开 API）：
//! - 两个仪表实例由本文件用 `SpeedGauge::new()` 创建，再经 `slint::ComponentFactory` 注入
//!   `ui/demo.slint` 的两个 `ComponentContainer`。Slint 的属性类型系统里没有"组件实例"类型
//!   （`i-slint-compiler::expression_tree::Type` 无 Component 变体，生成代码只给属性/回调
//!   生成访问器），所以"Rust 拿到实例 + .slint 自定义布局"只能走 ComponentFactory：闭包里
//!   把创建出来的 `SpeedGauge` 存进 `Shared`，Rust 于是能 `GaugeHandle::attach` 它。
//! - 面板控件 → handle：`on_*_clicked` 回调里读 `.slint` 侧属性 → 调对应 `GaugeHandle` API。
//!   面板状态用中性 int/bool（同一个 .slint 被独立编译两次会得到两个不同的 Rust 枚举类型），
//!   枚举映射只发生在本文件。
//! - Run Demo → `DemoSource`；`slint::Timer(100ms)` 里 `tick(100.0)`，样本经 listener 直接
//!   推给主仪表（`set_stage` + `set_speed_kbps`）。
//! - 每 100 ms 把 `angle()/number_text()/lit_count()/ring_visible_count()/timer_running()`
//!   格式化后写回只读状态区。
//!
//! 不做阻塞 IO（headless 渲染也在 UI 线程内、纯 CPU）；不引 async runtime；不加任何依赖。

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::platform::WindowEvent;
use slint::{ComponentHandle, LogicalSize};

use speed_gauge_slint::data_source::SpeedSource;
use speed_gauge_slint::demo_source::{DemoOptions, DemoSource};
use speed_gauge_slint::model::Stage as ModelStage;
use speed_gauge_slint::shots;
use speed_gauge_slint::{
    ConnectionMode, GaugeHandle, GaugeStage, GaugeTheme, MotionMode, ScaleMode, SpeedGauge,
};

// build.rs 里 `ui/demo.slint` 是**最后一次** `slint_build::compile`，所以
// `SLINT_INCLUDE_GENERATED` 正指向 `OUT_DIR/demo.rs`，标准宏即可
// （`src/lib.rs` 要的是 `SpeedGauge` 而不是 `DemoWindow`，那里仍用显式路径）。
slint::include_modules!();

/// 截图口径：1280×800 @ DPR 1（与 docs/demo.md 一致）。
const SHOT_W: f32 = 1280.0;
const SHOT_H: f32 = 800.0;

/// 第二仪表的静态读数（kbps）。
const MINI_KBPS: f64 = 4_200_000.0;

// ---------------------------------------------------------------------------
// 面板中性状态 ↔ 库枚举
// ---------------------------------------------------------------------------

fn stage_from_idx(idx: i32) -> GaugeStage {
    match idx {
        1 => GaugeStage::Ping,
        2 => GaugeStage::Download,
        3 => GaugeStage::Upload,
        4 => GaugeStage::Done,
        _ => GaugeStage::Idle,
    }
}

fn stage_to_idx(stage: GaugeStage) -> i32 {
    match stage {
        GaugeStage::Idle => 0,
        GaugeStage::Ping => 1,
        GaugeStage::Download => 2,
        GaugeStage::Upload => 3,
        GaugeStage::Done => 4,
    }
}

fn connection_from_idx(idx: i32) -> ConnectionMode {
    if idx == 0 {
        ConnectionMode::Multi
    } else {
        ConnectionMode::Single
    }
}

fn connection_to_idx(mode: ConnectionMode) -> i32 {
    match mode {
        ConnectionMode::Multi => 0,
        ConnectionMode::Single => 1,
    }
}

fn scale_from_idx(idx: i32) -> ScaleMode {
    if idx == 0 {
        ScaleMode::Fixed
    } else {
        ScaleMode::Auto
    }
}

fn scale_to_idx(mode: ScaleMode) -> i32 {
    match mode {
        ScaleMode::Fixed => 0,
        ScaleMode::Auto => 1,
    }
}

fn theme_from_idx(idx: i32) -> GaugeTheme {
    if idx == 0 {
        GaugeTheme::Dark
    } else {
        GaugeTheme::Light
    }
}

fn theme_to_idx(theme: GaugeTheme) -> i32 {
    match theme {
        GaugeTheme::Dark => 0,
        GaugeTheme::Light => 1,
    }
}

fn motion_from_idx(idx: i32) -> MotionMode {
    if idx == 0 {
        MotionMode::Spring
    } else {
        MotionMode::Tween
    }
}

fn to_gauge_stage(stage: ModelStage) -> GaugeStage {
    match stage {
        ModelStage::Idle => GaugeStage::Idle,
        ModelStage::Ping => GaugeStage::Ping,
        ModelStage::Download => GaugeStage::Download,
        ModelStage::Upload => GaugeStage::Upload,
        ModelStage::Done => GaugeStage::Done,
    }
}

// ---------------------------------------------------------------------------
// 共享状态
// ---------------------------------------------------------------------------

/// 工厂闭包与回调闭包之间共享的东西。
///
/// 字段分开用多个 `RefCell`（而不是一个大结构）：listener 在 `source.tick()` 期间会回写
/// `main`，与 `source` 的借用必须互不相干，否则同一时刻的 `borrow_mut` 会冲突。
#[derive(Default)]
struct Shared {
    /// 工厂创建出来的仪表实例（keep-alive；控制器内部只用 Weak 指回它）。
    main_gauge: RefCell<Option<SpeedGauge>>,
    mini_gauge: RefCell<Option<SpeedGauge>>,
    /// 各自的控制器。
    main: RefCell<Option<GaugeHandle>>,
    mini: RefCell<Option<GaugeHandle>>,
    /// Run Demo 期间的非 None 数据源。
    source: RefCell<Option<DemoSource>>,
}

/// 工厂要填哪个槽。
#[derive(Clone, Copy)]
enum Which {
    Main,
    Mini,
}

fn gauge_factory(shared: Rc<Shared>, which: Which) -> slint::ComponentFactory {
    slint::ComponentFactory::new(move |_ctx| {
        let gauge = SpeedGauge::new().ok()?;
        let keep = gauge.clone_strong();
        match which {
            Which::Main => *shared.main_gauge.borrow_mut() = Some(keep),
            Which::Mini => *shared.mini_gauge.borrow_mut() = Some(keep),
        }
        Some(gauge)
    })
}

// ---------------------------------------------------------------------------
// 控件 → handle 的接线
// ---------------------------------------------------------------------------

/// 把面板当前状态整体推给两个仪表（首次接线 + 主题联动都走它）。
fn apply_all(ui: &DemoWindow, shared: &Shared) {
    let theme = theme_from_idx(ui.get_theme_idx());
    if let Some(h) = shared.main.borrow().as_ref() {
        h.set_theme(theme);
        h.set_connection_mode(connection_from_idx(ui.get_connection_idx()));
        h.set_scale_mode(scale_from_idx(ui.get_scale_idx()));
        h.set_motion_mode(motion_from_idx(ui.get_motion_idx()));
        h.set_wobble_enabled(ui.get_wobble());
        h.set_animation_enabled(ui.get_animation_on());
        h.set_stage(stage_from_idx(ui.get_stage_idx()));
        let kbps = ui.get_speed_mbps() as f64 * 1000.0;
        if kbps > 0.0 {
            h.set_speed_kbps(kbps);
        }
    }
    if let Some(h) = shared.mini.borrow().as_ref() {
        // 第二仪表：**只**共享 theme；Single / Auto / 4200 Mbps 全部自持、冻结。
        h.set_theme(theme);
        h.set_connection_mode(ConnectionMode::Single);
        h.set_scale_mode(ScaleMode::Auto);
        h.set_motion_mode(MotionMode::Spring);
        h.set_animation_enabled(false);
        h.set_wobble_enabled(false);
        h.set_stage(GaugeStage::Download);
        h.snap_to(MINI_KBPS);
    }
}

/// 仪表实例一旦就位就 attach 控制器并推一次初始配置。返回是否已经就位。
fn wire(ui: &DemoWindow, shared: &Rc<Shared>) -> bool {
    if shared.main.borrow().is_some() && shared.mini.borrow().is_some() {
        return true;
    }
    let (main_ui, mini_ui) = {
        let main_guard = shared.main_gauge.borrow();
        let mini_guard = shared.mini_gauge.borrow();
        match (
            main_guard.as_ref().map(|g| g.clone_strong()),
            mini_guard.as_ref().map(|g| g.clone_strong()),
        ) {
            (Some(a), Some(b)) => (a, b),
            _ => return false,
        }
    };
    if shared.main.borrow().is_none() {
        *shared.main.borrow_mut() = Some(GaugeHandle::attach(&main_ui));
    }
    if shared.mini.borrow().is_none() {
        *shared.mini.borrow_mut() = Some(GaugeHandle::attach(&mini_ui));
    }
    apply_all(ui, shared);
    true
}

/// 停掉 demo（若在跑）。
fn stop_demo(ui: &DemoWindow, shared: &Rc<Shared>) {
    if let Some(mut source) = shared.source.borrow_mut().take() {
        source.stop();
    }
    ui.set_demo_running(false);
}

/// 起一轮 demo：`DemoSource` 的 listener 把样本直接推给主仪表。
fn start_demo(ui: &DemoWindow, shared: &Rc<Shared>, seed: u32) {
    stop_demo(ui, shared);
    let mut source = DemoSource::new(DemoOptions {
        seed,
        ..DemoOptions::default()
    });
    {
        let sh = shared.clone();
        let weak = ui.as_weak();
        source.set_listener(Box::new(move |sample| {
            if let Some(h) = sh.main.borrow().as_ref() {
                h.set_stage(to_gauge_stage(sample.stage));
                h.set_speed_kbps(sample.kbps);
            }
            // 面板滑块与读数字跟着样本走（等价参考实现里的 `updateReadout`）。
            if let Some(ui) = weak.upgrade() {
                let mbps = sample.kbps / 1000.0;
                ui.set_speed_mbps(mbps as f32);
                ui.set_speed_label(format!("{mbps:.0} Mbps").into());
            }
        }));
    }
    {
        let weak = ui.as_weak();
        source.set_phase_listener(Box::new(move |stage: ModelStage| {
            if let Some(ui) = weak.upgrade() {
                ui.set_stage_idx(stage_to_idx(to_gauge_stage(stage)));
            }
        }));
    }
    *shared.source.borrow_mut() = Some(source);
    ui.set_demo_running(true);
}

/// 只读状态区刷新（每 100 ms 一次）。
fn refresh_stats(ui: &DemoWindow, shared: &Rc<Shared>) {
    if let Some(h) = shared.main.borrow().as_ref() {
        ui.set_stat_angle_text(format!("{:.2}°", h.angle()).into());
        ui.set_stat_number_text(h.number_text().into());
        ui.set_stat_lit_text(h.lit_count().to_string().into());
        ui.set_stat_ring_text(h.ring_visible_count().to_string().into());
        ui.set_stat_timer_text(format!("timer_running = {}", h.timer_running()).into());
    }
    ui.set_stat_run_text(
        if shared.source.borrow().is_some() {
            "状态：DemoSource 自动播放"
        } else {
            "状态：manual"
        }
        .into(),
    );
}

/// 把面板回调全部接到 handle / DemoSource 上。
fn connect(ui: &DemoWindow, shared: &Rc<Shared>, cfg: &Config) {
    {
        let weak = ui.as_weak();
        let sh = shared.clone();
        let seed = cfg.seed;
        ui.on_run_clicked(move || {
            let Some(ui) = weak.upgrade() else { return };
            if sh.source.borrow().is_some() {
                stop_demo(&ui, &sh);
            } else {
                // 从头来：Idle + 归零，再交给 DemoSource 自动跑。
                if let Some(h) = sh.main.borrow().as_ref() {
                    h.set_stage(GaugeStage::Idle);
                    h.reset();
                }
                ui.set_stage_idx(0);
                start_demo(&ui, &sh, seed);
            }
            refresh_stats(&ui, &sh);
        });
    }
    {
        let weak = ui.as_weak();
        let sh = shared.clone();
        ui.on_reset_clicked(move || {
            let Some(ui) = weak.upgrade() else { return };
            stop_demo(&ui, &sh);
            if let Some(h) = sh.main.borrow().as_ref() {
                h.set_stage(GaugeStage::Idle);
                h.reset(); // 500 ms easeInOut 归位
            }
            ui.set_stage_idx(0);
            ui.set_speed_mbps(0.0);
            ui.set_speed_label("0 Mbps".into());
            refresh_stats(&ui, &sh);
        });
    }
    {
        let weak = ui.as_weak();
        let sh = shared.clone();
        ui.on_snap_clicked(move || {
            let Some(ui) = weak.upgrade() else { return };
            let text = ui.get_snap_text().to_string();
            let Ok(mbps) = text.trim().parse::<f64>() else {
                eprintln!("[demo] Snap To：{text:?} 不是数字");
                return;
            };
            stop_demo(&ui, &sh);
            if let Some(h) = sh.main.borrow().as_ref() {
                h.snap_to(mbps * 1000.0);
            }
            ui.set_speed_mbps(mbps as f32);
            ui.set_speed_label(format!("{mbps:.0} Mbps").into());
            refresh_stats(&ui, &sh);
        });
    }
    {
        let weak = ui.as_weak();
        let sh = shared.clone();
        ui.on_speed_picked(move || {
            let Some(ui) = weak.upgrade() else { return };
            let mbps = ui.get_speed_mbps() as f64;
            // 滑块是 manual：只改读数，不动 stage（任务书要求「stage 视为 manual」）。
            stop_demo(&ui, &sh);
            if let Some(h) = sh.main.borrow().as_ref() {
                h.set_speed_kbps(mbps * 1000.0);
            }
            ui.set_speed_label(format!("{mbps:.0} Mbps").into());
            refresh_stats(&ui, &sh);
        });
    }
    {
        let weak = ui.as_weak();
        let sh = shared.clone();
        ui.on_stage_picked(move || {
            let Some(ui) = weak.upgrade() else { return };
            stop_demo(&ui, &sh);
            if let Some(h) = sh.main.borrow().as_ref() {
                h.set_stage(stage_from_idx(ui.get_stage_idx()));
            }
            refresh_stats(&ui, &sh);
        });
    }
    {
        let weak = ui.as_weak();
        let sh = shared.clone();
        ui.on_connection_picked(move || {
            let Some(ui) = weak.upgrade() else { return };
            if let Some(h) = sh.main.borrow().as_ref() {
                h.set_connection_mode(connection_from_idx(ui.get_connection_idx()));
            }
            refresh_stats(&ui, &sh);
        });
    }
    {
        let weak = ui.as_weak();
        let sh = shared.clone();
        ui.on_scale_picked(move || {
            let Some(ui) = weak.upgrade() else { return };
            if let Some(h) = sh.main.borrow().as_ref() {
                h.set_scale_mode(scale_from_idx(ui.get_scale_idx()));
            }
            refresh_stats(&ui, &sh);
        });
    }
    {
        let weak = ui.as_weak();
        let sh = shared.clone();
        ui.on_theme_picked(move || {
            let Some(ui) = weak.upgrade() else { return };
            let theme = theme_from_idx(ui.get_theme_idx());
            // 两个仪表共用同一 theme；面板底色由 .slint 依 theme-idx 自己联动。
            if let Some(h) = sh.main.borrow().as_ref() {
                h.set_theme(theme);
            }
            if let Some(h) = sh.mini.borrow().as_ref() {
                h.set_theme(theme);
            }
            refresh_stats(&ui, &sh);
        });
    }
    {
        let weak = ui.as_weak();
        let sh = shared.clone();
        ui.on_motion_picked(move || {
            let Some(ui) = weak.upgrade() else { return };
            if let Some(h) = sh.main.borrow().as_ref() {
                h.set_motion_mode(motion_from_idx(ui.get_motion_idx()));
            }
            refresh_stats(&ui, &sh);
        });
    }
    {
        let weak = ui.as_weak();
        let sh = shared.clone();
        ui.on_wobble_picked(move || {
            let Some(ui) = weak.upgrade() else { return };
            if let Some(h) = sh.main.borrow().as_ref() {
                h.set_wobble_enabled(ui.get_wobble());
            }
            refresh_stats(&ui, &sh);
        });
    }
    {
        let weak = ui.as_weak();
        let sh = shared.clone();
        ui.on_animation_picked(move || {
            let Some(ui) = weak.upgrade() else { return };
            if let Some(h) = sh.main.borrow().as_ref() {
                h.set_animation_enabled(ui.get_animation_on());
            }
            refresh_stats(&ui, &sh);
        });
    }
    {
        let weak = ui.as_weak();
        let sh = shared.clone();
        ui.on_pause_clicked(move || {
            let Some(ui) = weak.upgrade() else { return };
            let paused = !ui.get_paused();
            ui.set_paused(paused);
            if let Some(h) = sh.main.borrow().as_ref() {
                if paused {
                    h.pause();
                } else {
                    h.resume();
                }
            }
            refresh_stats(&ui, &sh);
        });
    }
}

/// 100 ms 心跳：接线（首次）+ 推进 DemoSource + 刷状态区。
fn start_heartbeat(ui: &DemoWindow, shared: &Rc<Shared>) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let sh = shared.clone();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(100),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            wire(&ui, &sh);
            let mut finished = false;
            {
                // listener 内部只会 borrow `sh.main`，与这里的 `source` 借用互不重叠。
                let mut guard = sh.source.borrow_mut();
                if let Some(source) = guard.as_mut() {
                    source.tick(100.0);
                    finished = !source.is_running();
                }
            }
            if finished {
                sh.source.borrow_mut().take();
                ui.set_demo_running(false);
            }
            refresh_stats(&ui, &sh);
        },
    );
    timer
}

// ---------------------------------------------------------------------------
// 参数（URL 参数的 CLI/环境变量等价物）
// ---------------------------------------------------------------------------

struct Config {
    headless: bool,
    /// 等价 `?static=`：冻结读数（Mbps）。
    static_mbps: Option<f64>,
    /// 等价 `?stage=`。
    stage: Option<GaugeStage>,
    /// 等价 `?theme=`。
    theme: GaugeTheme,
    /// 等价 `?mode=`。
    connection: ConnectionMode,
    /// 等价 `?scale=`。
    scale: ScaleMode,
    /// 等价 `?seed=`。
    seed: u32,
    /// 等价 `?autoplay=1`。
    autoplay: bool,
    /// 真窗口演示用：>0 时到点自动退出（便于自动化验收）。
    exit_ms: u64,
    /// headless 下跑"控件联动自测"而不是出图。
    selftest: bool,
    /// `SPEED_GAUGE_FPS=1`（默认 3000 ms）或 `SPEED_GAUGE_FPS=毫秒数`：窗口帧间隔探针。
    fps_ms: Option<f64>,
}

impl Config {
    fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).ok();
        let stage =
            var("SPEED_GAUGE_STAGE").and_then(|s| match s.trim().to_ascii_lowercase().as_str() {
                "idle" => Some(GaugeStage::Idle),
                "ping" => Some(GaugeStage::Ping),
                "download" => Some(GaugeStage::Download),
                "upload" => Some(GaugeStage::Upload),
                "done" => Some(GaugeStage::Done),
                other => {
                    eprintln!("[demo] 未知 stage {other:?}，忽略");
                    None
                }
            });
        Self {
            headless: var("SPEED_GAUGE_HEADLESS").is_some(),
            static_mbps: var("SPEED_GAUGE_STATIC").and_then(|s| s.trim().parse::<f64>().ok()),
            stage,
            theme: match var("SPEED_GAUGE_THEME").as_deref() {
                Some("light") => GaugeTheme::Light,
                _ => GaugeTheme::Dark,
            },
            connection: match var("SPEED_GAUGE_MODE").as_deref() {
                Some("single") => ConnectionMode::Single,
                _ => ConnectionMode::Multi,
            },
            scale: match var("SPEED_GAUGE_SCALE").as_deref() {
                Some("auto") => ScaleMode::Auto,
                _ => ScaleMode::Fixed,
            },
            seed: var("SPEED_GAUGE_SEED")
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(20_260_101),
            autoplay: var("SPEED_GAUGE_AUTOPLAY").is_some(),
            exit_ms: var("SPEED_GAUGE_EXIT_MS")
                .and_then(|s| s.trim().parse::<u64>().ok())
                .unwrap_or(0),
            selftest: var("SPEED_GAUGE_SELFTEST").is_some(),
            fps_ms: var("SPEED_GAUGE_FPS").map(|s| match s.trim().parse::<f64>() {
                // 1（或非数字）= "开"，用默认 3000 ms；其它正数当作采样窗口毫秒数。
                Ok(ms) if ms > 1.0 => ms,
                _ => 3000.0,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// headless：1280×800 单张出图
// ---------------------------------------------------------------------------

/// 渲染一帧到给定逻辑尺寸：`shots::render_sized` 的薄包装（可选落盘）。
///
/// 1280×800 这种非正方形窗口现在由 `shots::render_sized` 直接支持；窗口适配器也由
/// `shots` 按**组件身份**解析（见 `WindowSlot::for_component`），不再依赖"最近创建的那个窗口"，
/// 因此这里不必再手工拼 `MinimalSoftwareWindow` 的渲染循环。
fn render_rect(
    slot: &shots::WindowSlot,
    ui: &DemoWindow,
    size: (f32, f32),
    dpr: f32,
    out: Option<&Path>,
) -> Result<shots::Shot, Box<dyn std::error::Error>> {
    let shot = shots::render_sized(ui, slot, size, dpr)?;
    if let Some(path) = out {
        shots::save_png(&shot, path)?;
    }
    Ok(shot)
}

/// 与 `(0,0)` 像素不同的像素数（Demo 的底不是纯黑，用角落像素当"背景色"）。
fn non_background_pixels(shot: &shots::Shot) -> usize {
    let Some(bg) = shot.pixel(0, 0) else { return 0 };
    shot.buffer
        .as_slice()
        .iter()
        .filter(|p| p.red != bg.red || p.green != bg.green || p.blue != bg.blue)
        .count()
}

/// headless 装配：装平台 → 建窗口 → 注入两个工厂 → 泵帧直到两个手柄就位。
///
/// 返回 `(窗口组件, 共享状态, DemoWindow 自己的 slot)`。slot 用 `WindowSlot::for_component`
/// 按组件身份取：工厂随后用 `SpeedGauge::new()` 创建的两个仪表各有各的窗口适配器，三者互不影响
/// （旧实现靠"创建顺序"，`SpeedGauge::new()` 会把 DemoWindow 的适配器覆盖掉）。
fn setup_headless(
    cfg: &Config,
) -> Result<(DemoWindow, Rc<Shared>, shots::WindowSlot), Box<dyn std::error::Error>> {
    let _platform = shots::install()?;
    let ui = DemoWindow::new()?;
    let slot = shots::WindowSlot::for_component(&ui)?;

    let shared = Rc::new(Shared::default());
    // 初值先写进 .slint 侧属性，`apply_all` 会据此初始化两个仪表。
    ui.set_theme_idx(theme_to_idx(cfg.theme));
    ui.set_connection_idx(connection_to_idx(cfg.connection));
    ui.set_scale_idx(scale_to_idx(cfg.scale));
    ui.set_main_gauge_factory(gauge_factory(shared.clone(), Which::Main));
    ui.set_mini_gauge_factory(gauge_factory(shared.clone(), Which::Mini));
    // 与真窗口一样接上全部回调：headless 的自测就是靠 `invoke_*` 走同一条接线。
    connect(&ui, &shared, cfg);
    ui.set_speed_label("0 Mbps".into());
    ui.window()
        .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
    ui.window().set_size(LogicalSize::new(SHOT_W, SHOT_H));
    ui.show()?;
    println!(
        "[demo] window={}x{} scale_factor={} (headless)",
        SHOT_W,
        SHOT_H,
        ui.window().scale_factor()
    );

    // 泵帧直到 ComponentContainer 把两个仪表建出来（工厂在首次布局/渲染时被调用）。
    let mut ready = false;
    for _ in 0..120 {
        let _ = render_rect(&slot, &ui, (SHOT_W, SHOT_H), 1.0, None);
        slint::platform::update_timers_and_animations();
        if wire(&ui, &shared) {
            ready = true;
            break;
        }
    }
    if !ready {
        return Err("两个仪表实例没能在 120 帧内就位（ComponentFactory 未被调用）".into());
    }
    println!(
        "[demo] 两个实例已就位：main={} mini={}（各自独立 GaugeHandle）",
        shared.main.borrow().is_some(),
        shared.mini.borrow().is_some()
    );
    Ok((ui, shared, slot))
}

fn run_headless(cfg: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let (ui, shared, slot) = setup_headless(cfg)?;

    if let Some(mbps) = cfg.static_mbps {
        // 冻结口径：先 stage 再 snapTo（与参考实现 `[data-stage]` 的顺序一致）。
        let stage = cfg.stage.unwrap_or(if mbps > 0.0 {
            GaugeStage::Download
        } else {
            GaugeStage::Idle
        });
        ui.set_stage_idx(stage_to_idx(stage));
        ui.set_speed_mbps(mbps as f32);
        ui.set_speed_label(format!("{mbps:.0} Mbps").into());
        if let Some(h) = shared.main.borrow().as_ref() {
            h.set_stage(stage);
            h.snap_to(mbps * 1000.0);
        }
    } else {
        run_demo_headless(&ui, &shared, cfg);
    }

    refresh_stats(&ui, &shared);

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("comparison/render/demo-ui.png");
    let shot = render_rect(&slot, &ui, (SHOT_W, SHOT_H), 1.0, Some(&out))?;
    let total = (shot.width * shot.height) as usize;
    println!(
        "[demo] 已写出 {}（{}×{}，DPR {}）非黑像素 {}/{}（{:.2}%），非背景色像素 {}",
        out.display(),
        shot.width,
        shot.height,
        shot.scale_factor,
        shot.non_black_pixels(),
        total,
        100.0 * shot.non_black_pixels() as f64 / total as f64,
        non_background_pixels(&shot),
    );
    {
        let main_guard = shared.main.borrow();
        let Some(h) = main_guard.as_ref() else {
            return Ok(());
        };
        println!(
            "[demo] main: angle={:.3}° number={} lit={} ring={} timer_running={}",
            h.angle(),
            h.number_text(),
            h.lit_count(),
            h.ring_visible_count(),
            h.timer_running()
        );
    }
    {
        let mini_guard = shared.mini.borrow();
        if let Some(h) = mini_guard.as_ref() {
            println!(
                "[demo] mini: angle={:.3}° number={} lit={} ring={}（Single/Auto/4200 Mbps，独立）",
                h.angle(),
                h.number_text(),
                h.lit_count(),
                h.ring_visible_count()
            );
        }
    }
    Ok(())
}

/// 无窗口跑完一整轮 DemoSource（Idle→Ping→Download→归零→Upload→Done），并打印阶段轨迹。
fn run_demo_headless(ui: &DemoWindow, shared: &Rc<Shared>, cfg: &Config) {
    start_demo(ui, shared, cfg.seed);
    let mut last_stage = GaugeStage::Idle;
    let mut ticks = 0usize;
    let mut upload_min_angle = f64::INFINITY;
    for _ in 0..400 {
        let mut running = false;
        {
            let mut guard = shared.source.borrow_mut();
            if let Some(source) = guard.as_mut() {
                source.tick(100.0);
                running = source.is_running();
            }
        }
        ticks += 1;
        // 每拍手动推进 3 帧 ≈ 48 ms（0.1 s 样本间隔内的插值）。
        if let Some(h) = shared.main.borrow().as_ref() {
            for _ in 0..3 {
                h.step(0.016);
            }
            let stage = stage_from_idx(ui.get_stage_idx());
            if stage == GaugeStage::Upload {
                upload_min_angle = upload_min_angle.min(h.angle());
            }
            if stage != last_stage {
                println!(
                    "[demo] stage {:?} -> {:?}  angle={:.3}° number={} ring={}",
                    last_stage,
                    stage,
                    h.angle(),
                    h.number_text(),
                    h.ring_visible_count()
                );
                last_stage = stage;
            }
        }
        if !running {
            break;
        }
    }
    // 让指针收敛（Done 之后不再有样本），并多泵几帧让动画泵自己停表。
    if let Some(h) = shared.main.borrow().as_ref() {
        for _ in 0..600 {
            h.step(1.0 / 60.0);
            if h.is_settled() && !h.timer_running() {
                break;
            }
        }
    }
    if upload_min_angle.is_finite() {
        println!(
            "[demo] 归零：Upload 阶段指针最低角 {upload_min_angle:.3}°（setStage(Upload) 触发 motion.reset()）"
        );
    }
    shared.source.borrow_mut().take();
    ui.set_demo_running(false);
    println!(
        "[demo] DemoSource 跑完：{ticks} 拍，末阶段={:?}",
        stage_from_idx(ui.get_stage_idx())
    );
}

// ---------------------------------------------------------------------------
// 控件联动自测（headless：逐个回调走一遍，打印 ✓/✗）
// ---------------------------------------------------------------------------

fn report(name: &str, ok: bool, detail: &str) -> bool {
    println!("[selftest] {} {name}：{detail}", if ok { "✓" } else { "✗" });
    ok
}

/// 第二仪表的可观察状态（用于"两仪表互不影响"断言）。
fn mini_state(shared: &Shared) -> (u64, String, usize, usize) {
    let guard = shared.mini.borrow();
    match guard.as_ref() {
        Some(h) => (
            h.angle().to_bits(),
            h.number_text(),
            h.lit_count(),
            h.ring_visible_count(),
        ),
        None => (0, String::new(), 0, 0),
    }
}

fn main_handle(shared: &Shared) -> Result<GaugeHandle, Box<dyn std::error::Error>> {
    shared
        .main
        .borrow()
        .clone()
        .ok_or_else(|| "主仪表 GaugeHandle 缺失".into())
}

fn main_component(shared: &Shared) -> Result<SpeedGauge, Box<dyn std::error::Error>> {
    let guard = shared.main_gauge.borrow();
    guard
        .as_ref()
        .map(|g| g.clone_strong())
        .ok_or_else(|| "主仪表实例缺失".into())
}

fn run_selftest(cfg: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let (ui, shared, slot) = setup_headless(cfg)?;
    let main_ui = main_component(&shared)?;
    let h = main_handle(&shared)?;
    let mut failures = 0usize;
    let mini_before = mini_state(&shared);

    // ---- 1) Snap To：输入框 + 按钮，即时精确（Fixed 刻度上限 1000 ⇒ 夹到 135°）----
    ui.set_snap_text("1500".into());
    ui.invoke_snap_clicked();
    let ok = (h.angle() - 135.0).abs() < 1e-9
        && h.number_text() == "1,500.00"
        && (ui.get_speed_mbps() - 1500.0).abs() < 1e-3
        && ui.get_speed_label() == "1500 Mbps";
    if !report(
        "Snap To",
        ok,
        &format!(
            "snap-text=1500 → angle={:.3}° number={} 滑块={}",
            h.angle(),
            h.number_text(),
            ui.get_speed_label()
        ),
    ) {
        failures += 1;
    }

    // ---- 2) Reset：500 ms 归位动画跑完必须回到 -135° ----
    ui.invoke_reset_clicked();
    let reset_number = h.number_text();
    let reset_stage_idx = ui.get_stage_idx();
    for _ in 0..90 {
        h.step(1.0 / 60.0);
    }
    let ok = reset_number == "0.00"
        && reset_stage_idx == 0
        && (h.angle() + 135.0).abs() < 0.05
        && ui.get_speed_mbps() < 1e-3;
    if !report(
        "Reset",
        ok,
        &format!(
            "number={reset_number} stage-idx={reset_stage_idx} 收敛 angle={:.3}°（期望 -135°）",
            h.angle()
        ),
    ) {
        failures += 1;
    }

    // ---- 3) Speed 滑块：读数立刻更新、指针需要推进帧 ----
    ui.set_speed_mbps(3200.0);
    ui.invoke_speed_picked();
    let before = h.angle();
    let number_now = h.number_text();
    let not_moved_yet = (h.angle() - before).abs() < 1e-9;
    for _ in 0..20 {
        h.step(1.0 / 60.0);
    }
    let after = h.angle();
    let ok = number_now == "3,200.00"
        && not_moved_yet
        && (after - before).abs() > 1.0
        && ui.get_speed_label() == "3200 Mbps";
    if !report(
        "Speed 滑块",
        ok,
        &format!(
            "3200 Mbps → number={number_now} 立即 angle={before:.3}°（未动）→ 20 帧后 {after:.3}°"
        ),
    ) {
        failures += 1;
    }

    // ---- 4) Stage 五档：逐档点一遍，组件 stage 属性必须跟上 ----
    let mut stage_ok = true;
    let mut stage_detail = String::new();
    for (idx, want) in [
        (0, GaugeStage::Idle),
        (1, GaugeStage::Ping),
        (2, GaugeStage::Download),
        (3, GaugeStage::Upload),
        (4, GaugeStage::Done),
    ] {
        ui.set_stage_idx(idx);
        ui.invoke_stage_picked();
        let got = main_ui.get_stage();
        stage_ok &= got == want;
        // Upload / Idle 会挂归位 tween，跑完再进下一档，避免串味
        for _ in 0..45 {
            h.step(1.0 / 60.0);
        }
        stage_detail += &format!("{idx}={got:?}(ring {}) ", h.ring_visible_count());
    }
    if !report("Stage 五档", stage_ok, stage_detail.trim_end()) {
        failures += 1;
    }

    // ---- 5) Connection / Scale / Motion / Theme：切换后组件属性 + 渲染都正确 ----
    let mut mode_ok = true;
    let mut mode_detail = String::new();
    for idx in [0, 1] {
        ui.set_connection_idx(idx);
        ui.invoke_connection_picked();
        let got = main_ui.get_connection_mode();
        mode_ok &= got == connection_from_idx(idx);
        mode_detail += &format!("connection{idx}={got:?} ");
    }
    for idx in [0, 1] {
        ui.set_scale_idx(idx);
        ui.invoke_scale_picked();
        let got = main_ui.get_scale_mode();
        mode_ok &= got == scale_from_idx(idx);
        mode_detail += &format!("scale{idx}={got:?} ");
    }
    for idx in [0, 1] {
        ui.set_motion_idx(idx);
        ui.invoke_motion_picked();
        let got = main_ui.get_motion_mode();
        mode_ok &= got == motion_from_idx(idx);
        mode_detail += &format!("motion{idx}={got:?} ");
    }
    if !report(
        "Connection / Scale / Motion",
        mode_ok,
        mode_detail.trim_end(),
    ) {
        failures += 1;
    }

    // Theme：两个仪表同一 theme，且**面板底色**真的换了（取面板内一点的实际像素）
    ui.set_theme_idx(1);
    ui.invoke_theme_picked();
    let light_ok = main_ui.get_theme() == GaugeTheme::Light;
    let light_shot = render_rect(&slot, &ui, (SHOT_W, SHOT_H), 1.0, None)?;
    let light_px = light_shot.pixel(1200, 40).map(|p| (p.red, p.green, p.blue));
    ui.set_theme_idx(0);
    ui.invoke_theme_picked();
    let dark_ok = main_ui.get_theme() == GaugeTheme::Dark;
    let dark_shot = render_rect(&slot, &ui, (SHOT_W, SHOT_H), 1.0, None)?;
    let dark_px = dark_shot.pixel(1200, 40).map(|p| (p.red, p.green, p.blue));
    let ok = light_ok
        && dark_ok
        && light_px != dark_px
        && dark_shot.non_black_pixels() > 100_000
        && light_shot.non_black_pixels() > 100_000;
    if !report(
        "Theme（面板底色联动）",
        ok,
        &format!(
            "light 面板像素={light_px:?} / dark 面板像素={dark_px:?}，非黑像素 dark={} light={}",
            dark_shot.non_black_pixels(),
            light_shot.non_black_pixels()
        ),
    ) {
        failures += 1;
    }

    // ---- 6) Wobble / Animation / Pause ----
    ui.set_wobble(false);
    ui.invoke_wobble_picked();
    let wobble_off = !main_ui.get_wobble_enabled();
    ui.set_wobble(true);
    ui.invoke_wobble_picked();
    let wobble_on = main_ui.get_wobble_enabled();

    ui.set_animation_on(false);
    ui.invoke_animation_picked();
    let anim_off = !main_ui.get_animation_enabled();
    let before = h.angle();
    h.set_speed_kbps(500_000.0);
    let anim_off_instant = (h.angle() - before).abs() > 1.0;
    ui.set_animation_on(true);
    ui.invoke_animation_picked();
    let anim_on = main_ui.get_animation_enabled();
    let ok = wobble_off && wobble_on && anim_off && anim_off_instant && anim_on;
    if !report(
        "Wobble / Animation",
        ok,
        &format!(
            "wobble off→on ok={}；animation off→on ok={}；动画关闭时 set_speed_kbps 立即定位={}",
            wobble_off && wobble_on,
            anim_off && anim_on,
            anim_off_instant
        ),
    ) {
        failures += 1;
    }

    ui.invoke_pause_clicked();
    let paused = ui.get_paused();
    h.set_speed_kbps(1_000_000.0);
    let paused_timer = h.timer_running();
    let paused_ok = paused && !paused_timer;
    ui.invoke_pause_clicked();
    let resumed = !ui.get_paused();
    h.set_speed_kbps(862_000.0);
    let resumed_armed = h.timer_running();
    if !report(
        "Pause / Resume",
        paused_ok && resumed && resumed_armed,
        &format!(
            "pause 后 paused={paused} timer_running={paused_timer}；resume 后 paused={} timer_running={resumed_armed}",
            !resumed
        ),
    ) {
        failures += 1;
    }

    // ---- 7) 两仪表互不影响：上面折腾完，第二仪表逐字段不变 ----
    let mini_after = mini_state(&shared);
    let ok = mini_before == mini_after && mini_after.1 == "4,200.00";
    if !report(
        "两仪表互不影响",
        ok,
        &format!(
            "mini {:.3}° / {} / lit {} / ring {}（走完上面全部操作后与开始时一致）",
            f64::from_bits(mini_after.0),
            mini_after.1,
            mini_after.2,
            mini_after.3
        ),
    ) {
        failures += 1;
    }

    println!("[selftest] 结束：{} 项失败", failures);
    if failures > 0 {
        return Err(format!("{failures} 项控件联动自测失败").into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 真窗口
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 帧间隔探针（`SPEED_GAUGE_FPS`，只在本 example 里）
// ---------------------------------------------------------------------------

/// 帧间隔探针的句柄：持有定时器使其不被回收。
struct FrameProbe {
    _timer: Rc<slint::Timer>,
}

/// 最近的样本数（分位数用 nearest-rank）。
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = (((sorted.len() - 1) as f64) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// 采样"事件循环帧间隔"：每 1 ms 回调一次，回调里 `request_redraw()` 请求下一帧，
/// 并记录与上次回调的间隔。
///
/// 语义边界（写进 docs/performance.md 用）：相邻两次回调之间必然已经包含上一帧的提交，
/// 所以采样值 = `max(1 ms 定时器, 后端 render 耗时, vsync 节流)`；它给的是**帧间隔的上界**
/// 而不是纯 GPU 着色耗时。窗口后端没有可用的 per-frame 回调（`Window` 不暴露 paint hook），
/// 这是不侵入组件库能做到的最接近量。
///
/// 采样窗口到点后打印 p50/p95/均值并 `quit_event_loop()`。
fn start_frame_probe(ui: &DemoWindow, window_ms: f64) -> FrameProbe {
    let timer = Rc::new(slint::Timer::default());
    let weak = ui.as_weak();
    let samples: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
    let last = Rc::new(RefCell::new(Instant::now()));
    let start = Instant::now();
    let fired = Rc::new(RefCell::new(0usize));
    {
        let timer_cb = timer.clone();
        let samples_cb = samples.clone();
        let last_cb = last.clone();
        let fired_cb = fired.clone();
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(1),
            move || {
                let Some(ui) = weak.upgrade() else {
                    timer_cb.stop();
                    return;
                };
                let now = Instant::now();
                let dt = now.duration_since(*last_cb.borrow()).as_secs_f64() * 1000.0;
                *last_cb.borrow_mut() = now;
                *fired_cb.borrow_mut() += 1;
                // 丢掉第 1 个样本（含建窗/首帧上传的抖动）。
                if *fired_cb.borrow() > 1 {
                    samples_cb.borrow_mut().push(dt);
                }
                ui.window().request_redraw();
                let elapsed = now.duration_since(start).as_secs_f64() * 1000.0;
                if elapsed >= window_ms {
                    timer_cb.stop();
                    let raw = samples_cb.borrow().clone();
                    let mut sorted = raw.clone();
                    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    let n = sorted.len();
                    let mean = if n == 0 {
                        f64::NAN
                    } else {
                        sorted.iter().sum::<f64>() / n as f64
                    };
                    println!(
                        "[fps] 采样窗口 {:.0} ms：帧数 {}（丢掉首帧），均值 {:.3} ms（{:.1} fps）\
                         p50 {:.3} ms（{:.1} fps）p95 {:.3} ms（{:.1} fps）p99 {:.3} ms 最小 {:.3} 最大 {:.3}",
                        elapsed,
                        n,
                        mean,
                        1000.0 / mean,
                        percentile(&sorted, 0.50),
                        1000.0 / percentile(&sorted, 0.50),
                        percentile(&sorted, 0.95),
                        1000.0 / percentile(&sorted, 0.95),
                        percentile(&sorted, 0.99),
                        sorted.first().copied().unwrap_or(f64::NAN),
                        sorted.last().copied().unwrap_or(f64::NAN),
                    );
                    println!(
                        "[fps] 说明：值为 request_redraw→下一帧回调的间隔上界（含 vsync 与后端 render），\
                         不是纯着色耗时；采样法见 docs/performance.md §5"
                    );
                    let _ = slint::quit_event_loop();
                }
            },
        );
    }
    FrameProbe { _timer: timer }
}

fn run_windowed(cfg: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let ui = DemoWindow::new()?;
    let shared = Rc::new(Shared::default());
    ui.set_theme_idx(theme_to_idx(cfg.theme));
    ui.set_connection_idx(connection_to_idx(cfg.connection));
    ui.set_scale_idx(scale_to_idx(cfg.scale));
    ui.set_speed_label("0 Mbps".into());
    ui.set_main_gauge_factory(gauge_factory(shared.clone(), Which::Main));
    ui.set_mini_gauge_factory(gauge_factory(shared.clone(), Which::Mini));
    connect(&ui, &shared, cfg);
    ui.window().set_size(LogicalSize::new(SHOT_W, SHOT_H));
    ui.show()?;

    let window = ui.window();
    let physical = window.size();
    println!(
        "[demo] window={}x{} scale_factor={:.2}",
        physical.width,
        physical.height,
        window.scale_factor()
    );
    println!(
        "[demo] 面板：Run Demo / Reset / Snap To / Speed 滑块 / Stage / Connection / Scale / \
         Theme / Motion / Wobble / Animation / Pause；面板底部小仪表为独立实例"
    );

    // 第一帧之前工厂还没跑，100 ms 心跳里会 attach。
    let heartbeat = start_heartbeat(&ui, &shared);
    // `SPEED_GAUGE_FPS=1`：连续采样 3 秒帧间隔，打印 p50/p95 后退出（性能测量用）。
    let _probe = cfg.fps_ms.map(|ms| start_frame_probe(&ui, ms));
    if cfg.autoplay {
        start_demo(&ui, &shared, cfg.seed);
    }
    if cfg.exit_ms > 0 {
        slint::Timer::single_shot(Duration::from_millis(cfg.exit_ms), || {
            println!("[demo] 到达 SPEED_GAUGE_EXIT_MS，退出事件循环");
            let _ = slint::quit_event_loop();
        });
    }

    slint::run_event_loop()?;

    {
        let main_guard = shared.main.borrow();
        let mini_guard = shared.mini.borrow();
        let (angle, number, timer) = match main_guard.as_ref() {
            Some(h) => (h.angle(), h.number_text(), h.timer_running()),
            None => (f64::NAN, String::new(), false),
        };
        println!(
            "[demo] 退出：main angle={angle:.3}° number={number} timer_running={timer}；mini number={}",
            mini_guard
                .as_ref()
                .map(|h| h.number_text())
                .unwrap_or_default()
        );
    }
    drop(heartbeat);
    Ok(())
}

fn main() {
    let cfg = Config::from_env();
    if cfg.headless {
        let result = if cfg.selftest {
            run_selftest(&cfg)
        } else {
            run_headless(&cfg)
        };
        if let Err(e) = result {
            eprintln!("[demo] headless 失败：{e}");
            std::process::exit(1);
        }
        return;
    }
    match run_windowed(&cfg) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("[demo] 窗口后端不可用（{e}），改用 headless 出图");
            if let Err(e2) = run_headless(&cfg) {
                eprintln!("[demo] headless 也失败：{e2}");
                std::process::exit(1);
            }
        }
    }
}
