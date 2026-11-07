//! 可复用的原生 Slint 动态测速仪表盘（speedtest-gauge-svg 的 Rust + Slint 移植）。
//!
//! 分层：
//! - 数值层（逐行等价于原项目 `src/gauge/*.ts`，不含 UI）：
//!   [`geometry`] [`scale`] [`model`] [`gradient`] [`gradient_data`] [`animation`]
//! - 视图桥接：[`renderer`]（uu → `Path.commands` / 环段表 / 指针烘焙 / 布局量）、
//!   [`gauge::GaugeHandle`]（状态机 + 动画泵 + 属性推送）
//! - 组件：[`SpeedGauge`]（`ui/speed-gauge.slint`，286 uu 坐标系，纯矢量）
//! - 自证工具：[`shots`]（无窗口软件渲染 → PNG，feature `screenshots`）
//!
//! 快速上手：
//! ```no_run
//! # use speed_gauge_slint::{GaugeHandle, GaugeStage, SpeedGauge};
//! # fn demo() -> Result<(), slint::PlatformError> {
//! let ui = SpeedGauge::new()?;
//! let gauge = GaugeHandle::attach(&ui);
//! gauge.set_stage(GaugeStage::Download);
//! gauge.set_speed_kbps(862_000.0);
//! ui.run()
//! # }
//! ```

// 纯 Rust 核心算法移植（逐行等价于原项目 src/gauge/*.ts，不含 UI / 渲染）。
pub mod animation;
// 数据层（等价于原项目 src/gauge/pipeline.ts）：样本来源抽象 + 确定性 demo。
pub mod data_source;
pub mod demo_source;
pub mod geometry;
pub mod gradient;
pub mod gradient_data;
pub mod model;
pub mod scale;

// 视图桥接。
pub mod gauge;
pub mod renderer;

// 指针整体光栅化（tiny-skia 画一张 RGBA，Slint `Image` 一次合成 ⇒ 无接缝）。
// 关掉 `needle-raster` 时本模块不参与编译，组件退回 32 片梯形平色路径。
#[cfg(feature = "needle-raster")]
pub mod needle_raster;

// 运行时字体注册（随包 Montserrat + 本机 Gauge Mono）。
pub mod fonts;

// 无窗口截图工具（依赖 tiny-skia 编码 PNG，故由 `screenshots` feature 门控）。
#[cfg(feature = "screenshots")]
pub mod shots;

// `SpeedGauge` / `GaugeStage` / `LabelSpec` / `RingSegment` 等由 slint-build 生成。
// `SLINT_INCLUDE_GENERATED` 只指向**最后一次** slint_build::compile 的产物（现在 = `ui/demo.slint`，
// 供 `examples/demo.rs` 的 `slint::include_modules!()`），所以本文件显式 include 自己的产物。
include!(concat!(env!("OUT_DIR"), "/speed-gauge.rs"));

pub use gauge::GaugeHandle;

/// 组件逻辑边长（uu）：`viewBox 0 0 286 286`。
pub const GAUGE_UU: f32 = 286.0;
