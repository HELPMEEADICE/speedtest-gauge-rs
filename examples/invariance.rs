//! 尺寸 / DPR 不变性探针：把同一状态渲染到一组逻辑尺寸与 DPR，交给
//! `tools/invariance.py` 量测后归一化对比。
//!
//! 产物（`comparison/` 下，全部幂等：同输入两次运行字节一致）：
//!
//! | 目录 | 文件名 | 用途 |
//! |---|---|---|
//! | `report/scales/` | `<逻辑边长>-<DPR>.png` | 尺寸不变性（286/300/440/640 @ DPR 1）+ DPR 不变性（572 @ DPR 1/2/3） |
//! | `render-light/` | `<状态>.png` | 浅色主题首个可视化基线（无官方参考图） |
//!
//! 复用 `shots` 的既有仪器（本文件不改 `src/**`）：`render_sized` 出预乘 RGBA，
//! `save_png` 反预乘后落盘。**刻意不写 `comparison/render/`**：那 11 张图另有任务在动。
//!
//! 运行：
//! ```text
//! cargo run --release --example invariance            # 两份产物都出
//! cargo run --release --example invariance -- scales   # 只出尺寸/DPR 矩阵
//! cargo run --release --example invariance -- light    # 只出浅色基线
//! ```

use std::path::{Path, PathBuf};

use speed_gauge_slint::shots::{self, Shot, WindowSlot};
use speed_gauge_slint::{
    ConnectionMode, GaugeHandle, GaugeStage, GaugeTheme, MotionMode, ScaleMode, SpeedGauge,
};

/// 组件坐标系边长（`viewBox 0 0 286 286`）；归一化分母就是它。
const UU: f32 = 286.0;

/// 尺寸不变性：逻辑边长（DPR 固定 1）。
const SIZES: [u32; 4] = [286, 300, 440, 640];

/// DPR 不变性：DPR 1/2/3（逻辑边长固定 572 = 参考图口径 1144 px @ DPR 2）。
const BASE_LOGICAL: u32 = 572;
const DPRS: [u32; 3] = [1, 2, 3];

/// 浅色基线用的状态（与 `examples/shots.rs` 的暗色 7 状态一一对应）。
///
/// `shots.rs` 里另有 `gigabit`，它没有参考图、也不参与像素对拍，故不列入基线。
const LIGHT_STATES: [State; 7] = [
    State::new("idle", GaugeStage::Idle, 0.0, ScaleMode::Fixed),
    State::new("low", GaugeStage::Download, 8_000.0, ScaleMode::Fixed),
    State::new("mid", GaugeStage::Download, 155_610.0, ScaleMode::Fixed),
    State::new("high", GaugeStage::Download, 862_000.0, ScaleMode::Fixed),
    State::new("upload", GaugeStage::Upload, 341_000.0, ScaleMode::Fixed),
    State::new("done", GaugeStage::Done, 862_000.0, ScaleMode::Fixed),
    State::new(
        "extended",
        GaugeStage::Download,
        4_200_000.0,
        ScaleMode::Auto,
    ),
];

/// 一个验收状态的配方（与 `examples/shots.rs` 的 `State` 同构）。
#[derive(Clone, Copy)]
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

impl State {
    /// 常量上下文里构造（数组初始化用）。
    const fn new(name: &'static str, stage: GaugeStage, kbps: f64, scale: ScaleMode) -> Self {
        Self {
            name,
            stage,
            kbps,
            scale,
        }
    }
}

/// 归一化测量的固定状态：`high`（download 862 Mbps，sweep 终点 116.37°）。
///
/// 选它是因为亮弧终点落在右下（远离 9 个刻度文字的墨迹），且 6 个字形
/// （`862.00`）覆盖数字 ink 的左右边界——对拍时两列边界都能量到。
const PROBE_STATE: State = State::new("high", GaugeStage::Download, 862_000.0, ScaleMode::Fixed);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 顺序同 `examples/shots.rs`：平台必须先装，`fontique` 的 shared_collection()
    // 会反向触发平台初始化。
    let _platform = shots::install()?;
    let fonts = speed_gauge_slint::fonts::register_bundled_fonts();
    println!("== 字体注册 ==\n  {fonts}");

    let mode = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("comparison");

    if mode == "all" || mode == "scales" {
        render_scales(&root.join("report").join("scales"))?;
    }
    if mode == "all" || mode == "light" {
        render_light(&root.join("render-light"))?;
    }
    if mode != "all" && mode != "scales" && mode != "light" {
        return Err(format!("未知子命令 {mode:?}：只有 all / scales / light").into());
    }
    Ok(())
}

