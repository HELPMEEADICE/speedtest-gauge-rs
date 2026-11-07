//! 视觉回归的**绝对不变量**集成测试：不依赖外部参考图目录，只断言从参考图反解出来、
//! 与机器/字体无关的硬指标。全部数据由 `shots::render_sized` 现场渲染得到，
//! 然后在本文件里自己扫像素——不读 `comparison/render/*.png`，也不读
//! `C:\oss\Project\speedtest\report`（可移植性要求）。
//!
//! ## 容差出处
//!
//! | 常量 | 值 | 出处 |
//! |---|---|---|
//! | [`TOL_SWEEP_DEG`] | 0.2° | `docs/visual-targets.md` §6「验收线定为 sweep 终点 \|Δ\| ≤ 0.2°」 |
//! | [`TOL_RING_UU`] | 0.7 uu | `docs/visual-targets.md` §1：内半径实测散布 118.3…118.8、外半径 141.55…141.7，±0.7 完整覆盖散布并留 1 个抗锯齿像素余量 |
//! | [`TOL_BBOX_PX`] | 1.5 px @1144 | `docs/visual-targets.md` §2 的 ink bbox 表 + 原任务书「±1.5 物理像素 @1144」 |
//! | [`TOL_CENTRE_UU`] | 0.5 uu | 原任务书「286 / 572 / 640 三档归一化后数字 ink 中心一致（±0.5 uu）」 |
//!
//! ## 为什么用 `OnceLock` 包一层
//!
//! Slint 的平台是**进程级全局**的（`set_platform` 只能成功一次），而 `shots::install()`
//! 把平台句柄记在**线程局部**里；libtest 默认每个 `#[test]` 一个线程，于是只有第一个
//! 线程能装平台。这里把"渲染全部帧"收进一个 `OnceLock`：第一个到达的测试线程装平台并
//! 把结果拷成 `Vec<u8>`，其余测试只读这份普通数据，不再碰 Slint——并行跑也不会抢平台。

use std::sync::OnceLock;

use speed_gauge_slint::shots::{self, WindowSlot};
use speed_gauge_slint::{
    ConnectionMode, GaugeHandle, GaugeStage, GaugeTheme, MotionMode, ScaleMode, SpeedGauge,
};

// ---------------------------------------------------------------- 常量与容差

/// 组件坐标系边长（`viewBox 0 0 286 286`）。
const UU: f64 = 286.0;
/// 参考图口径：286 逻辑 px × DPR 4 = 1144 px（`docs/visual-targets.md` §0）。
const DPR_REF: f32 = 4.0;
/// 参考图边长（物理像素）。
const REF_WIDTH: u32 = 1144;
/// 刻度文字环半径（`docs/visual-targets.md` §4）。
const LABEL_RADIUS: f64 = 97.0;
/// 9 档刻度值（`scale::LABELS_STANDARD`）。
const LABELS: [u32; 9] = [0, 5, 10, 50, 100, 250, 500, 750, 1000];
/// 亮弧扫描半径（`docs/visual-targets.md` §6）。
const SCAN_RADIUS: f64 = 130.0;
/// 亮弧/墨迹阈值（`docs/visual-targets.md` §6：轨道 luma≈39，故 60 安全）。
const LIT_LUMA: f64 = 60.0;
/// 数字墨迹阈值（`measure_reference.bright`）。
const BRIGHT_LUMA: f64 = 200.0;
/// 「非背景」阈值（`measure_reference.ring_scan`：`luma > 8`）。
const BACKGROUND_LUMA: f64 = 8.0;
/// 数字 ink 取样框（uu，`measure_reference` 的 `NUMBER_BOX`）。
const NUMBER_BOX: (f64, f64, f64, f64) = (100.0, 205.0, 190.0, 248.0);
/// 单位行取样框（uu，`verify_states.UNIT_BOX`）。
const UNIT_BOX: (f64, f64, f64, f64) = (100.0, 248.0, 190.0, 275.0);

