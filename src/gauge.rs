//! `GaugeHandle`：把数值层（[`GaugeState`] + [`NeedleMotion`]）与 `.slint` 视图连起来。
//!
//! 设计要点：
//! - 一个 `SpeedGauge` 实例一个 `GaugeHandle`，内部是 `Rc<RefCell<Inner>>` + `Weak<SpeedGauge>`，
//!   所以**同一个窗口里可以放多个互不影响的仪表**。
//! - `slint::Timer` 只在"确实有动画要跑"时启动；每帧推进后一旦
//!   `NeedleMotion::is_settled()` 成立且没有 pending 目标，就**立刻 `stop()`**，
//!   收敛后不再空转刷新。
//! - 同一个 `display_angle` 同时驱动指针几何、彩弧边界段与刻度点亮（port-spec §4 规则 6）。
//! - 每帧只写"真的变了"的属性；只有边界段的 `commands` 与指针的 `commands`/`fill`
//!   是每帧重写（都是小字符串 / 一次 brush 构造）。

use std::cell::RefCell;
use std::rc::{Rc, Weak as RcWeak};
use std::time::{Duration, Instant};

use slint::private_unstable_api::re_exports::{GradientStop, LinearGradientBrush};
use slint::{Brush, Color, ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
#[cfg(feature = "needle-raster")]
use slint::{Image, SharedPixelBuffer};

use crate::animation::{NeedleConfig, NeedleMotion, SetTargetOpts};
use crate::model::{self, GaugeState, ScaleKind, Theme};
#[cfg(feature = "needle-raster")]
use crate::needle_raster::NeedleRaster;
use crate::renderer;
use crate::{
    ConnectionMode, GaugeStage, GaugeTheme, LabelSpec, MotionMode, NeedleSlice, RingSegment,
    ScaleMode, SpeedGauge,
};

/// 动画帧间隔：60 Hz。
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
/// 单帧 `dt` 上限（`animator.ts:203` 的 `Math.min(0.05, …)`）。
const DT_MAX: f64 = 0.05;
/// 连续多少帧"已收敛"才真正停表（吃掉浮点抖动与最后一帧的 ease 尾部）。
const SETTLE_FRAMES: u8 = 3;
/// `display-angle` 的写入阈值（度）。小于它的变化不脏化属性。
const ANGLE_EPS: f32 = 0.01;

// ---------------------------------------------------------------------------
// Slint 枚举 ↔ 数值层枚举
// ---------------------------------------------------------------------------

fn to_stage(s: GaugeStage) -> model::Stage {
    match s {
        GaugeStage::Idle => model::Stage::Idle,
        GaugeStage::Ping => model::Stage::Ping,
        GaugeStage::Download => model::Stage::Download,
        GaugeStage::Upload => model::Stage::Upload,
        GaugeStage::Done => model::Stage::Done,
    }
}

fn to_theme(t: GaugeTheme) -> Theme {
    match t {
        GaugeTheme::Dark => Theme::Dark,
        GaugeTheme::Light => Theme::Light,
    }
}

fn to_connection(c: ConnectionMode) -> model::ConnectionMode {
    match c {
        ConnectionMode::Multi => model::ConnectionMode::Multi,
        ConnectionMode::Single => model::ConnectionMode::Single,
    }
}

fn to_scale(s: ScaleMode) -> model::ScaleMode {
    match s {
        ScaleMode::Fixed => model::ScaleMode::Fixed,
        ScaleMode::Auto => model::ScaleMode::Auto,
    }
}

fn to_motion(m: MotionMode) -> model::MotionMode {
    match m {
        MotionMode::Tween => model::MotionMode::Tween,
        MotionMode::Spring => model::MotionMode::Spring,
    }
}

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgb_u8(c[0], c[1], c[2])
}

/// 指针渐变 brush：方向 = 指针轴，元素盒 = 170 见方（见 [`renderer::needle_gradient`]）。
fn needle_brush(theme: Theme, angle_deg: f64) -> Brush {
    let stops = renderer::needle_gradient(theme, angle_deg)
        .into_iter()
        .map(|s| GradientStop {
            position: s.position as f32,
            color: Color::from_argb_u8(s.alpha, s.rgb[0], s.rgb[1], s.rgb[2]),
        });
    Brush::LinearGradient(LinearGradientBrush::new(angle_deg as f32, stops))
}

