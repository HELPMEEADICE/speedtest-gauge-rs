//! 运动引擎：cubic-bezier 缓动、阻尼弹簧、微颤与帧时钟。
//!
//! 逐行等价移植 `src/gauge/animator.ts`（全文 231 行）。
//!
//! 两个跟踪模型：
//! - [`MotionMode::Tween`] —— 生产环境仪表的行为：每个新样本从**当前角**重启一条 200 ms
//!   `easeOut` tween，相位切换跑 500 ms `easeInOut` 归位；
//! - [`MotionMode::Spring`]（默认）—— 阻尼弹簧，固有频率按"阶跃响应 ≈ 200 ms 收敛"标定，
//!   阻尼比略小于 1 以保留真实仪表的微小过冲。
//!
//! 两种模型之上都叠加一层由**速度**驱动的微颤（wobble）。

use std::sync::{Arc, LazyLock};

use crate::geometry::{clamp, max_nan, min_nan, GEO};
use crate::model::MotionMode;

pub use crate::model::{LABEL_FADE_IN, LABEL_STAGGER, NEEDLE_RETURN, NEEDLE_TRACKING};

/// 缓动函数的可共享句柄（`animator.ts:23` 的 `Easing = (t: number) => number`）。
pub type Easing = Arc<dyn Fn(f64) -> f64 + Send + Sync>;

/// 具名缓动（`animator.ts:68-69` 的两条常量 + 自定义）。
#[derive(Clone)]
pub enum Ease {
    /// `cubicBezier(0, 0, 0.58, 1)`（framer-motion 的 `easeOut`）。
    Out,
    /// `cubicBezier(0.42, 0, 0.58, 1)`（framer-motion 的 `easeInOut`）。
    InOut,
    /// 调用方自带的缓动。
    Custom(Easing),
}

impl Ease {
    /// 求值。
    pub fn eval(&self, t: f64) -> f64 {
        match self {
            Ease::Out => ease_out(t),
            Ease::InOut => ease_in_out(t),
            Ease::Custom(f) => f(t),
        }
    }
}

impl std::fmt::Debug for Ease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ease::Out => f.write_str("Ease::Out"),
            Ease::InOut => f.write_str("Ease::InOut"),
            Ease::Custom(_) => f.write_str("Ease::Custom(..)"),
        }
    }
}

/// 标准 CSS / framer 的 cubic-bezier 求值器（`animator.ts:26-65`）。
///
/// 控制点 `(x1, y1)`、`(x2, y2)`，端点固定在 `(0,0)` 与 `(1,1)`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    cx: f64,
    bx: f64,
    ax: f64,
    cy: f64,
    by: f64,
    ay: f64,
}

impl CubicBezier {
    /// 由两个控制点构造。
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        let cx = 3.0 * x1;
        let bx = 3.0 * (x2 - x1) - cx;
        let ax = 1.0 - cx - bx;
        let cy = 3.0 * y1;
        let by = 3.0 * (y2 - y1) - cy;
        let ay = 1.0 - cy - by;
        Self {
            cx,
            bx,
            ax,
            cy,
            by,
            ay,
        }
    }

    fn sample_x(&self, t: f64) -> f64 {
        ((self.ax * t + self.bx) * t + self.cx) * t
    }

    fn sample_y(&self, t: f64) -> f64 {
        ((self.ay * t + self.by) * t + self.cy) * t
    }

    fn sample_dx(&self, t: f64) -> f64 {
        (3.0 * self.ax * t + 2.0 * self.bx) * t + self.cx
    }

    /// 由 `x` 反解参数 `t`：先牛顿迭代最多 8 次，再二分兜底最多 24 次。
    ///
    /// 注意：牛顿循环结束后**不**沿用其结果，而是重置 `lo = 0, hi = 1, t = x` 再二分
    /// （源码如此，逐字复刻）。
    fn solve_x(&self, x: f64) -> f64 {
        let mut t = x;
        for _ in 0..8 {
            let err = self.sample_x(t) - x;
            if err.abs() < 1e-6 {
                return t;
            }
            let d = self.sample_dx(t);
            if d.abs() < 1e-6 {
                break;
            }
            t -= err / d;
        }
        let mut lo = 0.0;
        let mut hi = 1.0;
        t = x;
        for _ in 0..24 {
            let v = self.sample_x(t);
            if (v - x).abs() < 1e-6 {
                break;
            }
            if v < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.0;
        }
        t
    }

    /// 求值：`x <= 0 → 0`，`x >= 1 → 1`，否则 `sampleY(solveX(x))`。
    pub fn eval(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        self.sample_y(self.solve_x(x))
    }
}