/// sweep 终点角容差（`docs/visual-targets.md` §6 验收线）。
const TOL_SWEEP_DEG: f64 = 0.2;
/// 环带内外半径容差（`docs/visual-targets.md` §1 的实测散布 + 1 px 余量）。
const TOL_RING_UU: f64 = 0.7;
/// 数字 ink bbox 容差（物理像素 @1144；`docs/visual-targets.md` §2）。
const TOL_BBOX_PX: f64 = 1.5;
/// 尺寸不变性：数字 ink 中心归一化容差（uu）。
const TOL_CENTRE_UU: f64 = 0.5;

/// 环带内/外半径的理论值（`docs/visual-targets.md` §1，`stroke-width 23` @ r=130）。
const RING_INNER_UU: f64 = 118.5;
/// 环带外半径理论值。
const RING_OUTER_UU: f64 = 141.5;
/// 颜色 token：背景（暗色主题）。
const COLOR_BACKGROUND: [u8; 3] = [0, 0, 0];
/// 颜色 token：轨道 `#26273b`。
const COLOR_TRACK: [u8; 3] = [38, 39, 59];
/// 颜色 token：未点亮的刻度 `#666666`。
const COLOR_LABEL_OFF: [u8; 3] = [102, 102, 102];
/// 颜色 token：点亮的刻度 `#ffffff`。
const COLOR_LABEL_ON: [u8; 3] = [255, 255, 255];
/// 颜色 token：download 强调色（`model::ACCENT_DOWNLOAD`，像素实测值）。
const COLOR_ACCENT_DOWNLOAD: [u8; 3] = [0, 197, 189];
/// 颜色 token：upload 强调色（`model::ACCENT_UPLOAD`，像素实测值）。
const COLOR_ACCENT_UPLOAD: [u8; 3] = [180, 90, 237];

// ---------------------------------------------------------------- 状态表

/// 一个验收状态的配方 + 它的 sweep 终点期望值。
struct Case {
    /// 状态名。
    name: &'static str,
    /// 阶段。
    stage: GaugeStage,
    /// 读数（kbps）。
    kbps: f64,
    /// 刻度模式。
    scale: ScaleMode,
    /// sweep 终点角（度）；`None` = 无亮弧。
    sweep: Option<f64>,
    /// 数字 ink bbox 期望值（uu），来自 `docs/visual-targets.md` §2。
    bbox: [f64; 4],
}

impl Case {
    /// 常量上下文里构造。
    const fn new(
        name: &'static str,
        stage: GaugeStage,
        kbps: f64,
        scale: ScaleMode,
        sweep: Option<f64>,
        bbox: [f64; 4],
    ) -> Self {
        Self {
            name,
            stage,
            kbps,
            scale,
            sweep,
            bbox,
        }
    }
}

/// 7 个有参考的状态：期望值全部取自 `docs/visual-targets.md`（§2 的 bbox、§6 的 sweep）。
const CASES: [Case; 7] = [
    Case::new(
        "idle",
        GaugeStage::Idle,
        0.0,
        ScaleMode::Fixed,
        None,
        [120.75, 207.25, 165.00, 235.25],
    ),
    Case::new(
        "low",
        GaugeStage::Download,
        8_000.0,
        ScaleMode::Fixed,
        Some(-81.000),
        [120.50, 207.25, 165.00, 235.25],
    ),
    Case::new(
        "mid",
        GaugeStage::Download,
        155_610.0,
        ScaleMode::Fixed,
        Some(12.512),
        [106.50, 208.00, 179.25, 235.25],
    ),
    Case::new(
        "high",
        GaugeStage::Download,
        862_000.0,
        ScaleMode::Fixed,
        Some(116.370),
        [106.50, 207.25, 179.00, 235.25],
    ),
    Case::new(
        "upload",
        GaugeStage::Upload,
        341_000.0,
        ScaleMode::Fixed,
        Some(46.035),
        [106.50, 207.25, 179.00, 235.25],
    ),
    Case::new(
        "done",
        GaugeStage::Done,
        862_000.0,
        ScaleMode::Fixed,
        Some(116.370),
        [106.50, 207.25, 179.00, 235.25],
    ),
    Case::new(
        "extended",
        GaugeStage::Download,
        4_200_000.0,
        ScaleMode::Auto,
        Some(95.400),
        [100.00, 207.25, 189.25, 237.75],
    ),
];

