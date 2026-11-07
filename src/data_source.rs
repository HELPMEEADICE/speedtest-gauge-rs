//! 数据层：与 UI / 渲染完全解耦的样本来源抽象（等价移植 `src/gauge/pipeline.ts`）。
//!
//! 仪表只接收 `setSpeed(kbps)` + `setStage(stage)` 两件事（`pipeline.ts:2-4` 的注释），
//! 因此真实测速引擎、WebSocket 推送、内置 demo 全都走同一个 [`SpeedSource`] 接口。
//!
//! 本模块提供：
//! - [`SpeedSample`] / [`SampleListener`] / [`SpeedSource`] — `pipeline.ts:9-21`；
//! - [`ManualSource`] — `pipeline.ts:130-145`，宿主直接 `push`（"真实数据"缝）；
//! - [`ChannelSource`] + [`ChannelSender`] — `WebSocketSource`（`pipeline.ts:152-178`）的
//!   Rust 等价物。原实现绑死浏览器 `WebSocket`，Rust 侧没有等价物（也不该有），
//!   改成"引擎线程 send、UI 线程 poll"的通道模型，见 [`ChannelSource`] 的文档。
//!
//! `DemoSource` 在 [`crate::demo_source`]。
//!
//! # 线程模型
//!
//! - [`ManualSource`] / `ChannelSource` 本身**不要求** `Send`：回调是 `Box<dyn FnMut>`，
//!   只在调用它的那条线程上跑（通常是 UI 线程）。
//! - 跨线程的是 [`ChannelSender`]（内部 `std::sync::mpsc::Sender`）：把 sender `move`
//!   进引擎线程即可，**不需要 `Arc<Mutex<…>>` 包裹 source**。
//! - 不引入任何 async 运行时（tokio / async-std）：数据层是同步的 "push 队列 +
//!   宿主定时 poll"，UI 线程永远不做阻塞等待。

use std::sync::mpsc::{self, Receiver, Sender};

use crate::model::Stage;

/// 一个测速样本（`pipeline.ts:9-14`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeedSample {
    /// 千比特每秒——与生产数据源同一个单位。
    pub kbps: f64,
    /// 采样所属阶段。
    pub stage: Stage,
}

impl SpeedSample {
    /// 构造。
    pub fn new(kbps: f64, stage: Stage) -> Self {
        Self { kbps, stage }
    }
}

/// 样本回调。由 source 持有并调用；调用线程 = `push` / `poll` / `tick` 的调用线程。
///
/// 用 `Box<dyn FnMut>` 而不是 `Arc<dyn Fn>`：数据层只在一个线程上推进，
/// 不做跨线程回调分发（跨线程的部分交给 [`ChannelSender`]）。
pub type SampleListener = Box<dyn FnMut(SpeedSample)>;

/// 样本来源（`pipeline.ts:17-21`）。
///
/// # 契约
///
/// 1. [`SpeedSource::start`] 交出回调的所有权；`start` 之后到 `stop` 之前，
///    source 按自己的节拍调用回调。重复 `start` 等价于"先 stop 再 start"（源码
///    `DemoSource.start` 第一行就是 `this.stop()`）。
/// 2. [`SpeedSource::stop`] 之后**不再回调**；队列/定时器一律作废。
/// 3. `kbps` 单位恒定是 kbps，允许非整数，允许任意量级；clamp 由调用方（`GaugeState`）负责。
/// 4. `stage` 的合法取值就是 [`Stage`] 全表；数据层不做视觉决策。
///
/// 该 trait 是对象安全的，可以 `Box<dyn SpeedSource>` 统一持有。
pub trait SpeedSource {
    /// 源码里的 `readonly name`：`"demo"` / `"manual"` / `"channel"`。
    fn name(&self) -> &'static str;

    /// 注册回调并（对 demo 这类自驱 source）开始一轮新的运行。
    fn start(&mut self, cb: SampleListener);

    /// 停止：此后不再回调。
    fn stop(&mut self);
}

/// 手动数据源（`pipeline.ts:130-145`）：宿主推什么就是什么。这就是"真实数据"的缝。
///
/// 与源码一致：`push` **不做任何 clamp**，也不校验 `kbps` 是否有限；
/// 未 `start` 时 `push` 静默丢弃（源码 `this.listener?.()`）。
#[derive(Default)]
pub struct ManualSource {
    listener: Option<SampleListener>,
}

impl ManualSource {
    /// 构造（未注册回调）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 推一个样本（`pipeline.ts:142-144`）。`stage` 在源码里有默认值 `'download'`，
    /// Rust 没有默认参数，因此平级提供一个 [`Self::push_download`]。
    pub fn push(&mut self, kbps: f64, stage: Stage) {
        if let Some(cb) = self.listener.as_mut() {
            cb(SpeedSample { kbps, stage });
        }
    }

    /// 等价于 `push(kbps, 'download')`。
    pub fn push_download(&mut self, kbps: f64) {
        self.push(kbps, Stage::Download);
    }

    /// 是否已注册回调（即 `start` 过且没 `stop`）。
    pub fn is_started(&self) -> bool {
        self.listener.is_some()
    }
}

impl SpeedSource for ManualSource {
    fn name(&self) -> &'static str {
        "manual"
    }

    fn start(&mut self, cb: SampleListener) {
        self.listener = Some(cb);
    }

    fn stop(&mut self) {
        self.listener = None;
    }
}

