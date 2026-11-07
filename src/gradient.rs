//! 角向渐变：stop 表的线性插值采样与 240 段圆弧段表。
//!
//! 逐行等价移植 `src/gauge/gauge.ts`：
//! - `63-74` `sampleStops`、`50-61` `hexToRgb` / `lerpHex`；
//! - `357-379` `renderVariant`（每段角度公式、`overlap`、末段 `min` 截断）；
//! - `128` `ringSegments` 默认值 240。
//!
//! 颜色一律归一成 `[u8; 3]` 的 sRGB 值（源码 `Math.round` 逐通道量化后的结果）。

use crate::geometry::{arc_path, min_nan, GEO};
use crate::gradient_data::{DL_MULTI, DL_SINGLE, UL_MULTI, UL_SINGLE};
use crate::model::GradientVariant;

/// 圆环默认段数（`gauge.ts:128` 的 `options.ringSegments ?? 240`）。
pub const DEFAULT_SEGMENTS: usize = 240;

/// 相邻段的角度重叠量（度），用来掩盖 butt 端帽的抗锯齿发丝缝（`gauge.ts:363`）。
pub const OVERLAP_DEG: f64 = 0.28;

/// 一条圆弧段：`d` 为 `arcPath` 输出，`color` 为该段中心的采样色。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingSegment {
    /// `M x0 y0 A r r 0 <large> 1 x1 y1`。
    pub d: String,
    /// `[r, g, b]`，8-bit sRGB。
    pub color: [u8; 3],
}

/// 取某个变体的 stop 表（`gradient-stops.ts` 的四组，各 28 条）。
pub fn stops(variant: GradientVariant) -> &'static [(f64, [u8; 3])] {
    match variant {
        GradientVariant::DlMulti => DL_MULTI,
        GradientVariant::UlMulti => UL_MULTI,
        GradientVariant::DlSingle => DL_SINGLE,
        GradientVariant::UlSingle => UL_SINGLE,
    }
}

/// 逐通道 `Math.round(a + (b - a) * k)`（`gauge.ts:55-61` 的 `lerpHex`）。
fn lerp(a: [u8; 3], b: [u8; 3], k: f64) -> [u8; 3] {
    let mut out = [0u8; 3];
    for (i, slot) in out.iter_mut().enumerate() {
        let from = a[i] as f64;
        let to = b[i] as f64;
        let v = from + (to - from) * k;
        // Math.round 语义 = floor(v + 0.5)，对非负通道值等价；再夹到 0..=255 防量化溢出。
        *slot = (v + 0.5).floor().clamp(0.0, 255.0) as u8;
    }
    out
}

/// 分段线性采样（`gauge.ts:63-74` 的 `sampleStops`）。
///
/// - `clamp(t, 0, 1)`；
/// - 找 `x >= a.t && x <= b.t` 的**第一个**区间（相邻区间在端点处都命中，先命中的是左边
///   那一对，此时 `k` 恰为 1 或 0，两边等价）；
/// - `span == 0` 时 `k = 0`；
/// - 逐通道 `round` 后返回 8-bit 色。
pub fn sample_stops(stops: &[(f64, [u8; 3])], t: f64) -> [u8; 3] {
    let x = t.clamp(0.0, 1.0);
    for pair in stops.windows(2) {
        let (at, a) = pair[0];
        let (bt, b) = pair[1];
        if x >= at && x <= bt {
            let span = bt - at;
            let k = if span == 0.0 { 0.0 } else { (x - at) / span };
            return lerp(a, b, k);
        }
    }
    // 兜底：源码返回 `stops[last].hex`（字符串形态），这里归一成颜色值。
    stops.last().map(|(_, c)| *c).unwrap_or([0, 0, 0])
}

/// 第 `i` 段（共 `n` 段）的采样色：`sampleStops(stops, (i + 0.5) / n)`（`gauge.ts:372`）。
///
/// 采样点取段中心而非段起点，所以首段 `t = 0.5/n`、末段 `t = (n-0.5)/n`：
/// 弧的两端永远不会用到表首/表尾色。
pub fn ring_segment_color(variant: GradientVariant, i: usize, n: usize) -> [u8; 3] {
    if n == 0 {
        return [0, 0, 0];
    }
    let t = (i as f64 + 0.5) / n as f64;
    sample_stops(stops(variant), t)
}

/// 第 `i` 段（共 `n` 段）的角度区间 `(a0, a1)`，单位度。
///
/// ```text
/// a0 = start + span * i / n
/// a1 = min(end, start + span * (i + 1) / n + 0.28)
/// ```
///
/// `a1` 已含 [`OVERLAP_DEG`]，并会被 `end` 截断 —— 因此**末段没有 overlap 余量**。
///
/// 下游若要替代 SVG 的 `clip-path: path()`（Slint 不支持任意路径裁剪），做法是
/// "只画 `a0 < arcAngle` 的段，并把跨边界的那段截到 `a1_eff = min(a1, arcAngle)`"，
/// 这时必须能拿到每段的 `a0`/`a1`：本函数就是给这个用途的。
pub fn segment_angles(i: usize, n: usize) -> (f64, f64) {
    if n == 0 {
        return (GEO.start_angle, GEO.start_angle);
    }
    let span = GEO.end_angle - GEO.start_angle;
    let count = n as f64;
    let index = i as f64;
    let a0 = GEO.start_angle + span * index / count;
    let a1 = min_nan(
        GEO.end_angle,
        GEO.start_angle + span * (index + 1.0) / count + OVERLAP_DEG,
    );
    (a0, a1)
}

/// 生成整张段表（`gauge.ts:365-377`）。
///
/// 每段见 [`segment_angles`]；颜色是段中心 `(i + 0.5) / n` 处的采样值。
///
/// **末段（`i == n-1`）会被 `min` 截断**，因此没有 overlap 余量：角宽 `1.125°` 而不是
/// `1.405°`（`n = 240`）。这是源码真实行为（port-spec §3.2 / 附录 B7），不要"修好"它。
pub fn ring_segments(
    variant: GradientVariant,
    n: usize,
    radius: f64,
    cx: f64,
    cy: f64,
) -> Vec<RingSegment> {
    if n == 0 {
        return Vec::new();
    }
    let table = stops(variant);
    let count = n as f64;
    (0..n)
        .map(|i| {
            let (a0, a1) = segment_angles(i, n);
            RingSegment {
                d: arc_path(cx, cy, radius, a0, a1),
                color: sample_stops(table, (i as f64 + 0.5) / count),
            }
        })
        .collect()
}

/// 按仪表默认几何（`center=143`、`radius=130`、`n=240`）生成段表。
pub fn ring_segments_default(variant: GradientVariant) -> Vec<RingSegment> {
    ring_segments(
        variant,
        DEFAULT_SEGMENTS,
        GEO.radius,
        GEO.center,
        GEO.center,
    )
}