/// 尺寸不变性用的三档逻辑边长（原任务书）。
const SIZES: [u32; 3] = [286, 572, 640];

// ---------------------------------------------------------------- 帧与渲染

/// 一帧非预乘 RGBA（拷成普通字节，可跨线程读）。
struct Frame {
    /// 物理宽。
    width: u32,
    /// 物理高。
    height: u32,
    /// 逐像素 RGBA，非预乘。
    rgba: Vec<u8>,
}

impl Frame {
    /// 从 `shots::Shot` 拷出来（反预乘算法与 `shots::save_png` 完全一致）。
    fn from_shot(shot: &shots::Shot) -> Self {
        let mut rgba = Vec::with_capacity((shot.width * shot.height * 4) as usize);
        for p in shot.buffer.as_slice() {
            let a = p.alpha;
            let un = |c: u8| -> u8 {
                if a == 0 {
                    0
                } else if a == 255 {
                    c
                } else {
                    ((c as u32 * 255 + (a as u32 / 2)) / a as u32).min(255) as u8
                }
            };
            rgba.extend_from_slice(&[un(p.red), un(p.green), un(p.blue), a]);
        }
        Self {
            width: shot.width,
            height: shot.height,
            rgba,
        }
    }

    /// 物理像素 / uu。
    fn scale(&self) -> f64 {
        f64::from(self.width) / UU
    }

    /// 取一个物理像素；越界返回 `None`。
    fn px(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y * self.width + x) * 4) as usize;
        Some([
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ])
    }

    /// 某像素的 luma（系数与 `measure_reference.luma` 一致）；越界按背景 0 处理。
    fn luma(&self, x: u32, y: u32) -> f64 {
        match self.px(x, y) {
            Some(p) => {
                0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2])
            }
            None => 0.0,
        }
    }

    /// 在 uu 坐标处取最近像素的 luma（与 `Frame::px` 同一套取整规则）。
    fn px_at_uu(&self, ux: f64, uy: f64) -> Option<[u8; 4]> {
        let s = self.scale();
        let x = (ux * s).round();
        let y = (uy * s).round();
        if x < 0.0 || y < 0.0 {
            return None;
        }
        self.px(x as u32, y as u32)
    }

    /// 极坐标取点（0° 朝上、顺时针，圆心在图像中心）。
    fn luma_at_polar(&self, angle_deg: f64, radius_uu: f64) -> f64 {
        let rad = (angle_deg - 90.0).to_radians();
        let cx = f64::from(self.width) / 2.0;
        let cy = f64::from(self.height) / 2.0;
        let s = self.scale();
        let x = (cx + radius_uu * s * rad.cos()).round();
        let y = (cy + radius_uu * s * rad.sin()).round();
        if x < 0.0 || y < 0.0 {
            return 0.0;
        }
        self.luma(x as u32, y as u32)
    }
}