// ---------------------------------------------------------------------------
// Inner
// ---------------------------------------------------------------------------

/// 控制器的全部可变状态（`GaugeHandle` 通过 `Rc<RefCell<_>>` 共享它）。
struct Inner {
    /// 纯决策层状态（`gauge.ts` 的字段镜像）。
    state: GaugeState,
    /// 指针积分器（含微颤）。
    motion: NeedleMotion,
    /// 当前**绘制角** = `motion.step()` 的返回值（含微颤，范围 `[-137, 137]`）。
    display: f64,
    /// `animation-enabled` 的镜像：关掉后 `set_speed_kbps` 退化成直接定位。
    animation_enabled: bool,

    // ---- 属性推送去抖 ----
    last_angle: Option<f32>,
    last_number: String,
    last_lit: i32,
    last_visible: i32,
    last_boundary: String,
    last_boundary_color: Option<Color>,
    pushed_variant: Option<model::GradientVariant>,
    pushed_scale: Option<ScaleKind>,
    pushed_theme: Option<Theme>,
    pushed_accent: Option<&'static str>,
    /// 指针切片模型（32 行，逐帧 `set_row_data` 原地更新，不重建）。
    needle_slices: Option<Rc<VecModel<NeedleSlice>>>,
    /// 指针整体光栅化器（feature `needle-raster`）：一张 `Pixmap` + 一块输出缓冲，
    /// 稳态零重建（见 [`NeedleRaster::resizes`]）。
    #[cfg(feature = "needle-raster")]
    needle_raster: NeedleRaster,

    // ---- 动画泵 ----
    /// 回指 `GaugeHandle` 持有的 `Timer`（用于收敛后自行停表）。
    timer: RcWeak<Timer>,
    /// 上一帧时间戳（真实时钟推进用）。
    clock: Option<Instant>,
    paused: bool,
    stopped: bool,
    settle_frames: u8,
}

impl Inner {
    fn new(theme: Theme, connection: model::ConnectionMode, scale: model::ScaleMode) -> Self {
        Self {
            state: GaugeState::new(theme, connection, scale),
            motion: NeedleMotion::new(NeedleConfig::default()),
            display: crate::geometry::GEO.start_angle,
            animation_enabled: true,
            last_angle: None,
            last_number: String::new(),
            last_lit: -1,
            last_visible: -1,
            last_boundary: String::new(),
            last_boundary_color: None,
            pushed_variant: None,
            pushed_scale: None,
            pushed_theme: None,
            pushed_accent: None,
            needle_slices: None,
            #[cfg(feature = "needle-raster")]
            needle_raster: NeedleRaster::new(),
            timer: RcWeak::new(),
            clock: None,
            paused: false,
            stopped: false,
            settle_frames: 0,
        }
    }

    /// 是否还有动画要跑（收敛后为 `false`）。
    fn busy(&self) -> bool {
        !(self.motion.is_settled() && !self.motion.is_resetting())
    }

    /// 停表（幂等）。
    fn stop_timer(&mut self) {
        self.clock = None;
        self.settle_frames = 0;
        if let Some(t) = self.timer.upgrade() {
            if t.running() {
                t.stop();
            }
        }
    }

    /// 一帧之后判断是否可以停表。
    fn settle_after_frame(&mut self) {
        if self.busy() {
            self.settle_frames = SETTLE_FRAMES;
            return;
        }
        self.settle_frames = self.settle_frames.saturating_sub(1);
        if self.settle_frames == 0 {
            self.stop_timer();
        }
    }

