//! `DemoSource`：确定性 demo 数据源（逐行等价移植 `pipeline.ts:51-127`）。
//!
//! 复刻的时序（默认参数，wall clock，单位 ms）：
//!
//! ```text
//! t=0      phase(ping);      emit(ping, 0)
//! t=1200   phase(download);  段内 t_local=0,100,…,5600 共 57 个样本
//! t=6900   段结束 -> phase(upload)（不发样本）+ 620ms 指针归位间隔
//! t=7520   phase(upload) 再一次（源码 runMeasuredPhase 开头也调 phase）;
//!          段内 t_local=0,100,…,5600 共 57 个样本
//! t=13220  phase(done);      emit(done, download_peak*1000); 之后无事件
//! ```
//!
//! # 与原实现的唯一实质差异：时间由外部显式推进
//!
//! 源码用 `performance.now()` + `window.setTimeout` 链自驱（`pipeline.ts:79-96`）。
//! Rust 侧改成**显式步进**：[`DemoSource::tick`] 接收 `dt_ms`，内部维护一条虚拟时钟与
//! "下一个到期事件"，`setTimeout(fn, 100)` 等价于"在 `due + 100` 处排下一个事件"。
//! 因此：
//!
//! - **时间基准仍然是段内相对时间** `t = due - segment_start`，与源码 `now - started` 同义；
//! - 样本时间点严格落在 `k*100` 网格上（源码因 `setTimeout` 漂移会略大于 `k*100`，
//!   这是唯一被"理想化"的地方——正是它让黄金向量可复现）；
//! - `tick` 的分割方式不影响结果：`tick(100)×200` 与 `tick(20000)×1` 产出同一序列。
//!
//! # 逐条记录的可观察差异
//!
//! | 源码 | 本实现 | 理由 |
//! |---|---|---|
//! | `stop()` 只置 `stopped`，`this.timer` 清一个 | `stop()` 置 `stopped` 并清掉待发事件 | 等价的"取消定时器"；Rust 侧事件是单链，清一个就是清全部 |
//! | 运行结束后 `stopped` 仍为 `false` | [`DemoSource::is_running`] 在 `done` 发完后返回 `false` | 源码没有该访问器；"没有待发事件"没有更好的解释 |
//! | `emit` 用 `Math.max(0, kbps)` | `f64::max(0.0)` | `NaN` 时源码得 `NaN`、Rust 得 `0.0`；只有非有限 peaking 才可达 |
//! | `onPhase` / `onDone` 两个回调 | [`DemoSource::set_phase_listener`] 一个回调 | `onDone` 恒紧跟在 `phase('done')` 之后（`pipeline.ts:101-103`），用 `stage()==Done` 判定完全等价 |
//! | `holdMs` 原样参与 `rampMs + holdMs` | 构造期兜底：非有限或负值 → 4200 | 否则 `t >= total` 永假 → 无限事件；正常参数下不可达 |
//!
//! # 必须继承的"怪行为"
//!
//! `finish()` 发出的 done 读数用的是**下载峰值** `dl * 1000`（`pipeline.ts:102`），
//! 不是上传峰值。源码如此，本实现原样保留（[`DemoSource`] 的 done 样本恒为
//! `download_peak_mbps * 1000.0`）。

use crate::data_source::{SampleListener, SpeedSample, SpeedSource};
use crate::model::Stage;

/// 打点节拍（`pipeline.ts:76` 硬编码 `tickMs = 100`）。
pub const TICK_MS: f64 = 100.0;
/// 爬坡时长（`pipeline.ts:77` 硬编码 `rampMs = 1500`）。
pub const RAMP_MS: f64 = 1500.0;
/// ping 前置等待（`pipeline.ts:117`）。
pub const PING_WAIT_MS: f64 = 1200.0;
/// download 段结束到 upload 段开始的间隔（`pipeline.ts:115`）。
pub const STAGE_GAP_MS: f64 = 620.0;

/// 阶段切换回调（等价源码 `DemoOptions.onPhase`）。
///
/// 与源码一致**不去重**：`upload` 会在 t=6900 与 t=7520 各通知一次
/// （`pipeline.ts:112` 与 `:74` 都调了 `phase('upload')`）。
pub type PhaseListener = Box<dyn FnMut(Stage)>;