/// 全部现场渲染的产物。
struct Artifacts {
    /// 7 个状态 @ 286 逻辑 × DPR 4（= 1144 px）。
    states: Vec<(&'static str, Frame)>,
    /// `high` 的第二次渲染（确定性断言用）。
    high_again: Frame,
    /// 三档逻辑尺寸的 `high`（尺寸不变性断言用）。
    sizes: Vec<(u32, Frame)>,
}

impl Artifacts {
    /// 按名字取状态帧。
    fn state(&self, name: &str) -> &Frame {
        let found = self.states.iter().find(|(n, _)| *n == name);
        assert!(found.is_some(), "没有渲染状态 {name}");
        match found {
            Some((_, frame)) => frame,
            None => unreachable!(),
        }
    }
}

/// 渲染一个状态（动画关、微颤关、`snap_to` 冻结）。
fn capture(case: &Case, logical: u32, dpr: f32) -> Result<Frame, Box<dyn std::error::Error>> {
    let ui = SpeedGauge::new()?;
    let gauge = GaugeHandle::attach(&ui);
    let slot = WindowSlot::for_component(&ui)?;
    gauge.set_theme(GaugeTheme::Dark);
    gauge.set_connection_mode(ConnectionMode::Multi);
    gauge.set_motion_mode(MotionMode::Spring);
    gauge.set_animation_enabled(false);
    gauge.set_wobble_enabled(false);
    gauge.set_scale_mode(case.scale);
    // 顺序与 `main.ts` 的 `[data-stage]` 按钮一致：先 setStage（可能归零），再 snapTo。
    gauge.set_stage(case.stage);
    gauge.snap_to(case.kbps);

    // 暖帧：`needle-image` 是命令式属性，窗口还没落地 DPR 时那一帧的指针位图会被扔掉
    // （见 `examples/shots.rs::warm_up_dpr`）。
    let size = (logical as f32, logical as f32);
    let _ = shots::render_sized(&ui, &slot, size, dpr)?;
    gauge.snap_to(case.kbps);
    let shot = shots::render_sized(&ui, &slot, size, dpr)?;
    let frame = Frame::from_shot(&shot);
    drop(gauge);
    Ok(frame)
}

/// 全量渲染一次（整个进程只跑一次；见文件头的 `OnceLock` 说明）。
fn build() -> Result<Artifacts, Box<dyn std::error::Error>> {
    // 平台必须先装：`fontique` 的 shared_collection() 会反向触发平台初始化。
    let _platform = shots::install()?;
    speed_gauge_slint::fonts::register_bundled_fonts();

    let mut states = Vec::with_capacity(CASES.len());
    for case in &CASES {
        states.push((case.name, capture(case, UU as u32, DPR_REF)?));
    }
    let high = &CASES[3];
    let high_again = capture(high, UU as u32, DPR_REF)?;
    let mut sizes = Vec::with_capacity(SIZES.len());
    for logical in SIZES {
        sizes.push((logical, capture(high, logical, 1.0)?));
    }
    Ok(Artifacts {
        states,
        high_again,
        sizes,
    })
}

/// 取（或首次构建）全部产物。
fn artifacts() -> &'static Artifacts {
    static CELL: OnceLock<Result<Artifacts, String>> = OnceLock::new();
    let cell = CELL.get_or_init(|| build().map_err(|e| e.to_string()));
    match cell {
        Ok(a) => a,
        Err(msg) => panic!("渲染失败：{msg}"),
    }
}

// ---------------------------------------------------------------- 量测工具

/// 沿 r=130 uu 等角扫描（步长 0.05°，0=正上顺时针），返回最后一个 luma>60 的角度。
///
/// 与 `measure_reference.sweep_end_angle` 逐位同口径（同步长、同阈值、最近像素采样），
/// 这样测试里断言的期望值（`docs/visual-targets.md` §6）与既有仪器可比。
fn sweep_end(frame: &Frame) -> Option<f64> {
    let mut last: Option<f64> = None;
    let mut angle = -135.0_f64;
    while angle <= 135.0 {
        if frame.luma_at_polar(angle, SCAN_RADIUS) > LIT_LUMA {
            last = Some(angle);
        }
        angle += 0.05;
    }
    last
}

/// 径向扫描：返回 (第一个非背景半径, 最后一个非背景半径)，uu。
fn ring_radii(frame: &Frame, angle_deg: f64) -> (Option<f64>, f64) {
    let mut inner: Option<f64> = None;
    let mut outer = 0.0_f64;
    let mut r = 90.0_f64;
    while r <= 175.0 {
        if frame.luma_at_polar(angle_deg, r) > BACKGROUND_LUMA {
            if inner.is_none() {
                inner = Some(r);
            }
            outer = r;
        }
        r += 0.05;
    }
    (inner, outer)
}

/// 墨迹 bbox（**物理像素**，右/下边界取 +1 外沿，与 `measure_reference.ink_bbox` 同口径）。
fn ink_bbox_px(frame: &Frame, box_uu: (f64, f64, f64, f64), threshold: f64) -> [f64; 4] {
    let s = frame.scale();
    let x0 = (box_uu.0 * s).round() as u32;
    let y0 = (box_uu.1 * s).round() as u32;
    let x1 = ((box_uu.2 * s).round() as u32).min(frame.width);
    let y1 = ((box_uu.3 * s).round() as u32).min(frame.height);
    let mut bb = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for y in y0..y1 {
        for x in x0..x1 {
            if frame.luma(x, y) > threshold {
                bb[0] = bb[0].min(f64::from(x));
                bb[1] = bb[1].min(f64::from(y));
                bb[2] = bb[2].max(f64::from(x + 1));
                bb[3] = bb[3].max(f64::from(y + 1));
            }
        }
    }
    assert!(
        bb[0].is_finite() && bb[2].is_finite(),
        "取样框 {box_uu:?} 里没有墨迹"
    );
    bb
}