    /// 把"当前状态"整体推给视图。只写真的变了的属性。
    fn paint(&mut self, ui: &SpeedGauge) {
        let angle = self.display;
        let arc = angle.clamp(
            crate::geometry::GEO.start_angle,
            crate::geometry::GEO.end_angle,
        );

        // 圆环段表只在变体变化时重建（240 条字符串，不在每帧做）。
        if self.pushed_variant != Some(self.state.variant) {
            let table = renderer::table(self.state.variant);
            let segs: Vec<RingSegment> = table
                .commands
                .iter()
                .zip(table.colors.iter())
                .map(|(d, c)| RingSegment {
                    commands: d.as_str().into(),
                    color: rgb(*c),
                })
                .collect();
            ui.set_ring_segments(ModelRc::new(VecModel::from(segs)));
            self.pushed_variant = Some(self.state.variant);
        }

        // 刻度表只在档位变化时重建。
        if self.pushed_scale != Some(self.state.scale) {
            let specs: Vec<LabelSpec> = renderer::labels_geo(self.state.labels())
                .into_iter()
                .map(|l| LabelSpec {
                    text: l.text.as_str().into(),
                    x: l.x as f32,
                    y: l.y as f32,
                })
                .collect();
            ui.set_labels(ModelRc::new(VecModel::from(specs)));
            self.pushed_scale = Some(self.state.scale);
        }

        // 主题 token 与强调色。
        if self.pushed_theme != Some(self.state.theme) {
            let t = self.state.tokens();
            ui.set_track_color(rgb(t.track));
            ui.set_label_on_color(rgb(t.label_on));
            ui.set_label_off_color(rgb(t.label_off));
            ui.set_number_color(rgb(t.number));
            ui.set_unit_color(rgb(t.unit));
            self.pushed_theme = Some(self.state.theme);
        }
        if self.pushed_accent != Some(self.state.accent.name) {
            ui.set_accent_color(rgb(self.state.accent.srgb));
            self.pushed_accent = Some(self.state.accent.name);
        }

        // 彩弧：前缀段数 + 边界段（同一个 arc 驱动）。
        let view = renderer::ring_view(self.state.variant, arc);
        let visible = view.visible as i32;
        if self.last_visible != visible {
            ui.set_ring_visible_count(visible);
            self.last_visible = visible;
        }
        if self.last_boundary != view.boundary {
            ui.set_ring_boundary_commands(view.boundary.as_str().into());
            self.last_boundary = view.boundary.clone();
        }
        let boundary_color = rgb(view.boundary_color);
        if self.last_boundary_color != Some(boundary_color) {
            ui.set_ring_boundary_color(boundary_color);
            self.last_boundary_color = Some(boundary_color);
        }

        // 刻度点亮：前缀计数。
        let lit = renderer::lit_count(self.state.labels(), arc) as i32;
        if self.last_lit != lit {
            ui.set_lit_count(lit);
            self.last_lit = lit;
        }

        // 数字读数。
        if self.last_number != self.state.number_text {
            ui.set_number_text(self.state.number_text.as_str().into());
            self.last_number = self.state.number_text.clone();
        }

        // 指针：几何每帧烘焙（软件渲染器不支持 transform-rotation，也不支持渐变填充
        // 路径）。`display-angle` / `needle-commands` / `needle-fill` 三条属性始终推，
        // 保证"单 Path 回退"这条路对宿主始终可用。
        let f = angle as f32;
        if self.last_angle.is_none_or(|p| (p - f).abs() > ANGLE_EPS) {
            ui.set_display_angle(f);
            self.last_angle = Some(f);
        }
        ui.set_needle_commands(renderer::needle_commands(angle).as_str().into());
        ui.set_needle_fill(needle_brush(self.state.theme, angle));

        // 真正的指针实现，两条互斥路径，`#[cfg]` 在 impl 块层面分开：
        // - feature `needle-raster`：整条指针光栅化成一张 RGBA，`Image` 一次合成；
        // - 关掉它：32 片梯形平色（历史路径）。
        self.paint_needle(ui, angle);
    }