/// 构造一个 cubic-bezier 缓动闭包（`animator.ts:26` 的返回类型）。
pub fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64) -> impl Fn(f64) -> f64 {
    let curve = CubicBezier::new(x1, y1, x2, y2);
    move |x: f64| curve.eval(x)
}

static EASE_OUT_CURVE: LazyLock<CubicBezier> =
    LazyLock::new(|| CubicBezier::new(0.0, 0.0, 0.58, 1.0));
static EASE_IN_OUT_CURVE: LazyLock<CubicBezier> =
    LazyLock::new(|| CubicBezier::new(0.42, 0.0, 0.58, 1.0));

/// `EASE_OUT(0, 0, 0.58, 1)`（`animator.ts:68`）。
pub fn ease_out(t: f64) -> f64 {
    EASE_OUT_CURVE.eval(t)
}

/// `EASE_IN_OUT(0.42, 0, 0.58, 1)`（`animator.ts:69`）。
pub fn ease_in_out(t: f64) -> f64 {
    EASE_IN_OUT_CURVE.eval(t)
}

/// `EASE_OUT` 曲线本体（需要反复求值时可避免重复构造）。
pub fn ease_out_curve() -> &'static CubicBezier {
    &EASE_OUT_CURVE
}

/// `EASE_IN_OUT` 曲线本体。
pub fn ease_in_out_curve() -> &'static CubicBezier {
    &EASE_IN_OUT_CURVE
}

/// 指针运动参数（`animator.ts:71-83`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeedleConfig {
    /// 跟踪模型。
    pub mode: MotionMode,
    /// 弹簧固有频率（rad/s）；21.4 对应 ≈0.2 s 收敛。
    ///
    /// 注：源码注释的 "~0.2 s" 是**1% 收敛**口径（实测 0.267 s）；
    /// 若用 [`NeedleMotion::is_settled`] 的 0.02° 容差衡量，实测要到 0.367 s。
    pub omega: f64,
    /// 阻尼比；1 = 临界阻尼，< 1 略有过冲。
    pub zeta: f64,
    /// 微颤频率（Hz）。
    pub wobble_hz: f64,
    /// 微颤振幅上限（度）。
    pub wobble_max: f64,
    /// 每 1 度/秒的指针速度对应的微颤幅度（度）。
    pub wobble_gain: f64,
}

impl Default for NeedleConfig {
    /// `DEFAULT_NEEDLE_CONFIG`（`animator.ts:85-92`）。
    fn default() -> Self {
        Self {
            mode: MotionMode::Spring,
            omega: 21.4,
            zeta: 0.92,
            wobble_hz: 6.5,
            wobble_max: 1.1,
            wobble_gain: 0.0032,
        }
    }
}

/// `setTarget` 的可选项（`animator.ts:117` 的 `opts`）。
#[derive(Debug, Clone, Default)]
pub struct SetTargetOpts {
    /// 立即摆到目标角（清空 tween、速度与微颤振幅）。
    pub snap: bool,
    /// tween 时长（秒），默认 [`NEEDLE_TRACKING`]。
    pub duration: Option<f64>,
    /// 缓动，默认 [`Ease::Out`]。
    pub ease: Option<Ease>,
}

impl SetTargetOpts {
    /// `{ snap: true }`。
    pub fn snap() -> Self {
        Self {
            snap: true,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone)]
struct Tween {
    from: f64,
    to: f64,
    elapsed: f64,
    duration: f64,
    ease: Ease,
}

/// 指针积分器（`animator.ts:102-195` 的 `NeedleMotion`）。
#[derive(Debug, Clone)]
pub struct NeedleMotion {
    /// 当前角（度）。
    pub angle: f64,
    /// 当前角速度（度/秒）。
    pub velocity: f64,
    /// 目标角（度，已 clamp 到 `[-135, 135]`）。
    pub target: f64,
    /// 运动参数。
    pub config: NeedleConfig,
    tween: Option<Tween>,
    spring_reset_tween: Option<Tween>,
    wobble_amp: f64,
    wobble_phase: f64,
}

impl Default for NeedleMotion {
    fn default() -> Self {
        Self::new(NeedleConfig::default())
    }
}

impl NeedleMotion {
    /// 以给定参数构造，初始静止在起始角 `-135°`。
    pub fn new(config: NeedleConfig) -> Self {
        Self {
            angle: GEO.start_angle,
            velocity: 0.0,
            target: GEO.start_angle,
            config,
            tween: None,
            spring_reset_tween: None,
            wobble_amp: 0.0,
            wobble_phase: 0.0,
        }
    }