/// 尺寸矩阵 + DPR 矩阵。
fn render_scales(out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "\n== 尺寸不变性（逻辑 {} @ DPR 1，状态 {}）==",
        SIZES.map(|s| s.to_string()).join(" / "),
        PROBE_STATE.name
    );
    for logical in SIZES {
        let shot = render_one(&PROBE_STATE, out, logical, 1)?;
        println!(
            "  {logical}-1.png  {:>5}×{:<5} 逻辑 {logical}  非黑像素 {}",
            shot.width,
            shot.height,
            shot.non_black_pixels()
        );
    }

    println!(
        "\n== DPR 不变性（逻辑 {BASE_LOGICAL} @ DPR {}）==",
        DPRS.map(|d| d.to_string()).join(" / ")
    );
    for dpr in DPRS {
        let shot = render_one(&PROBE_STATE, out, BASE_LOGICAL, dpr)?;
        println!(
            "  {BASE_LOGICAL}-{dpr}.png  {:>5}×{:<5} DPR {dpr}  非黑像素 {}",
            shot.width,
            shot.height,
            shot.non_black_pixels()
        );
    }
    Ok(())
}

/// 浅色主题 7 状态（口径同参考图：286 逻辑 @ DPR 4 = 1144 px）。
fn render_light(out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n== 浅色基线（286 逻辑 × DPR 4 = 1144×1144，无官方参考图）==");
    for state in &LIGHT_STATES {
        let path = out.join(format!("{}.png", state.name));
        let shot = render_state(state, GaugeTheme::Light, &path, UU, 4.0)?;
        println!(
            "  {:<9} {:>10.0} kbps  -> {}×{}  非黑像素 {}",
            state.name,
            state.kbps,
            shot.width,
            shot.height,
            shot.non_black_pixels()
        );
    }
    Ok(())
}

/// 渲染一个状态到 `<out>/<logical>-<dpr>.png`。
fn render_one(
    state: &State,
    out: &Path,
    logical: u32,
    dpr: u32,
) -> Result<Shot, Box<dyn std::error::Error>> {
    let path = out.join(format!("{logical}-{dpr}.png"));
    render_state(state, GaugeTheme::Dark, &path, logical as f32, dpr as f32)
}

/// 出图：动画关、微颤关、`snap_to` 精确冻结，先暖一帧把 DPR 落到指针光栅上。
fn render_state(
    state: &State,
    theme: GaugeTheme,
    path: &Path,
    logical: f32,
    dpr: f32,
) -> Result<Shot, Box<dyn std::error::Error>> {
    let ui = SpeedGauge::new()?;
    let gauge = GaugeHandle::attach(&ui);
    let slot = WindowSlot::for_component(&ui)?;
    gauge.set_theme(theme);
    gauge.set_connection_mode(ConnectionMode::Multi);
    gauge.set_motion_mode(MotionMode::Spring);
    gauge.set_animation_enabled(false);
    gauge.set_wobble_enabled(false);
    gauge.set_scale_mode(state.scale);
    // 顺序与 `main.ts` 的 `[data-stage]` 按钮一致：先 setStage（可能触发归零），再 snapTo。
    gauge.set_stage(state.stage);
    gauge.snap_to(state.kbps);

    // 暖一帧：`needle-image` 是命令式属性，窗口还没 `set_size`/`ScaleFactorChanged` 时
    // 按未知 DPR 出的位图会被扔掉（见 `examples/shots.rs::warm_up_dpr`）。
    let _ = shots::render_sized(&ui, &slot, (logical, logical), dpr)?;
    gauge.snap_to(state.kbps);

    let shot = shots::render_to_png_sized(&ui, &slot, path, (logical, logical), dpr)?;
    drop(gauge);
    Ok(shot)
}