    /// 切片路径：32 片梯形平色（`needle-slices` 模型逐行原地更新）。
    ///
    /// - 关掉 `needle-raster` 时这是**唯一**路径；
    /// - 开着时它是"窗口尺寸还未知"的回退（见 `push_needle_image`）。
    fn paint_needle_slices(&mut self, ui: &SpeedGauge, angle: f64) {
        // 光栅图已接管时直接返回：三条路互斥由 `needle-image.width == 0` 保证，这里提前
        // 退出只是省掉每帧 32 条 path 字符串的格式化开销。
        if ui.get_needle_image().size().width > 0 {
            return;
        }
        // 底色显式传下去：不透明预合成要把 `page` 混进片色（指针只落在 page 上，
        // 环形轨道内半径 118.5 > 指针长 85，刻度/读数不在扫掠范围内 ⇒ 逐像素等价）。
        let slices = renderer::needle_slices_on(self.state.theme, angle, self.state.tokens().page);
        let rows: Vec<NeedleSlice> = slices
            .iter()
            .map(|slice| NeedleSlice {
                commands: slice.commands.as_str().into(),
                color: Color::from_argb_u8(slice.alpha, slice.rgb[0], slice.rgb[1], slice.rgb[2]),
            })
            .collect();
        match &self.needle_slices {
            Some(model) => {
                // 行已存在：逐片原地更新，不重建模型、不动 repeater 结构。
                for (i, row) in rows.into_iter().enumerate() {
                    model.set_row_data(i, row);
                }
            }
            None => {
                // 首次：一次性建满 32 行（`set_row_data` 不会自动扩行）。
                let model = Rc::new(VecModel::from(rows));
                ui.set_needle_slices(ModelRc::from(model.clone()));
                self.needle_slices = Some(model);
            }
        }
    }

    /// 手动 / 定时推进一帧。
    fn step(&mut self, ui: &SpeedGauge, dt: f64) {
        self.display = self.motion.step(dt.clamp(0.0, DT_MAX));
        self.paint(ui);
        self.settle_after_frame();
    }

    /// 用真实时钟推进一帧（Timer 回调走这条）。
    fn tick_now(&mut self, ui: &SpeedGauge) {
        let now = Instant::now();
        let dt = match self.clock {
            Some(prev) => now.duration_since(prev).as_secs_f64(),
            None => 0.0,
        };
        self.clock = Some(now);
        self.step(ui, dt);
    }

    /// 应用运动参数（模式 + 微颤开关）。
    fn apply_config(&mut self, motion: model::MotionMode, wobble: bool) {
        self.motion.config.mode = motion;
        self.motion.config.wobble_max = if wobble { 1.1 } else { 0.0 };
    }
}

// ---------------------------------------------------------------------------
// 指针实现之一：整体光栅化（feature `needle-raster`）
// ---------------------------------------------------------------------------

#[cfg(feature = "needle-raster")]
impl Inner {
    /// 光栅路径；拿不到窗口尺寸时退回切片（`#[cfg]` 只在本 impl 块上，不混进 `paint`）。
    fn paint_needle(&mut self, ui: &SpeedGauge, angle: f64) {
        if !self.push_needle_image(ui, angle) {
            self.paint_needle_slices(ui, angle);
        }
    }

    /// 把整条指针光栅化成一张 RGBA，推到 `needle-image`。
    ///
    /// 像素边长 = `170 * k * dpr`，其中 `k = 组件逻辑宽 / 286`、`dpr = window().scale_factor()`。
    /// 组件通常就是窗口根元素（`shots.rs` / `minimal.rs` / `ComponentFactory` 建的仪表都是），
    /// 故 `k` 由窗口逻辑宽反推；窗口还没定尺寸（`< 2px`）时返回 `false` 让调用方回退。
    ///
    /// **注意时序**：`needle-image` 是命令式属性，只有真被 `paint` 过才会更新。宿主若在
    /// 建组件之后才改 `scale_factor` / 窗口尺寸，需要再触发一次 `paint`（`examples/shots.rs`
    /// 的 `warm_up_raster` 就是干这个的）。
    fn push_needle_image(&mut self, ui: &SpeedGauge, angle: f64) -> bool {
        let window = ui.window();
        let dpr = window.scale_factor();
        if !dpr.is_finite() || dpr <= 0.0 {
            return false;
        }
        let logical_w = window.size().width as f32 / dpr;
        if !logical_w.is_finite() || logical_w < 1.0 {
            return false;
        }
        let px = (170.0 * (logical_w / crate::GAUGE_UU) * dpr).round();
        if !px.is_finite() || px < 2.0 {
            return false;
        }
        let theme = ui.get_theme();
        let image = {
            let (bytes, w, h) = self.needle_raster.update(theme, angle, px as u32);
            Image::from_rgba8(SharedPixelBuffer::clone_from_slice(bytes, w, h))
        };
        ui.set_needle_image(image);
        self.drop_needle_slices(ui);
        true
    }