/// demo 参数（`pipeline.ts:35-45` 去掉两个回调后的纯数据部分）。
///
/// 默认值 = 源码默认值（`pipeline.ts:61-64`，也是 `main.ts` 运行时实际生效值）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DemoOptions {
    /// 下载峰值，Mbps（`pipeline.ts:61` = 862）。
    pub download_peak_mbps: f64,
    /// 上传峰值，Mbps（`pipeline.ts:62` = 341）。
    pub upload_peak_mbps: f64,
    /// 随机种子（`pipeline.ts:64` = 20260101）。
    pub seed: u32,
    /// 平台期时长，ms（`pipeline.ts:63` = 4200）。
    pub hold_ms: f64,
}

impl Default for DemoOptions {
    fn default() -> Self {
        Self {
            download_peak_mbps: 862.0,
            upload_peak_mbps: 341.0,
            seed: 20_260_101,
            hold_ms: 4200.0,
        }
    }
}

/// `mulberry32`（`pipeline.ts:24-33`）的逐位等价实现。
///
/// 每一步都用 `u32` + `wrapping_*`：`Math.imul` 是"wrapping 32 位乘法"，
/// `>>>` 是 u32 逻辑右移，`(a + b) ^ c` 里 JS 的 number 加法再 ToInt32 与
/// `wrapping_add` 在模 2^32 下完全一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mulberry32 {
    a: u32,
}

impl Mulberry32 {
    /// 以 `seed` 初始化（源码 `seed >>> 0`；`u32` 天然就是这个域）。
    pub fn new(seed: u32) -> Self {
        Self { a: seed }
    }

    /// 下一个 `[0, 1)` 的样本。
    pub fn next_f64(&mut self) -> f64 {
        self.a = self.a.wrapping_add(0x6d2b_79f5);
        let mut t = self.a;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        let s = (t ^ (t >> 7)).wrapping_mul(t | 61);
        t ^= t.wrapping_add(s);
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

/// 测量段（只有这两段会打点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Segment {
    Download,
    Upload,
}

impl Segment {
    /// 该段打点用的 stage。
    fn stage(self) -> Stage {
        match self {
            Segment::Download => Stage::Download,
            Segment::Upload => Stage::Upload,
        }
    }
}

/// 时间线上唯一待执行的事件种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    /// `phase('ping'); emit('ping', 0)`。
    PingEmit,
    /// `phase(stage)` + 段起点（同一瞬间立刻打第一个样本）。
    BeginSegment(Segment),
    /// 段内一格：`t_local >= total` 时执行段的 `next()`，否则打点并排下一格。
    Step(Segment),
}

/// 待执行事件 + 绝对到期时刻（虚拟时钟口径）。
#[derive(Debug, Clone, Copy, PartialEq)]
struct Scheduled {
    due: f64,
    event: Event,
}

/// 确定性 demo 数据源（`pipeline.ts:51-127`）。
///
/// 用法（宿主每帧或定时器里推进）：
///
/// ```
/// use speed_gauge_slint::demo_source::{DemoOptions, DemoSource};
/// use speed_gauge_slint::model::Stage;
///
/// let mut source = DemoSource::new(DemoOptions::default());
/// source.set_listener(Box::new(|s| println!("{:?} {}", s.stage, s.kbps)));
/// // 第一拍就会吐出 ping 样本（t=0 的事件在首次 tick 时到期）。
/// assert_eq!(source.tick(100.0), 1);
/// assert_eq!(source.stage(), Stage::Ping);
/// ```
pub struct DemoSource {
    opts: DemoOptions,
    rnd: Mulberry32,
    /// 虚拟时钟（ms），只在 `tick` 里前进。
    now: f64,
    /// 当前阶段（等价 `Gauge.stage`，源码由宿主在 `onPhase` 里同步）。
    stage: Stage,
    /// 当前测量段的起点（绝对时刻），`t_local = due - segment_start`。
    segment_start: f64,
    /// 待执行事件；`None` ⇒ 一轮跑完（或已 stop）。
    scheduled: Option<Scheduled>,
    listener: Option<SampleListener>,
    phase_listener: Option<PhaseListener>,
    stopped: bool,
}

