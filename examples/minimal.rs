//! 最小嵌入示例：把 `SpeedGauge` 放进一个窗口，用公开 API 推几次速度。
//!
//! 运行：
//!   `cargo run --release --example minimal`            # 有桌面 → 真窗口（约 2 s 后自动关闭）
//!   `SPEED_GAUGE_HEADLESS=1 cargo run --release --example minimal`  # 强制无窗口演示
//!
//! 这里演示了宿主侧的全部必要步骤：
//! 1. `SpeedGauge::new()`
//! 2. `GaugeHandle::attach(&ui)` —— 一个实例一个控制器，多实例互不影响
//! 3. 调 `set_stage` / `set_speed_kbps` / `set_scale_mode` / `set_connection_mode`
//! 4. 交给 Slint 事件循环（动画泵是控制器内部的 `slint::Timer`，宿主不用管）

use std::time::Duration;

use slint::ComponentHandle;
use speed_gauge_slint::{
    ConnectionMode, GaugeHandle, GaugeStage, GaugeTheme, MotionMode, ScaleMode, SpeedGauge,
};

/// 演示用速度序列（kbps），模拟一次真实的 ping → download → upload → done。
const DEMO: [(GaugeStage, f64); 6] = [
    (GaugeStage::Ping, 0.0),
    (GaugeStage::Download, 120_000.0),
    (GaugeStage::Download, 480_000.0),
    (GaugeStage::Download, 862_000.0),
    (GaugeStage::Upload, 341_000.0),
    (GaugeStage::Done, 862_000.0),
];

fn main() {
    if std::env::var_os("SPEED_GAUGE_HEADLESS").is_some() {
        if let Err(e) = run_headless() {
            eprintln!("[minimal] headless 演示失败：{e}");
        }
        return;
    }
    match run_windowed() {
        Ok(()) => {}
        Err(e) => {
            eprintln!("[minimal] 窗口后端不可用（{e}），改为无窗口演示");
            if let Err(e2) = run_headless() {
                eprintln!("[minimal] headless 演示也失败：{e2}");
            }
        }
    }
}

/// 真窗口演示：572 逻辑 px（= 参考口径 `#gauge-stage` 的宽度），2 s 后自动关闭。
fn run_windowed() -> Result<(), Box<dyn std::error::Error>> {
    let ui = SpeedGauge::new()?;
    let gauge = GaugeHandle::attach(&ui);

    // 初始配置。
    gauge.set_theme(GaugeTheme::Dark);
    gauge.set_connection_mode(ConnectionMode::Multi);
    gauge.set_scale_mode(ScaleMode::Auto);
    gauge.set_motion_mode(MotionMode::Spring);
    gauge.set_wobble_enabled(true);

    // 根元素是 Rectangle（不是 Window），窗口尺寸由宿主决定。
    ui.window().set_size(slint::LogicalSize::new(572.0, 572.0));

    // 每 60 ms 推一个采样。
    let pusher = {
        let gauge = gauge.clone();
        let timer = slint::Timer::default();
        let mut step = 0usize;
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(60),
            move || {
                let (stage, kbps) = DEMO[step.min(DEMO.len() - 1)];
                if step == 0 {
                    gauge.set_stage(stage);
                } else if step < DEMO.len() {
                    gauge.set_stage(stage);
                    gauge.set_speed_kbps(kbps);
                    println!(
                        "[minimal] step {step}: stage={stage:?} kbps={kbps:.0} \
                         -> angle={:.3}° number={}",
                        gauge.angle(),
                        gauge.number_text()
                    );
                }
                step += 1;
            },
        );
        timer
    };

    // 2 s 后自动关闭。
    slint::Timer::single_shot(Duration::from_millis(2000), || {
        println!("[minimal] 2 s 到，自动关闭窗口");
        let _ = slint::quit_event_loop();
    });

    ui.show()?;
    slint::run_event_loop()?;

    println!(
        "[minimal] 结束：angle={:.3}° number={} timer_running={}",
        gauge.angle(),
        gauge.number_text(),
        gauge.timer_running()
    );
    drop(pusher);
    Ok(())
}

/// 无窗口演示：软件渲染器 + 手动推进，验证同一套公开 API 在没有桌面的机器上也能跑。
fn run_headless() -> Result<(), Box<dyn std::error::Error>> {
    use speed_gauge_slint::shots;

    let _platform = shots::install()?;
    let ui = SpeedGauge::new()?;
    // 该实例自己的窗口适配器：按组件身份解析（`WindowSlot::for_component`），
    // 与"谁最后被创建"无关 —— 同一线程里多个组件并存也各拿各的窗口。
    let slot = shots::WindowSlot::for_component(&ui)?;
    let gauge = GaugeHandle::attach(&ui);
    gauge.set_theme(GaugeTheme::Dark);
    gauge.set_stage(GaugeStage::Download);

    let mut stable = 0;
    for step in 0..6 {
        let kbps = 120_000.0 + step as f64 * 140_000.0;
        gauge.set_speed_kbps(kbps);
        // 手动推进 40 帧（确定性，不依赖真实时钟）。
        for _ in 0..40 {
            gauge.step(1.0 / 60.0);
        }
        println!(
            "[minimal] step {step}: kbps={kbps:.0} -> angle={:.3}° number={} \
             lit={} ring={}",
            gauge.angle(),
            gauge.number_text(),
            gauge.lit_count(),
            gauge.ring_visible_count()
        );
        if gauge.is_settled() {
            stable += 1;
        }
    }
    println!(
        "[minimal] 收敛次数 {stable}/6，timer_running={}",
        gauge.timer_running()
    );

    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("comparison/render/minimal-headless.png");
    let shot = shots::render_to_png(&ui, &slot, &out, 286.0, 2.0)?;
    println!(
        "[minimal] 已写出 {}（{}×{}）",
        out.display(),
        shot.width,
        shot.height
    );
    Ok(())
}
