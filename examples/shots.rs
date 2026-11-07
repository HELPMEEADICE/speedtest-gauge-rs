//! 自证截图：渲染 8 个状态到 `comparison/render/*.png`（1144×1144），
//! 并顺带做三件事：
//! 1. **指针渐变标定**：0°/45°/90°/116.37° 四个角度，沿指针轴采样 alpha 剖面，
//!    与"hub 0 → tip 0.6 线性"对比，打印最大偏差（> 4/255 就该改用切片法）。
//! 2. **单帧耗时**：286×286 release，n = 10 取中位。
//! 3. **定时器收敛断言**：动画跑完后 `Timer` 必须自己停下（`timer_running() == false`）。
//!
//! 运行：`cargo run --release --example shots`

use std::path::PathBuf;
use std::time::{Duration, Instant};

use speed_gauge_slint::renderer::SliceComposite;
use speed_gauge_slint::shots::{self, WindowSlot};
use speed_gauge_slint::{
    ConnectionMode, GaugeHandle, GaugeStage, GaugeTheme, LabelSpec, MotionMode, ScaleMode,
    SpeedGauge,
};

/// 组件逻辑边长（= viewBox 的 286 uu）。
const LOGICAL: f32 = 286.0;
/// 参考图口径：286 uu × 4 = 1144 px。
const DPR: f32 = 4.0;

/// 一个验收状态的配方。
struct State {
    /// 文件名（不含扩展名）。
    name: &'static str,
    /// 阶段。
    stage: GaugeStage,
    /// 读数（kbps）。
    kbps: f64,
    /// 刻度模式。
    scale: ScaleMode,
}