/// 覆盖率重心（uu）：权重 = luma，`< 2/255` 归零（与 `tools/invariance.py` 同式）。
fn ink_centroid_uu(frame: &Frame, box_uu: (f64, f64, f64, f64)) -> (f64, f64) {
    let s = frame.scale();
    let x0 = (box_uu.0 * s).round() as u32;
    let y0 = (box_uu.1 * s).round() as u32;
    let x1 = ((box_uu.2 * s).round() as u32).min(frame.width);
    let y1 = ((box_uu.3 * s).round() as u32).min(frame.height);
    let mut total = 0.0;
    let mut sx = 0.0;
    let mut sy = 0.0;
    for y in y0..y1 {
        for x in x0..x1 {
            let w = frame.luma(x, y);
            if w < 2.0 {
                continue;
            }
            total += w;
            sx += w * f64::from(x);
            sy += w * f64::from(y);
        }
    }
    assert!(total > 0.0, "取样框 {box_uu:?} 里没有覆盖");
    // +0.5：像素索引 → 像素中心。
    ((sx / total + 0.5) / s, (sy / total + 0.5) / s)
}

/// 某档刻度的取样框（uu）。
fn label_box(value_index: usize) -> (f64, f64, f64, f64) {
    let angle = -135.0 + (value_index as f64) * 33.75;
    let rad = (angle - 90.0).to_radians();
    let lx = 143.0 + LABEL_RADIUS * rad.cos();
    let ly = 143.0 + LABEL_RADIUS * rad.sin();
    // ±13×±8 而不是 ±18×±10：`1000` 的框（x 198.6…224.6）必须避开中央数字的墨迹右缘。
    // 用 Gauge Mono 时数字右缘约 179 uu，宽松的框也没问题；但缺这个专有字体时数字回退到
    // 系统等宽、宽度 +42.6%，墨迹右缘会顶到 ≈194 uu，正好插进 ±18 的框里造成假"点亮"。
    // 收紧后既不丢刻度自身的墨迹（`1000` 墨迹宽 33 uu，±13 覆盖中间 78%），也不再受数字影响。
    (lx - 13.0, ly - 8.0, lx + 13.0, ly + 8.0)
}

/// 数一下框里有多少个**精确等于** `color` 的像素。
fn count_exact(frame: &Frame, box_uu: (f64, f64, f64, f64), color: [u8; 3]) -> usize {
    let s = frame.scale();
    let x0 = (box_uu.0 * s).round() as u32;
    let y0 = (box_uu.1 * s).round() as u32;
    let x1 = ((box_uu.2 * s).round() as u32).min(frame.width);
    let y1 = ((box_uu.3 * s).round() as u32).min(frame.height);
    let mut hits = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if let Some(p) = frame.px(x, y) {
                if p[0] == color[0] && p[1] == color[1] && p[2] == color[2] {
                    hits += 1;
                }
            }
        }
    }
    hits
}

/// 「这一档是点亮还是熄灭」的判定：比较两种精确色的像素数，要求**主导比 ≥ 20**。
///
/// 为什么不断言"另一种色为 0 个"：抗锯齿会把某个边缘像素恰好混成另一档的精确色
/// （实测 `high` 的 `750` 里就躺着 1 个 `#666666`），那是个例而非状态错误。
/// 主导比判法对单像素巧合免疫，又足够严格——真正的熄灭/点亮是 600…2400 像素的区别。
const DOMINANCE: usize = 20;