    /// 光栅图接管后清空切片模型 —— `needle-slices.length == 0` + `needle-image.width > 0`
    /// 一起保证"raster / slices / 单 Path"三条路互斥。
    fn drop_needle_slices(&mut self, ui: &SpeedGauge) {
        if self.needle_slices.is_some() {
            let empty: Vec<NeedleSlice> = Vec::new();
            ui.set_needle_slices(ModelRc::new(VecModel::from(empty)));
            self.needle_slices = None;
        }
    }
}

// ---------------------------------------------------------------------------
// 指针实现之二：切片（关掉 `needle-raster` 时的唯一路径）
// ---------------------------------------------------------------------------

#[cfg(not(feature = "needle-raster"))]
impl Inner {
    /// 切片路径（没有光栅化器可用，直接走 32 片梯形）。
    fn paint_needle(&mut self, ui: &SpeedGauge, angle: f64) {
        self.paint_needle_slices(ui, angle);
    }
}

/// 装配定时器回调：每 16 ms 推进一帧，收敛时由 [`Inner::stop_timer`] 自行停表。
fn arm(timer: &Rc<Timer>, inner: &Rc<RefCell<Inner>>, weak: &slint::Weak<SpeedGauge>) {
    let inner_cb = inner.clone();
    let weak_cb = weak.clone();
    timer.start(TimerMode::Repeated, FRAME_INTERVAL, move || {
        let Some(ui) = weak_cb.upgrade() else {
            if let Ok(mut i) = inner_cb.try_borrow_mut() {
                i.stop_timer();
            }
            return;
        };
        if let Ok(mut i) = inner_cb.try_borrow_mut() {
            i.tick_now(&ui);
        }
    });
}

// ---------------------------------------------------------------------------
// GaugeHandle
// ---------------------------------------------------------------------------

/// 一个 `SpeedGauge` 实例的控制器。
#[derive(Clone)]
pub struct GaugeHandle {
    weak: slint::Weak<SpeedGauge>,
    inner: Rc<RefCell<Inner>>,
    timer: Rc<Timer>,
}

impl GaugeHandle {
    /// 绑定到一个已经创建好的 `SpeedGauge`，返回控制器。
    ///
    /// 会立刻把初始状态（刻度表、彩弧、颜色、指针）推给视图一次。
    /// 宿主在 attach 之前写好的 `stage` / `speed-kbps` / 主题等意图会被读进来。
    pub fn attach(ui: &SpeedGauge) -> Self {
        // 字体必须先就位，否则 `Gauge Mono` / `Montserrat` 会退回系统默认字体，
        // 数字与刻度墨迹宽度会整体走样（PROBE E1）。
        let _ = crate::fonts::register_bundled_fonts();

        let mut inner = Inner::new(
            to_theme(ui.get_theme()),
            to_connection(ui.get_connection_mode()),
            to_scale(ui.get_scale_mode()),
        );
        let motion = to_motion(ui.get_motion_mode());
        let wobble = ui.get_wobble_enabled();
        inner.apply_config(motion, wobble);
        inner.animation_enabled = ui.get_animation_enabled();

        // 宿主可能在 attach 之前就写好了 stage：按"无副作用"的方式接过来（不触发归零）。
        let stage = to_stage(ui.get_stage());
        inner.state.stage = stage;
        inner.state.variant = model::variant_for(stage, inner.state.connection_mode);
        inner.state.last_variant = Some(inner.state.variant);
        inner.state.accent = model::accent_for(stage, inner.state.connection_mode);

        let timer = Rc::new(Timer::default());
        inner.timer = Rc::downgrade(&timer);

        let seed_speed = ui.get_speed_kbps() as f64;
        if seed_speed > 0.0 {
            let angle = inner.state.snap_to(seed_speed);
            inner.motion.set_target(angle, SetTargetOpts::snap());
            inner.display = angle;
        } else {
            inner.state.update_number(0.0);
        }

        let handle = Self {
            weak: ui.as_weak(),
            inner: Rc::new(RefCell::new(inner)),
            timer,
        };
        {
            let mut i = handle.inner.borrow_mut();
            i.paint(ui);
        }
        handle
    }