/// 可跨线程搬运的样本：比 [`SpeedSample`] 多实现 `Send` 语义上的用途区分
/// （`Stage` 本身已经是 `Send + 'static`，这个类型只是把"通道里的东西"和
/// "回调里的东西"在类型上分开，避免误把一个 `!Send` 的 listener 混进队列）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelSample {
    /// 千比特每秒。
    pub kbps: f64,
    /// 采样所属阶段。
    pub stage: Stage,
}

impl ChannelSample {
    /// 构造。
    pub fn new(kbps: f64, stage: Stage) -> Self {
        Self { kbps, stage }
    }
}

impl From<ChannelSample> for SpeedSample {
    fn from(sample: ChannelSample) -> Self {
        Self {
            kbps: sample.kbps,
            stage: sample.stage,
        }
    }
}

/// [`ChannelSource`] 的生产端：交给测速引擎线程持有。
///
/// `Clone` 可以多线程各持一份同发；`send` 在接收端已销毁时返回 `false`，
/// 引擎线程据此收尾（不需要额外的中断标志）。
#[derive(Debug, Clone)]
pub struct ChannelSender {
    tx: Sender<ChannelSample>,
}

impl ChannelSender {
    /// 送一个样本。返回 `false` ⇒ 对端已销毁（source 被 drop 了），该停线程了。
    pub fn send(&self, kbps: f64, stage: Stage) -> bool {
        self.tx.send(ChannelSample { kbps, stage }).is_ok()
    }

    /// 送一个现成的样本。
    pub fn send_sample(&self, sample: ChannelSample) -> bool {
        self.tx.send(sample).is_ok()
    }
}

/// 通道数据源：`WebSocketSource`（`pipeline.ts:152-178`）的 Rust 等价物。
///
/// # 为什么不是 WebSocket
///
/// 原实现依赖浏览器 `WebSocket` API（`new WebSocket(url)` + `addEventListener('message')`），
/// Rust 侧既没有等价物，也不该在数据层硬凑一个网络栈。这里把"外部推送"抽象成一条
/// `std::sync::mpsc` 通道：
///
/// ```text
/// 引擎线程（你自己的 HTTP/WS 客户端）      UI 线程
///   sender.send(kbps, stage)  ─────►  source.poll()  ─────►  listener(sample)
/// ```
///
/// 你的 WS 客户端线程里 `serde_json` 解出 `{ "kbps": 123456, "stage": "download" }`
/// 之后调 `sender.send(...)` 即可，语义与 `WebSocketSource` 的 message handler 一一对应。
/// 具体接法见 `docs/real-backend.md`。
///
/// # 投递语义（重要）
///
/// [`ChannelSource::poll`] 把队列里积压的样本**按 FIFO 顺序全部**喂给回调，
/// **不保留最后一个、不丢中间样本**：指针动画（[`crate::animation`]）靠连续样本做
/// 跟踪，抽样/合并会让速度曲线出现"跳变"台阶。每次 `poll` 返回本次真正转发的条数。
///
/// 若上游打点比 UI 帧率快，队列会积压；需要限流的宿主应自己在 `poll` 之后按时间戳
/// 丢帧，而不是指望数据层抽样。
///
/// # 停止后
///
/// [`SpeedSource::stop`] 之后 `poll` 仍会把队列排空（防止无界增长），但**不再回调**，
/// 返回值恒为 0。被排空而丢弃的样本不会回灌。
pub struct ChannelSource {
    rx: Receiver<ChannelSample>,
    listener: Option<SampleListener>,
    stopped: bool,
}

impl ChannelSource {
    /// 建一条通道，返回 `(生产端, 消费端)`。
    ///
    /// 故意不叫 `new`：它不是"构造一个 Self"，而是"建一对端点"。
    ///
    /// ```
    /// use std::cell::Cell;
    /// use std::rc::Rc;
    ///
    /// use speed_gauge_slint::data_source::{ChannelSource, SpeedSource};
    /// use speed_gauge_slint::model::Stage;
    ///
    /// let (tx, mut source) = ChannelSource::channel();
    /// assert!(tx.send(1_500.0, Stage::Download));
    ///
    /// let hits = Rc::new(Cell::new(0));
    /// let sink = Rc::clone(&hits);
    /// source.start(Box::new(move |_sample| sink.set(sink.get() + 1)));
    ///
    /// // 每次 poll 把队列排空并按序回调。
    /// assert_eq!(source.poll(), 1);
    /// assert_eq!(hits.get(), 1);
    /// ```
    pub fn channel() -> (ChannelSender, ChannelSource) {
        let (tx, rx) = mpsc::channel();
        (
            ChannelSender { tx },
            ChannelSource {
                rx,
                listener: None,
                stopped: false,
            },
        )
    }

    /// 排空队列，按序回调。返回本次转发的样本条数。
    ///
    /// 未 `start`（没有回调）时样本被丢弃——等价于源码里 `listener?.()` 的空转。
    pub fn poll(&mut self) -> usize {
        let mut forwarded = 0usize;
        while let Ok(sample) = self.rx.try_recv() {
            if self.stopped {
                // 停止后只排空、不回调。
                continue;
            }
            if let Some(cb) = self.listener.as_mut() {
                cb(SpeedSample::from(sample));
                forwarded += 1;
            }
        }
        forwarded
    }
}

impl SpeedSource for ChannelSource {
    fn name(&self) -> &'static str {
        "channel"
    }

    fn start(&mut self, cb: SampleListener) {
        self.listener = Some(cb);
        self.stopped = false;
    }

    fn stop(&mut self) {
        self.listener = None;
        self.stopped = true;
    }
}