    /// 指向 `angle`（`animator.ts:117-135`）。
    ///
    /// tween 模式下**每次**调用都把 `elapsed` 归零、`from` 取**当前**角、`duration`
    /// 重置为完整时长 —— 连续高频样本因此会产生"永远追不上"的追赶曲线（原站真实行为）。
    ///
    /// 注意 `spring_reset_tween` **不**在这里被清除（`animator.ts:141-144` vs `117-135`，
    /// port-spec 附录 B12）：spring 模式下"归位途中收到新样本"的表现是先把归位跑完。
    pub fn set_target(&mut self, angle: f64, opts: SetTargetOpts) {
        self.target = clamp(angle, GEO.start_angle, GEO.end_angle);
        if opts.snap {
            self.angle = self.target;
            self.velocity = 0.0;
            self.tween = None;
            self.wobble_amp = 0.0;
            return;
        }
        if self.config.mode == MotionMode::Tween {
            self.tween = Some(Tween {
                from: self.angle,
                to: self.target,
                elapsed: 0.0,
                duration: opts.duration.unwrap_or(NEEDLE_TRACKING),
                ease: opts.ease.unwrap_or(Ease::Out),
            });
        }
    }

    /// 平滑归位到起始角，默认时长 [`NEEDLE_RETURN`]（`animator.ts:138-145`）。
    pub fn reset(&mut self) {
        self.reset_with(NEEDLE_RETURN);
    }

    /// 同 [`Self::reset`]，但显式指定时长。
    ///
    /// spring 模式下源码会把这条 tween **额外**挂到 `springResetTween` 槽上，使
    /// `step()` 走 tween 分支（即"弹簧模式也要遵守显式时长"）。
    pub fn reset_with(&mut self, duration: f64) {
        self.target = GEO.start_angle;
        let tween = Tween {
            from: self.angle,
            to: GEO.start_angle,
            elapsed: 0.0,
            duration,
            ease: Ease::InOut,
        };
        if self.config.mode == MotionMode::Spring {
            self.spring_reset_tween = Some(tween.clone());
        }
        self.tween = Some(tween);
    }

    /// 取消一切待执行的动画，把指针**冻结在当前角**：清 `spring_reset_tween` 与 `tween`，
    /// `target = angle`，`velocity = 0`（微颤状态不动）。
    ///
    /// 语义与 `set_target(.., SetTargetOpts::snap())` 的区别：`snap` 跳到**新目标**，本方法
    /// 只"停住"，不改变朝向 —— 它是给 [`Self::set_target`] 之前用的"清场"动作。
    ///
    /// 存在的理由（源码 B12）：`set_target` 不清 `spring_reset_tween`，`step()` 又优先走归位
    /// tween，所以"`reset()` / `reset_with()` 之后立刻改变目标"会被仍在跑的 500 ms 归位过程
    /// 覆盖回 −135°。`set_target` / `reset` / `reset_with` / `step` 的既有行为**完全不变**，
    /// 需要"立刻接管指针"的调用方显式调用本方法（[`crate::gauge::GaugeHandle::snap_to`] 就是）。
    pub fn cancel_pending_return(&mut self) {
        self.spring_reset_tween = None;
        self.tween = None;
        self.target = self.angle;
        self.velocity = 0.0;
    }

    /// 是否已稳定（`animator.ts:149-151`）：`|angle - target| < 0.02 && |velocity| < 0.5`。
    pub fn is_settled(&self) -> bool {
        (self.angle - self.target).abs() < 0.02 && self.velocity.abs() < 0.5
    }

    /// 归位 tween 是否仍在进行（`springResetTween` 非空）。
    pub fn is_resetting(&self) -> bool {
        self.spring_reset_tween.is_some()
    }

    /// 当前微颤振幅（度），调试 / 测试用。
    pub fn wobble_amp(&self) -> f64 {
        self.wobble_amp
    }

    /// 当前微颤相位（弧度），调试 / 测试用。
    pub fn wobble_phase(&self) -> f64 {
        self.wobble_phase
    }