/// 断言某档刻度是「点亮」。
fn assert_lit(frame: &Frame, value: u32) {
    let idx = LABELS
        .iter()
        .position(|v| *v == value)
        .unwrap_or_else(|| panic!("刻度表里没有 {value}"));
    let on = count_exact(frame, label_box(idx), COLOR_LABEL_ON);
    let off = count_exact(frame, label_box(idx), COLOR_LABEL_OFF);
    assert!(
        on > off * DOMINANCE,
        "刻度 {value} 应点亮：亮色 {on} px vs 熄灭色 {off} px（主导比需 > {DOMINANCE}）"
    );
}

/// 断言某档刻度是「熄灭」。
fn assert_dim(frame: &Frame, value: u32) {
    let idx = LABELS
        .iter()
        .position(|v| *v == value)
        .unwrap_or_else(|| panic!("刻度表里没有 {value}"));
    let on = count_exact(frame, label_box(idx), COLOR_LABEL_ON);
    let off = count_exact(frame, label_box(idx), COLOR_LABEL_OFF);
    assert!(
        off > on * DOMINANCE,
        "刻度 {value} 应熄灭：熄灭色 {off} px vs 亮色 {on} px（主导比需 > {DOMINANCE}）"
    );
}

// ---------------------------------------------------------------- 断言 1：sweep 终点角

#[test]
fn sweep_end_angles_match_reference() {
    let art = artifacts();
    for case in &CASES {
        let frame = art.state(case.name);
        let got = sweep_end(frame);
        match (got, case.sweep) {
            (None, None) => {}
            (Some(g), Some(want)) => assert!(
                (g - want).abs() <= TOL_SWEEP_DEG,
                "{}: sweep 终点 {g:.3}f° vs 期望 {want:.3}f°，差 {:.3}f° > ±{TOL_SWEEP_DEG}°",
                case.name,
                (g - want).abs()
            ),
            (None, Some(want)) => panic!("{}: 期望亮弧到 {want:.3}f°，实际无亮弧", case.name),
            (Some(g), None) => panic!("{}: 期望无亮弧，实际亮到 {g:.3}f°", case.name),
        }
    }
}

// ---------------------------------------------------------------- 断言 2：环几何

#[test]
fn ring_band_covers_documented_radii() {
    let art = artifacts();
    // 用 idle：没有亮弧、指针朝左下（-135°），±60° 的射线只穿过轨道，最干净。
    let frame = art.state("idle");
    for angle in [-60.0_f64, 60.0] {
        let (inner, outer) = ring_radii(frame, angle);
        let inner = inner.unwrap_or_else(|| panic!("{angle}°: 没扫到环带内缘"));
        assert!(
            (inner - RING_INNER_UU).abs() <= TOL_RING_UU,
            "{angle}°: 环带内缘 {inner:.3} uu vs 理论 {RING_INNER_UU} uu，超 ±{TOL_RING_UU} uu"
        );
        assert!(
            (outer - RING_OUTER_UU).abs() <= TOL_RING_UU,
            "{angle}°: 环带外缘 {outer:.3} uu vs 理论 {RING_OUTER_UU} uu，超 ±{TOL_RING_UU} uu"
        );
    }

    // 带内：轨道色 + 全不透明；带外（内外两侧）：背景色。
    let s = frame.scale();
    let (cx, cy) = (f64::from(frame.width) / 2.0, f64::from(frame.height) / 2.0);
    for angle in [-60.0_f64, 60.0] {
        let rad = (angle - 90.0_f64).to_radians();
        let sample = |r: f64| -> [u8; 4] {
            let x = (cx + r * s * rad.cos()).round() as u32;
            let y = (cy + r * s * rad.sin()).round() as u32;
            frame.px(x, y).unwrap_or([0, 0, 0, 0])
        };
        let mid = sample(SCAN_RADIUS);
        assert_eq!(
            [mid[0], mid[1], mid[2]],
            COLOR_TRACK,
            "{angle}° r=130 uu 处应是轨道色 {COLOR_TRACK:?}"
        );
        assert_eq!(mid[3], 255, "{angle}° 轨道处必须不透明");
        for r in [115.0_f64, 145.0] {
            let out = sample(r);
            assert_eq!(
                [out[0], out[1], out[2]],
                COLOR_BACKGROUND,
                "{angle}° r={r} uu 在环带外，应是背景色"
            );
            assert_eq!(out[3], 255, "背景也必须不透明（组件整幅铺满）");
        }
    }

    // 四个角是背景（背景矩形铺满整幅）。
    for (x, y) in [
        (0_u32, 0_u32),
        (frame.width - 1, 0),
        (0, frame.height - 1),
        (frame.width - 1, frame.height - 1),
    ] {
        let p = frame.px(x, y).expect("角落像素必须在图内");
        assert_eq!([p[0], p[1], p[2]], COLOR_BACKGROUND, "({x},{y}) 应是背景");
        assert_eq!(p[3], 255, "({x},{y}) 背景必须不透明");
    }
}