impl DemoSource {
    /// 构造并武装时间线：`t=0` 的 ping 事件已排好，但还没有回调。
    ///
    /// 等价于源码 `start()` 的事件侧（源码 `stop()` 之后 `stopped=true`，`start()` 才置
    /// `false`；Rust 侧 `new` 就是唯一的入口，因此直接处于"已武装"状态）。
    pub fn new(opts: DemoOptions) -> Self {
        let opts = DemoOptions {
            // 见模块文档的差异表：兜底避免 `t >= total` 永假。
            hold_ms: if opts.hold_ms.is_finite() && opts.hold_ms >= 0.0 {
                opts.hold_ms
            } else {
                DemoOptions::default().hold_ms
            },
            ..opts
        };
        let mut source = Self {
            opts,
            rnd: Mulberry32::new(opts.seed),
            now: 0.0,
            stage: Stage::Idle,
            segment_start: 0.0,
            scheduled: Some(Scheduled {
                due: 0.0,
                event: Event::PingEmit,
            }),
            listener: None,
            phase_listener: None,
            stopped: false,
        };
        source.reset();
        source
    }

    /// 注册样本回调（不影响时间线；要重开一轮用 [`Self::reset`] 或 [`SpeedSource::start`]）。
    pub fn set_listener(&mut self, cb: SampleListener) {
        self.listener = Some(cb);
    }

    /// 注册阶段回调（等价源码 `DemoOptions.onPhase`）。
    pub fn set_phase_listener(&mut self, cb: PhaseListener) {
        self.phase_listener = Some(cb);
    }

    /// 推进 `dt_ms`，把这段时间内到期的事件全部执行。返回本次发出的样本条数。
    ///
    /// - `dt_ms <= 0` / 非有限值：不推进时钟，但仍会执行"已经到期"的事件
    ///   （`tick(0.0)` 因此可以当 flush 用）。
    /// - `stopped` 时恒返回 0。
    pub fn tick(&mut self, dt_ms: f64) -> usize {
        if self.stopped {
            return 0;
        }
        if dt_ms.is_finite() && dt_ms > 0.0 {
            self.now += dt_ms;
        }
        let mut emitted = 0usize;
        while let Some(sched) = self.scheduled {
            if sched.due > self.now {
                break;
            }
            self.scheduled = None;
            emitted += self.fire(sched);
        }
        emitted
    }

    /// 回到 `t=0` 并**重新播种**：样本序列与首次运行完全一致。保留已注册的回调。
    pub fn reset(&mut self) {
        self.rnd = Mulberry32::new(self.opts.seed);
        self.now = 0.0;
        self.stage = Stage::Idle;
        self.segment_start = 0.0;
        self.stopped = false;
        self.scheduled = Some(Scheduled {
            due: 0.0,
            event: Event::PingEmit,
        });
    }

    /// 当前阶段（`phase()` 最近一次通知的值）。
    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// 还有待发事件且没被 `stop`。`done` 样本发完后返回 `false`。
    pub fn is_running(&self) -> bool {
        !self.stopped && self.scheduled.is_some()
    }

    /// 当前虚拟时钟（ms）。
    pub fn elapsed_ms(&self) -> f64 {
        self.now
    }

    /// 生效参数（含构造期兜底后的值）。
    pub fn options(&self) -> DemoOptions {
        self.opts
    }