    /// 推进 `dt` 秒，返回**实际绘制角**（含微颤，已 clamp 到 `[-137, 137]`）。
    ///
    /// `animator.ts:154-194` 逐行等价：
    /// 1. `mode == Tween || springResetTween.is_some()` 走 tween 分支（后者优先）；
    ///    tween 为空时**什么也不做**，`velocity` 保留旧值。
    /// 2. 否则走弹簧：半隐式欧拉，子步长上限 `1/480 s`，`steps = max(1, ceil(dt / (1/480)))`。
    /// 3. 两条分支之后都叠加微颤并 clamp。
    pub fn step(&mut self, dt: f64) -> f64 {
        if self.config.mode == MotionMode::Tween || self.spring_reset_tween.is_some() {
            let using_reset = self.spring_reset_tween.is_some();
            let active = if using_reset {
                self.spring_reset_tween.as_mut()
            } else {
                self.tween.as_mut()
            };
            let advanced = active.map(|tw| {
                tw.elapsed += dt;
                let ratio = tw.elapsed / tw.duration;
                let raw = if tw.duration <= 0.0 {
                    1.0
                } else {
                    min_nan(1.0, ratio)
                };
                let next = tw.from + (tw.to - tw.from) * tw.ease.eval(raw);
                (next, raw >= 1.0)
            });
            if let Some((next, finished)) = advanced {
                self.velocity = (next - self.angle) / max_nan(dt, 1e-4);
                self.angle = next;
                if finished {
                    if using_reset {
                        self.spring_reset_tween = None;
                        self.velocity = 0.0;
                    }
                    self.tween = None;
                }
            }
        } else {
            let omega = self.config.omega;
            let zeta = self.config.zeta;
            let k = omega * omega;
            let c = 2.0 * zeta * omega;
            let steps = max_nan(1.0, (dt / (1.0 / 480.0)).ceil());
            let steps = if steps.is_finite() && steps >= 1.0 {
                steps as u32
            } else {
                1
            };
            let h = dt / steps as f64;
            for _ in 0..steps {
                let a = k * (self.target - self.angle) - c * self.velocity;
                self.velocity += a * h;
                self.angle += self.velocity * h;
            }
        }

        // micro-wobble：振幅跟随指针速度，所以只在剧烈变化时才看得出来
        let slew = self.velocity.abs();
        let amp_target = min_nan(self.config.wobble_max, slew * self.config.wobble_gain);
        self.wobble_amp += (amp_target - self.wobble_amp) * min_nan(1.0, dt / 0.09);
        self.wobble_phase += dt * self.config.wobble_hz * std::f64::consts::PI * 2.0;
        let wobble = self.wobble_phase.sin() * self.wobble_amp;

        clamp(
            self.angle + wobble,
            GEO.start_angle - 2.0,
            GEO.end_angle + 2.0,
        )
    }
}

/// 单帧 dt 上限（秒）：`Math.min(0.05, dt)`（`animator.ts:219`）。
///
/// 后台标签页/被挂起的窗口回到前台时不会爆冲。
pub fn clamp_dt(dt: f64) -> f64 {
    min_nan(0.05, dt)
}

/// 不依赖真实时钟的帧节拍器（`animator.ts:198-231` 的 `Clock` 的可测等价物）。
///
/// 调用方把"当前毫秒数"喂进来（Slint 侧用 `std::time::Instant` 或计时器回调的时间戳），
/// 结构本身不读时钟，因此单测可以精确控制 dt。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepClock {
    last_ms: f64,
}

impl StepClock {
    /// 以当前毫秒数起点构造；首帧 dt 为 0（与源码 `start()` 里 `last = now()` 一致）。
    pub fn new(now_ms: f64) -> Self {
        Self { last_ms: now_ms }
    }

    /// 推进到 `now_ms`，返回 clamp 过的 dt（秒）。
    pub fn tick(&mut self, now_ms: f64) -> f64 {
        let dt = (now_ms - self.last_ms) / 1000.0;
        self.last_ms = now_ms;
        clamp_dt(dt)
    }

    /// 重新对表（例如窗口被暂停之后）。
    pub fn rebase(&mut self, now_ms: f64) {
        self.last_ms = now_ms;
    }

    /// 上一次 tick 的毫秒时间戳。
    pub fn last_ms(&self) -> f64 {
        self.last_ms
    }
}