// ---------------------------------------------------------------- 断言 3：数字 ink bbox

/// 中央数字的度量是用 `Gauge Mono` 标定的，而它是 Ookla 的专有字体、**不随仓库分发**
/// （见 `NOTICE.md` §3.2）。缺它时 Slint 会回退到系统等宽字体，墨迹宽度约 +42.6%，
/// 于是下面这两条按字体度量写死的断言在 CI（干净 clone）上必然失败——那不代表实现坏了。
/// 所以这里显式跳过并打印原因，而不是把容差放宽到失去意义。
///
/// ⚠️ 只能在 `artifacts()` **之后**调用：字体注册必须在 `shots::install()` 装好平台之后
/// 进行（`fontique` 的 shared_collection 会反向触发平台初始化），否则 `OnceLock` 会把
/// "平台还没装时注册失败"的坏结果缓存住，连带所有字体相关断言一起挂。
fn gauge_mono_available() -> bool {
    speed_gauge_slint::fonts::register_bundled_fonts()
        .sources
        .iter()
        .any(|s| s.contains("gaugemono"))
}

macro_rules! skip_without_gauge_mono {
    () => {
        if !gauge_mono_available() {
            eprintln!(
                "SKIP: 未找到 .local-fonts/gaugemono.ttf（Ookla 专有字体不随仓库分发），\
                 数字字形度量断言跳过。要跑这两条请把该字体放到 crate 根目录的 .local-fonts/ 下。"
            );
            return;
        }
    };
}

#[test]
fn number_ink_bbox_matches_documented_table() {
    let art = artifacts();
    skip_without_gauge_mono!();
    for case in &CASES {
        let frame = art.state(case.name);
        assert_eq!(
            frame.width, REF_WIDTH,
            "{}: 渲染口径必须是 1144 px",
            case.name
        );
        let got = ink_bbox_px(frame, NUMBER_BOX, BRIGHT_LUMA);
        for (idx, name) in ["x0", "y0", "x1", "y1"].iter().enumerate() {
            let want = case.bbox[idx] * 4.0; // uu → 物理 px @1144
            assert!(
                (got[idx] - want).abs() <= TOL_BBOX_PX,
                "{}: 数字 ink bbox {name} = {:.2} px vs 期望 {:.2} px，差 {:.2} px > ±{TOL_BBOX_PX} px",
                case.name,
                got[idx],
                want,
                (got[idx] - want).abs()
            );
        }
    }
}

// ---------------------------------------------------------------- 断言 4：刻度点亮前缀

#[test]
fn tick_prefix_lights_up_in_order() {
    let art = artifacts();
    // high（862 Mbps）：亮到 750，1000 仍熄灭（docs §5：亮起的是"不超过当前位置"的档位）。
    let high = art.state("high");
    assert_lit(high, 750);
    assert_dim(high, 1000);

    // idle：一档都不亮，`0` 是熄灭色。
    let idle = art.state("idle");
    for value in LABELS {
        assert_dim(idle, value);
    }

    // mid（155.61 Mbps）：亮到 100，250 起熄灭。
    let mid = art.state("mid");
    assert_lit(mid, 100);
    assert_dim(mid, 250);
}

// ---------------------------------------------------------------- 断言 5：颜色 token