    // -- 内部工具 ----------------------------------------------------------

    /// 需要时启动定时器（暂停 / 停止 / 已收敛时不启）。
    fn arm_if_needed(&self) {
        {
            let Ok(inner) = self.inner.try_borrow() else {
                return;
            };
            if inner.paused || inner.stopped || !inner.busy() {
                return;
            }
        }
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.settle_frames = SETTLE_FRAMES;
            inner.clock = Some(Instant::now());
        }
        if !self.timer.running() {
            arm(&self.timer, &self.inner, &self.weak);
        }
    }

    /// 推进一帧并把结果推给视图（内部统一入口）。
    fn advance(&self, dt: f64) {
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.step(&ui, dt);
        }
    }

    // -- 公共 API ----------------------------------------------------------

    /// 推一个采样值（kbps）。指针按 `motion-mode` 追踪，数字立刻更新。
    pub fn set_speed_kbps(&self, kbps: f64) {
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        {
            let mut inner = self.inner.borrow_mut();
            let angle = inner.state.set_speed(kbps);
            ui.set_speed_kbps(kbps as f32);
            if inner.animation_enabled {
                inner.motion.set_target(angle, SetTargetOpts::default());
            } else {
                inner.motion.set_target(angle, SetTargetOpts::snap());
                inner.display = angle;
            }
            inner.paint(&ui);
        }
        self.arm_if_needed();
    }

    /// 切换阶段（`gauge.ts:283-302` 的完整副作用，含 upload/idle 归零）。
    pub fn set_stage(&self, stage: GaugeStage) {
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        {
            let mut inner = self.inner.borrow_mut();
            let effects = inner.state.set_stage(to_stage(stage));
            if effects.changed {
                ui.set_stage(stage);
                if effects.reset_needle {
                    inner.motion.reset();
                }
                inner.paint(&ui);
            }
        }
        self.arm_if_needed();
    }

    /// 切换主题（颜色 + 指针渐变；不会重置指针）。
    pub fn set_theme(&self, theme: GaugeTheme) {
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        let mut inner = self.inner.borrow_mut();
        inner.state.theme = to_theme(theme);
        ui.set_theme(theme);
        inner.paint(&ui);
    }

    /// 切换连接模式（重算变体与强调色，不动指针/数字）。
    pub fn set_connection_mode(&self, m: ConnectionMode) {
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        let mut inner = self.inner.borrow_mut();
        let effects = inner.state.set_connection_mode(to_connection(m));
        if effects.changed {
            ui.set_connection_mode(m);
            inner.paint(&ui);
        }
    }

    /// 切换刻度模式（`auto` + `>1000 Mbps` 时切 10 档）。
    pub fn set_scale_mode(&self, m: ScaleMode) {
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        let mut inner = self.inner.borrow_mut();
        let rebuilt = inner.state.set_scale_mode(to_scale(m));
        ui.set_scale_mode(m);
        if rebuilt {
            inner.paint(&ui);
        }
    }

    /// 切换指针跟踪模型（tween / spring）。
    pub fn set_motion_mode(&self, m: MotionMode) {
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        {
            let mut inner = self.inner.borrow_mut();
            inner.motion.config.mode = to_motion(m);
            ui.set_motion_mode(m);
        }
        self.arm_if_needed();
    }

    /// 关掉动画后所有 `set_speed_kbps` 都退化成直接定位。
    pub fn set_animation_enabled(&self, on: bool) {
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        {
            let mut inner = self.inner.borrow_mut();
            inner.animation_enabled = on;
            ui.set_animation_enabled(on);
            if !on {
                let target = inner.motion.target;
                inner.motion.set_target(target, SetTargetOpts::snap());
                inner.display = target;
                inner.paint(&ui);
                inner.stop_timer();
            }
        }
        self.arm_if_needed();
    }

    /// 开关微颤（等价于把 `wobbleMax` 设成 0）。
    pub fn set_wobble_enabled(&self, on: bool) {
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        let mut inner = self.inner.borrow_mut();
        let mode = inner.motion.config.mode;
        inner.apply_config(mode, on);
        ui.set_wobble_enabled(on);
    }

    /// 无动画精确定位（截图 / 冻结用）。
    ///
    /// **有意偏离原实现的一处**：参考实现的 `snapTo` 只写 `angle` / `target`，**不清**
    /// `springResetTween`（`animator.ts:117-135` vs `138-145`，port-spec 附录 B12）。于是
    /// `reset()` / `set_stage(Upload|Idle)` 之后紧接着调 `snap_to()`，接下来的帧会被那条仍在
    /// 进行的 500 ms 归位 tween 拽回 −135° —— 定位结果取决于调用时机，而 `snap_to` 是"无动画
    /// 精确校准"的公共 API，其语义必须是**最终态**。因此这里先
    /// [`NeedleMotion::cancel_pending_return`] 再 `set_target(.., snap)`。
    ///
    /// 只影响 `snap_to` 自己：`set_target` / `reset` / `step` 的既有行为（含 B12 那条"归位期间
    /// `set_target` 不打断"）保持原样。
    pub fn snap_to(&self, kbps: f64) {
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        let mut inner = self.inner.borrow_mut();
        let angle = inner.state.snap_to(kbps);
        inner.motion.cancel_pending_return();
        inner.motion.set_target(angle, SetTargetOpts::snap());
        inner.display = angle;
        ui.set_speed_kbps(kbps as f32);
        inner.paint(&ui);
        inner.stop_timer();
    }

    /// 500 ms `easeInOut` 归零（不改 stage / labels）。
    pub fn reset(&self) {
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        {
            let mut inner = self.inner.borrow_mut();
            inner.state.reset();
            inner.motion.reset();
            inner.paint(&ui);
        }
        self.arm_if_needed();
    }

    /// 暂停动画泵（保留当前视图）。
    pub fn pause(&self) {
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.paused = true;
            inner.stop_timer();
        }
    }

    /// 恢复动画泵（仅在还有动画要跑时真正启动）。
    pub fn resume(&self) {
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.paused = false;
        }
        self.arm_if_needed();
    }

    /// 彻底停掉动画泵；后续 `set_*` 不会自动重启，直到 [`Self::resume`]。
    pub fn stop(&self) {
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.stopped = true;
            inner.stop_timer();
        }
    }

    /// 当前绘制角（含微颤，度）。
    pub fn angle(&self) -> f64 {
        self.inner.borrow().display
    }

    /// 指针光栅化器**重建画布**的累计次数（feature `needle-raster`）。
    ///
    /// 稳态下这个数不该再涨：画布（`Pixmap` + 输出 `Vec<u8>`）只在像素边长变化时重建，
    /// 每帧只 `clear` + 重绘。性能 / 零稳态分配的验收就靠它（见 `examples/shots.rs`）。
    #[cfg(feature = "needle-raster")]
    pub fn needle_raster_resizes(&self) -> usize {
        self.inner.borrow().needle_raster.resizes()
    }

    /// 手动推进一帧（确定性测试 / 截图用）。
    pub fn step(&self, dt_seconds: f64) {
        self.advance(dt_seconds);
    }

    /// 用真实时钟推进一帧（`Timer` 内部走的就是这条）。
    pub fn tick_now(&self) {
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.tick_now(&ui);
        }
    }

    /// 指针是否已经收敛（`isSettled()` 且没有归位 tween）。
    pub fn is_settled(&self) -> bool {
        !self.inner.borrow().busy()
    }

    /// 定时器当前是否在跑（验收用：收敛后必须是 `false`）。
    pub fn timer_running(&self) -> bool {
        self.timer.running()
    }

    /// 当前读数文本（`formatSpeed` 的结果）。
    pub fn number_text(&self) -> String {
        self.inner.borrow().state.number_text.clone()
    }

    /// 当前被点亮的刻度数。
    pub fn lit_count(&self) -> usize {
        let inner = self.inner.borrow();
        let arc = inner.display.clamp(-135.0, 135.0);
        renderer::lit_count(inner.state.labels(), arc)
    }

    /// 当前完整画出的彩弧段数。
    pub fn ring_visible_count(&self) -> usize {
        let inner = self.inner.borrow();
        let arc = inner.display.clamp(-135.0, 135.0);
        renderer::ring_view(inner.state.variant, arc).visible
    }
}