    /// 执行一个到期事件，返回它发出的样本数。
    fn fire(&mut self, sched: Scheduled) -> usize {
        match sched.event {
            Event::PingEmit => {
                // pipeline.ts:107-108
                self.set_stage(Stage::Ping);
                self.emit(SpeedSample::new(0.0, Stage::Ping));
                self.scheduled = Some(Scheduled {
                    due: sched.due + PING_WAIT_MS,
                    event: Event::BeginSegment(Segment::Download),
                });
                1
            }
            Event::BeginSegment(segment) => {
                // runMeasuredPhase 的 `phase(stage)`（pipeline.ts:75），
                // 且首个样本与它同一瞬间（源码 loop() 是同步直呼，pipeline.ts:96）。
                self.set_stage(segment.stage());
                self.segment_start = sched.due;
                self.scheduled = Some(Scheduled {
                    due: sched.due,
                    event: Event::Step(segment),
                });
                0
            }
            Event::Step(segment) => {
                let t_local = sched.due - self.segment_start;
                let total = RAMP_MS + self.opts.hold_ms;
                if t_local >= total {
                    // 段的 `next()`（pipeline.ts:83-86）
                    self.end_segment(segment, sched.due);
                    0
                } else {
                    let sample =
                        SpeedSample::new(self.sample_kbps(segment, t_local), segment.stage());
                    self.emit(sample);
                    self.scheduled = Some(Scheduled {
                        due: sched.due + TICK_MS,
                        event: Event::Step(segment),
                    });
                    1
                }
            }
        }
    }

    /// 段结束：download → `phase('upload')` + 620ms 后开 upload；upload → `finish()`。
    fn end_segment(&mut self, segment: Segment, due: f64) {
        match segment {
            Segment::Download => {
                // pipeline.ts:111-115：外层 `next()` 里先换阶段（指针开始归位），再等 620ms。
                self.set_stage(Stage::Upload);
                self.scheduled = Some(Scheduled {
                    due: due + STAGE_GAP_MS,
                    event: Event::BeginSegment(Segment::Upload),
                });
            }
            Segment::Upload => {
                // finish()（pipeline.ts:99-104）：done 的读数用**下载峰值**，源码如此。
                self.set_stage(Stage::Done);
                self.emit(SpeedSample::new(
                    self.opts.download_peak_mbps * 1000.0,
                    Stage::Done,
                ));
                self.scheduled = None;
            }
        }
    }

    /// 单点样本公式（`pipeline.ts:87-93`）。
    fn sample_kbps(&mut self, segment: Segment, t: f64) -> f64 {
        let peak = match segment {
            Segment::Download => self.opts.download_peak_mbps,
            Segment::Upload => self.opts.upload_peak_mbps,
        };
        let ramp_t = (t / RAMP_MS).min(1.0);
        // `1 - Math.pow(1 - rampT, 3)`：三连乘在 IEEE754 下与 `x*x*x` 逐位一致。
        let envelope = if ramp_t < 1.0 {
            let q = 1.0 - ramp_t;
            1.0 - q * q * q
        } else {
            1.0
        };
        let seconds = t / 1000.0;
        let drift = (seconds * 1.7).sin() * 0.012 + (seconds * 4.3).sin() * 0.006;
        let noise = (self.rnd.next_f64() - 0.5) * 0.03;
        let mbps = peak * envelope * (1.0 + drift + noise);
        // emit 里的 `Math.max(0, kbps)`（pipeline.ts:67）。
        (mbps * 1000.0).max(0.0)
    }

    /// `phase(stage)`（`pipeline.ts:69-72`）：**不去重**，每次都通知。
    fn set_stage(&mut self, stage: Stage) {
        if self.stopped {
            return;
        }
        self.stage = stage;
        if let Some(cb) = self.phase_listener.as_mut() {
            cb(stage);
        }
    }

    /// `emit(stage, kbps)`（`pipeline.ts:65-68`）。
    fn emit(&mut self, sample: SpeedSample) {
        if self.stopped {
            return;
        }
        if let Some(cb) = self.listener.as_mut() {
            cb(sample);
        }
    }
}

impl SpeedSource for DemoSource {
    fn name(&self) -> &'static str {
        "demo"
    }

    /// 等价源码 `start()`：先 `stop()`、再重开一轮（`pipeline.ts:59-60`）。
    fn start(&mut self, cb: SampleListener) {
        self.listener = Some(cb);
        self.reset();
    }

    /// 等价源码 `stop()`：置 `stopped` 并作废待发定时器（`pipeline.ts:120-126`）。
    fn stop(&mut self) {
        self.stopped = true;
        self.scheduled = None;
    }
}