#[test]
fn colour_tokens_are_exact() {
    let art = artifacts();

    // 强调色箭头：download 用 (0,197,189)，upload 用 (180,90,237)，互不串色。
    let download = art.state("high");
    let upload = art.state("upload");
    assert!(
        count_exact(download, UNIT_BOX, COLOR_ACCENT_DOWNLOAD) > 100,
        "download: 单位行应含 download 强调色 {COLOR_ACCENT_DOWNLOAD:?}"
    );
    assert_eq!(
        count_exact(download, UNIT_BOX, COLOR_ACCENT_UPLOAD),
        0,
        "download: 单位行不该出现 upload 强调色"
    );
    assert!(
        count_exact(upload, UNIT_BOX, COLOR_ACCENT_UPLOAD) > 100,
        "upload: 单位行应含 upload 强调色 {COLOR_ACCENT_UPLOAD:?}"
    );
    assert_eq!(
        count_exact(upload, UNIT_BOX, COLOR_ACCENT_DOWNLOAD),
        0,
        "upload: 单位行不该出现 download 强调色"
    );

    // 轨道色：idle 的 r=130、0°（正上）落在未点亮轨道上。
    let idle = art.state("idle");
    assert_eq!(
        frame_rgb(idle, 0.0, SCAN_RADIUS),
        COLOR_TRACK,
        "idle 的 0° 环带应是轨道色"
    );
}

/// 极坐标取 RGB。
fn frame_rgb(frame: &Frame, angle_deg: f64, radius_uu: f64) -> [u8; 3] {
    let rad = (angle_deg - 90.0).to_radians();
    let cx = f64::from(frame.width) / 2.0 / frame.scale();
    let cy = f64::from(frame.height) / 2.0 / frame.scale();
    let p = frame
        .px_at_uu(cx + radius_uu * rad.cos(), cy + radius_uu * rad.sin())
        .expect("极坐标采样越界");
    [p[0], p[1], p[2]]
}

// ---------------------------------------------------------------- 断言 6/7：确定性

#[test]
fn render_is_deterministic() {
    let art = artifacts();
    let high = art.state("high");
    assert_eq!(
        (high.width, high.height, high.rgba.len()),
        (
            art.high_again.width,
            art.high_again.height,
            art.high_again.rgba.len()
        ),
        "两次渲染的尺寸必须一致"
    );
    assert!(
        high.rgba == art.high_again.rgba,
        "同一状态渲染两次，缓冲必须逐字节相同（有隐藏的随机/时间依赖）"
    );
}

#[test]
fn high_and_done_render_identically() {
    let art = artifacts();
    let high = art.state("high");
    let done = art.state("done");
    assert_eq!(
        (high.width, high.height),
        (done.width, done.height),
        "high 与 done 的尺寸必须一致"
    );
    assert!(
        high.rgba == done.rgba,
        "high 与 done 必须逐字节相同（done = 下载变体 + 强调色 + 指针不回零，\
         参考图 local-done.png 与 local-high.png 也是逐字节相同，见 docs/visual-targets.md §7）"
    );
}

// ---------------------------------------------------------------- 断言 8：尺寸不变性

#[test]
fn number_ink_centre_is_size_invariant() {
    let art = artifacts();
    skip_without_gauge_mono!();
    let mut centres = Vec::with_capacity(art.sizes.len());
    for (logical, frame) in &art.sizes {
        let (cx, cy) = ink_centroid_uu(frame, NUMBER_BOX);
        centres.push((*logical, cx, cy));
    }
    let (base_logical, bx, by) = centres[0];
    for (logical, cx, cy) in centres.iter().skip(1) {
        assert!(
            (cx - bx).abs() <= TOL_CENTRE_UU,
            "数字 ink 中心 x 归一化后：{logical} 档 {cx:.3} uu vs {base_logical} 档 {bx:.3} uu，\
             差 {:.3} uu > ±{TOL_CENTRE_UU} uu",
            (cx - bx).abs()
        );
        assert!(
            (cy - by).abs() <= TOL_CENTRE_UU,
            "数字 ink 中心 y 归一化后：{logical} 档 {cy:.3} uu vs {base_logical} 档 {by:.3} uu，\
             差 {:.3} uu > ±{TOL_CENTRE_UU} uu",
            (cy - by).abs()
        );
    }
}