const STATES: [State; 8] = [
    State {
        name: "idle",
        stage: GaugeStage::Idle,
        kbps: 0.0,
        scale: ScaleMode::Fixed,
    },
    State {
        name: "low",
        stage: GaugeStage::Download,
        kbps: 8_000.0,
        scale: ScaleMode::Fixed,
    },
    State {
        name: "mid",
        stage: GaugeStage::Download,
        kbps: 155_610.0,
        scale: ScaleMode::Fixed,
    },
    State {
        name: "high",
        stage: GaugeStage::Download,
        kbps: 862_000.0,
        scale: ScaleMode::Fixed,
    },
    State {
        name: "gigabit",
        stage: GaugeStage::Download,
        kbps: 1_000_000.0,
        scale: ScaleMode::Fixed,
    },
    State {
        name: "upload",
        stage: GaugeStage::Upload,
        kbps: 341_000.0,
        scale: ScaleMode::Fixed,
    },
    State {
        name: "done",
        stage: GaugeStage::Done,
        kbps: 862_000.0,
        scale: ScaleMode::Fixed,
    },
    State {
        name: "extended",
        stage: GaugeStage::Download,
        kbps: 4_200_000.0,
        scale: ScaleMode::Auto,
    },
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 先装平台再碰字体：`fontique` 的 shared_collection() 会触发 Slint 平台初始化，
    // 反过来会让后面的 set_platform 报 AlreadySet。
    let _platform = shots::install()?;
    // 字体必须早于任何组件创建：`font-family` 不支持回退列表，缺字体时墨迹宽度整体走样。
    let fonts = speed_gauge_slint::fonts::register_bundled_fonts();
    println!("== 字体注册 ==\n  {fonts}");

    // ---- 0. 切片参数（实验开关；缺省 = 32 片 / 无重叠 = 历史行为）-------------
    let cfg = speed_gauge_slint::renderer::SliceCfg {
        slices: std::env::var("SPEED_GAUGE_NEEDLE_SLICES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(speed_gauge_slint::renderer::NEEDLE_SLICES),
        overlap: std::env::var("SPEED_GAUGE_NEEDLE_OVERLAP")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
            .unwrap_or(0.0),
    };
    speed_gauge_slint::renderer::set_slice_cfg(cfg);
    println!(
        "== 指针切片参数 ==\n  N = {}  重叠 = {:.2} uu",
        cfg.slices, cfg.overlap
    );

    // ---- 0b. 合成模式（第二轮结构性 A/B：alpha over page vs 不透明预合成）------
    let composite = match std::env::var("SPEED_GAUGE_NEEDLE_COMPOSITE")
        .ok()
        .as_deref()
    {
        Some("alpha") => speed_gauge_slint::renderer::SliceComposite::Alpha,
        Some("opaque") => speed_gauge_slint::renderer::SliceComposite::Opaque,
        _ => speed_gauge_slint::renderer::SLICE_COMPOSITE,
    };
    speed_gauge_slint::renderer::set_slice_composite(composite);
    println!("== 指针合成模式 ==\n  {composite:?}");

    #[cfg(feature = "needle-raster")]
    println!(
        "== 指针实现 ==\n  光栅化（feature needle-raster：tiny-skia → 一张 RGBA → `Image` 一次合成）"
    );
    #[cfg(not(feature = "needle-raster"))]
    println!("== 指针实现 ==\n  切片（feature needle-raster 已关闭：32 片梯形平色，逐路径 over）");

    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("comparison/render");

    // ---- 1. 指针渐变剖面标定 ------------------------------------------------
    println!("== 指针渐变剖面（沿轴采样 red 通道，期望 255*0.6*d/85）==");
    let mut worst = 0.0f64;
    for angle in [0.0f64, 45.0, 90.0, 116.37] {
        let dev = calibrate_profile(angle)?;
        println!("  angle {angle:8.2}°  最大偏差 {dev:6.2}/255");
        worst = worst.max(dev);
    }
    println!(
        "  -> 最大偏差 {worst:.2}/255（阈值 4.00）=> {}",
        if worst <= 4.0 {
            needle_impl_label(&cfg, composite)
        } else {
            format!("偏差超标：{}", needle_impl_label(&cfg, composite))
        }
    );

    // ---- 2. 八个状态 --------------------------------------------------------
    println!("\n== 状态截图（{LOGICAL:.0} 逻辑 px × DPR {DPR} = 1144×1144）==");
    for state in &STATES {
        let path = out_dir.join(format!("{}.png", state.name));
        let shot = render_state(state, &path, LOGICAL, DPR)?;
        println!(
            "  {:<9} {:>10.0} kbps  stage={:<8} scale={:<6} -> {}×{}  非黑像素 {}{}",
            state.name,
            state.kbps,
            format!("{:?}", state.stage),
            format!("{:?}", state.scale),
            shot.width,
            shot.height,
            shot.non_black_pixels(),
            if shot.drew { "" } else { "  [!! 未重画]" },
        );
    }

    // ---- 2b. 指针专用剖面图（给 tools/needle_profile_dense.py 做 0.5uu 密集扫描）----
    // 只在显式要时出图：默认产物的文件名/字节不受影响。
    if std::env::var_os("SPEED_GAUGE_NEEDLE_PROFILES").is_some() {
        println!("\n== 指针剖面（只留指针，角度 116.37°，两主题）==");
        for (theme, name) in [
            (GaugeTheme::Dark, "needle-dark"),
            (GaugeTheme::Light, "needle-light"),
        ] {
            let path = out_dir.join(format!("{name}.png"));
            render_needle_profile(theme, 116.37, &path, LOGICAL, DPR)?;
            println!("  {name}.png  theme={theme:?}");
        }
    }

    // ---- 3. 单帧耗时（286×286@DPR1 与 572×572@DPR2）-------------------------
    for (dpr, label) in [(1.0f32, "286×286 @ DPR 1"), (2.0, "572×572 @ DPR 2")] {
        let row = bench_frame(dpr)?;
        println!(
            "\n== 单帧耗时（{label}，release，n=10，指针 {}) ==",
            needle_impl_label(&cfg, composite)
        );
        println!(
            "  渲染 draw_if_needed 中位 {:>7.2}ms  最小 {:>7.2}  最大 {:>7.2}",
            ms(row.draw_median),
            ms(row.draw_min),
            ms(row.draw_max)
        );
        println!(
            "  含指针生成 合计      中位 {:>7.2}ms（生成 {:.2}ms + 渲染 {:.2}ms）",
            ms(row.total_median),
            ms(row.gen_median),
            ms(row.draw_median)
        );
        #[cfg(feature = "needle-raster")]
        println!(
            "  光栅画布重建次数：预热后 {} → 10 帧后 {}（稳态不增 = 零稳态分配）",
            row.resizes_before, row.resizes_after
        );
        #[cfg(not(feature = "needle-raster"))]
        {
            let _ = (row.resizes_before, row.resizes_after);
            println!("  （切片路径没有光栅画布，重建次数 N/A）");
        }
    }

    // ---- 4. 定时器收敛断言 --------------------------------------------------
    println!("\n== 动画泵收敛 ==");
    timer_check()?;

    Ok(())
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn needle_impl_label(
    cfg: &speed_gauge_slint::renderer::SliceCfg,
    composite: SliceComposite,
) -> String {
    #[cfg(feature = "needle-raster")]
    {
        let _ = (cfg, composite);
        "整条光栅化".to_string()
    }
    #[cfg(not(feature = "needle-raster"))]
    {
        format!("{} 片切片 / {composite:?}", cfg.slices)
    }
}

/// 一档 DPR 的测量结果（各 10 帧，取中位 / 最小 / 最大）。
struct BenchRow {
    draw_median: Duration,
    draw_min: Duration,
    draw_max: Duration,
    gen_median: Duration,
    total_median: Duration,
    resizes_before: usize,
    resizes_after: usize,
}

/// 一档 DPR 的单帧测量：每帧 = `snap_to`（生成指针）+ `draw_if_needed`（光栅化整帧）。
fn bench_frame(dpr: f32) -> Result<BenchRow, Box<dyn std::error::Error>> {
    let ui = SpeedGauge::new()?;
    let gauge = GaugeHandle::attach(&ui);
    // 本实例自己的窗口适配器（`WindowSlot::for_component`：按组件身份解析，与创建顺序无关）。
    let slot = WindowSlot::for_component(&ui)?;
    gauge.set_animation_enabled(false);
    gauge.set_wobble_enabled(false);
    gauge.set_stage(GaugeStage::Download);
    gauge.snap_to(862_000.0);
    // 预热：把 DPR 落地并让指针位图按真实 DPR 重建一次（这次重建是重建，不是稳态）。
    warm_up_dpr(&ui, &gauge, &slot, 862_000.0, LOGICAL, dpr)?;

    #[cfg(feature = "needle-raster")]
    let resizes_before = gauge.needle_raster_resizes();
    #[cfg(not(feature = "needle-raster"))]
    let resizes_before = 0usize;

    let mut draws = Vec::with_capacity(10);
    let mut gens = Vec::with_capacity(10);
    for _ in 0..10 {
        let t0 = Instant::now();
        gauge.snap_to(862_000.0);
        let gen = t0.elapsed();
        let shot = shots::render(&ui, &slot, LOGICAL, dpr)?;
        gens.push(gen);
        draws.push(shot.draw_time);
    }

    #[cfg(feature = "needle-raster")]
    let resizes_after = gauge.needle_raster_resizes();
    #[cfg(not(feature = "needle-raster"))]
    let resizes_after = 0usize;

    let total_median = {
        let mut t: Vec<Duration> = draws.iter().zip(&gens).map(|(d, g)| *d + *g).collect();
        t.sort();
        t[5]
    };
    draws.sort();
    gens.sort();
    Ok(BenchRow {
        draw_min: draws[0],
        draw_median: draws[5],
        draw_max: draws[9],
        gen_median: gens[5],
        total_median,
        resizes_before,
        resizes_after,
    })
}

/// 渲染一个验收状态：动画关、微颤关、`snap_to` 精确冻结。
fn render_state(
    state: &State,
    path: &std::path::Path,
    logical: f32,
    dpr: f32,
) -> Result<shots::Shot, Box<dyn std::error::Error>> {
    let ui = SpeedGauge::new()?;
    let gauge = GaugeHandle::attach(&ui);
    let slot = WindowSlot::for_component(&ui)?;
    gauge.set_theme(GaugeTheme::Dark);
    gauge.set_connection_mode(ConnectionMode::Multi);
    gauge.set_motion_mode(MotionMode::Spring);
    gauge.set_animation_enabled(false);
    gauge.set_wobble_enabled(false);
    gauge.set_scale_mode(state.scale);
    // 顺序与 main.ts 的 `[data-stage]` 按钮一致：先 setStage（可能触发归零），再 snapTo。
    gauge.set_stage(state.stage);
    gauge.snap_to(state.kbps);

    warm_up_dpr(&ui, &gauge, &slot, state.kbps, logical, dpr)?;
    let shot = shots::render_to_png(&ui, &slot, path, logical, dpr)?;
    drop(gauge);
    Ok(shot)
}

/// 把 DPR 落到窗口上，再让控制器**重绘一次**指针（之后才真正出图 / 计时）。
///
/// 光栅路径里 `needle-image` 是**命令式**属性：`GaugeHandle::attach` 时窗口还没
/// `set_size` / `dispatch_event(ScaleFactorChanged)`，那一帧按未知 DPR 出的位图会被扔掉
/// 或者干脆退回切片。所以必须先渲染一帧把 DPR 落地，再 `snap_to` 触发一次 `paint`，
/// 让 [`speed_gauge_slint::needle_raster::NeedleRaster`] 读到真实的 `scale_factor()`。
///
/// 切片路径下这次多出来的 `paint` 是幂等的（同角度 ⇒ 同 32 条 path），不影响产物。
fn warm_up_dpr(
    ui: &SpeedGauge,
    gauge: &GaugeHandle,
    slot: &WindowSlot,
    kbps: f64,
    logical: f32,
    dpr: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = shots::render(ui, slot, logical, dpr)?;
    gauge.snap_to(kbps);
    Ok(())
}

/// 指针专用剖面图：清空刻度与读数，只留指针（+ 环形轨道/彩弧，都在 r ≥ 118.5 之外）。
///
/// 用于 `tools/needle_profile_dense.py` 的 0.5uu 密集扫描：指针扫掠范围内只剩 `page`
/// 底色，这正是"不透明预合成逐像素等价"所要求的唯一前提。
fn render_needle_profile(
    theme: GaugeTheme,
    angle: f64,
    path: &std::path::Path,
    logical: f32,
    dpr: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mbps = speed_gauge_slint::scale::value_for_angle(
        &speed_gauge_slint::scale::LABELS_STANDARD,
        angle,
    );
    let ui = SpeedGauge::new()?;
    let gauge = GaugeHandle::attach(&ui);
    let slot = WindowSlot::for_component(&ui)?;
    gauge.set_theme(theme);
    gauge.set_connection_mode(ConnectionMode::Multi);
    gauge.set_motion_mode(MotionMode::Spring);
    gauge.set_animation_enabled(false);
    gauge.set_wobble_enabled(false);
    gauge.set_stage(GaugeStage::Download);
    gauge.snap_to(mbps * 1000.0);
    ui.set_labels(slint::ModelRc::new(slint::VecModel::from(
        Vec::<LabelSpec>::new(),
    )));
    ui.set_number_text("".into());
    let actual = gauge.angle();
    assert!(
        (actual - angle).abs() < 0.05,
        "剖面角没对上：想要 {angle}°，实际 {actual}°"
    );
    warm_up_dpr(&ui, &gauge, &slot, mbps * 1000.0, logical, dpr)?;
    assert!(
        (gauge.angle() - angle).abs() < 0.05,
        "预热后角度跑掉了：{}",
        gauge.angle()
    );
    drop(gauge);
    shots::render_to_png(&ui, &slot, path, logical, dpr)?;
    Ok(())
}

/// 指针渐变剖面：只留指针（清空刻度、清空数字、彩弧隐藏），沿指针轴采样。
///
/// 期望值：hub 处 alpha 0，尖端处 alpha 0.6（= 153/255），沿线**线性**。
/// 因为背景是纯黑不透明，白 alpha=a 的合成结果就是 `RGB = 255a`，所以直接读 red 通道。
fn calibrate_profile(angle: f64) -> Result<f64, Box<dyn std::error::Error>> {
    // 用 valueForAngle 反解出让指针正好停在 `angle` 的读数，再 snapTo 冻结。
    let mbps = speed_gauge_slint::scale::value_for_angle(
        &speed_gauge_slint::scale::LABELS_STANDARD,
        angle,
    );
    let ui = SpeedGauge::new()?;
    let gauge = GaugeHandle::attach(&ui);
    let slot = WindowSlot::for_component(&ui)?;
    gauge.set_animation_enabled(false);
    gauge.set_wobble_enabled(false);
    gauge.set_stage(GaugeStage::Download);
    gauge.snap_to(mbps * 1000.0);
    // 只留指针：清掉刻度与读数（都是 `in` 属性，直接覆盖）。
    ui.set_labels(slint::ModelRc::new(slint::VecModel::from(
        Vec::<LabelSpec>::new(),
    )));
    ui.set_number_text("".into());
    let actual = gauge.angle();
    assert!(
        (actual - angle).abs() < 0.05,
        "标定角没对上：想要 {angle}°，实际 {actual}°"
    );

    warm_up_dpr(&ui, &gauge, &slot, mbps * 1000.0, LOGICAL, DPR)?;
    let shot = shots::render(&ui, &slot, LOGICAL, DPR)?;
    let (sin, cos) = angle.to_radians().sin_cos();

    let mut worst = 0.0f64;
    let mut worst_d = 0u32;
    let mut worst_got = 0f64;
    let mut samples = Vec::new();
    // d ∈ [2, 84] uu：避开根部圆心的抗锯齿与尖端 1uu 边缘。
    for d in 2..=84 {
        let d = d as f64;
        let x = 143.0 + sin * d;
        let y = 143.0 - cos * d;
        let px = (x * DPR as f64).round() as u32;
        let py = (y * DPR as f64).round() as u32;
        let Some(p) = shot.pixel(px, py) else {
            continue;
        };
        // 反预乘后取红通道（背景 alpha=255，此处等价于原值）。
        let got = p.red as f64;
        let expect = 255.0 * 0.6 * (d / 85.0);
        let dev = (got - expect).abs();
        if dev > worst {
            worst = dev;
            worst_d = d as u32;
            worst_got = got;
        }
        samples.push((d, got, expect));
    }
    if std::env::var_os("SHOTS_VERBOSE").is_some() {
        println!(
            "    worst at d={worst_d}uu  got={worst_got:.0}  expect={:.1}",
            255.0 * 0.6 * worst_d as f64 / 85.0
        );
    }
    let _ = &samples;
    Ok(worst)
}

/// 动画泵收敛：`set_speed_kbps` 启动 `Timer`，真实时间泵到收敛后必须自动停表。
fn timer_check() -> Result<(), Box<dyn std::error::Error>> {
    let ui = SpeedGauge::new()?;
    let gauge = GaugeHandle::attach(&ui);
    let slot = WindowSlot::for_component(&ui)?;
    gauge.set_stage(GaugeStage::Download);
    gauge.set_wobble_enabled(false);
    gauge.set_speed_kbps(862_000.0);

    // 先真正渲染一帧，保证窗口存在（Timer 不依赖窗口，但这样更接近真实用法）。
    let _ = shots::render(&ui, &slot, LOGICAL, 1.0)?;

    println!("  启动后 timer_running = {}", gauge.timer_running());
    let mut pumped = 0;
    while pumped < 200 && !gauge.is_settled() {
        std::thread::sleep(Duration::from_millis(16));
        slint::platform::update_timers_and_animations();
        pumped += 1;
    }
    // 收敛后多泵几帧，确认表真的停了（不是"恰好这一帧没跑"）。
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(16));
        slint::platform::update_timers_and_animations();
    }
    let running = gauge.timer_running();
    println!(
        "  收敛于 {} 帧泵送后：angle={:.3}° settled={} timer_running={}",
        pumped,
        gauge.angle(),
        gauge.is_settled(),
        running
    );
    assert!(gauge.is_settled(), "动画没有收敛");
    assert!(!running, "收敛后 Timer 仍在运行（空转刷新）");
    println!("  ✓ 收敛后 Timer 已停");
    Ok(())
}
