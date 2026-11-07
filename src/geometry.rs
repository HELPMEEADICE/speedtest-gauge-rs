//! 极坐标 / SVG 路径字符串 / 几何常量表。
//!
//! 逐行等价移植：
//! - `src/gauge/geometry.ts`（全文 39 行）：[`polar`] [`arc_path`] [`sector_path`] [`clamp`]
//! - `src/gauge/tokens.ts:9-50`：`GEO` 与 `TEXT_BLOCK`
//!
//! 全部运算在 `f64` 域完成（JS number 的等价物），单位一律为 SVG 用户单位
//! （`viewBox="0 0 286 286"`）。

use std::f64::consts::PI;

/// 仪表几何常量（`tokens.ts:9-42` 的 `GEO`，字段名一一对应）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geo {
    /// 圆弧中心半径（描边中线），`tokens.ts:11` = 130。
    pub radius: f64,
    /// 圆弧描边宽度，`tokens.ts:13` = 23。
    pub stroke_width: f64,
    /// 容器边长 = `radius * 2.2`，`tokens.ts:15` = 286。
    pub size: f64,
    /// `centerX = centerY = size / 2`，`tokens.ts:17` = 143。
    pub center: f64,
    /// 起始角（0° 朝上、顺时针为正），`tokens.ts:19` = -135。
    pub start_angle: f64,
    /// 结束角，`tokens.ts:20` = 135。
    pub end_angle: f64,
    /// 刻度文字半径 = `radius - 33`，`tokens.ts:22` = 97。
    pub label_radius: f64,
    /// 刻度字号，`tokens.ts:24` = 14。
    pub label_font_size: f64,
    /// 刻度字重（bold），`tokens.ts:25` = 700。
    pub label_font_weight: u16,
    /// 指针根部宽度，`tokens.ts:27` = 20。
    pub needle_base: f64,
    /// 指针尖端宽度，`tokens.ts:28` = 10。
    pub needle_tip: f64,
    /// 指针长度（自圆心向上），`tokens.ts:29` = 85。
    pub needle_length: f64,
    /// 数字字号（`text-4xl`），`tokens.ts:31` = 36。
    pub number_size: f64,
    /// 数字字重（`font-medium`），`tokens.ts:32` = 500。
    pub number_weight: u16,
    /// 单位行字号（`text-sm`），`tokens.ts:34` = 14。
    pub unit_size: f64,
    /// 图标与 `"Mbps"` 的水平间距，`tokens.ts:35` = 4。
    pub unit_gap: f64,
    /// 图标名义边长，`tokens.ts:36` = 16。
    pub unit_icon_size: f64,
    /// 文字块底边距（`bottom-4`），`tokens.ts:38` = 16。
    pub bottom_inset: f64,
    /// 数字行高（`2.5rem`），`tokens.ts:40` = 40。
    pub number_line_height: f64,
    /// 单位行行高（`text-sm` = 20px），`tokens.ts:41` = 20。
    pub unit_line_height: f64,
}

/// `GEO` 单例（源码是 `as const` 对象，这里用 `const` 结构体表达）。
pub const GEO: Geo = Geo {
    radius: 130.0,
    stroke_width: 23.0,
    size: 286.0,
    center: 143.0,
    start_angle: -135.0,
    end_angle: 135.0,
    label_radius: 97.0,
    label_font_size: 14.0,
    label_font_weight: 700,
    needle_base: 20.0,
    needle_tip: 10.0,
    needle_length: 85.0,
    number_size: 36.0,
    number_weight: 500,
    unit_size: 14.0,
    unit_gap: 4.0,
    unit_icon_size: 16.0,
    bottom_inset: 16.0,
    number_line_height: 40.0,
    unit_line_height: 20.0,
};

/// sweep 裁剪半径 = `radius + stroke_width / 2`（`gauge.ts:445`），141.5。
pub const SWEEP_RADIUS: f64 = GEO.radius + GEO.stroke_width / 2.0;

/// 底部文字块底边：`size - bottomInset` = 270（`tokens.ts:46`）。
pub const TEXT_BLOCK_BOTTOM: f64 = GEO.size - GEO.bottom_inset;
/// 单位行垂直中心：`bottom - unitLineHeight / 2` = 260（`tokens.ts:47`）。
pub const TEXT_BLOCK_UNIT_CENTER: f64 = TEXT_BLOCK_BOTTOM - GEO.unit_line_height / 2.0;
/// 数字垂直中心，`tokens.ts:48` = 226。
pub const TEXT_BLOCK_NUMBER_CENTER: f64 = TEXT_BLOCK_UNIT_CENTER
    - GEO.unit_line_height / 2.0
    - GEO.unit_gap
    - GEO.number_line_height / 2.0;

/// 平面点（`geometry.ts:3-6` 的 `Point`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// 横坐标，SVG 用户单位。
    pub x: f64,
    /// 纵坐标，SVG 用户单位（向下为正）。
    pub y: f64,
}

/// 极坐标 → 直角坐标（`geometry.ts:11-14`）。
///
/// `rad = (angle_deg - 90) * PI / 180`；`0°` 朝上，顺时针为正。
pub fn polar(cx: f64, cy: f64, radius: f64, angle_deg: f64) -> Point {
    let rad = ((angle_deg - 90.0) * PI) / 180.0;
    Point {
        x: cx + radius * rad.cos(),
        y: cy + radius * rad.sin(),
    }
}

/// JS `Math.min(a, b)`：任一为 `NaN` 则结果为 `NaN`（Rust 的 `f64::min` 会返回另一操作数）。
pub fn min_nan(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else {
        b
    }
}

/// JS `Math.max(a, b)`：任一为 `NaN` 则结果为 `NaN`。
pub fn max_nan(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

/// `geometry.ts:8` 的 `r3`：`(Math.round(v * 1000) / 1000).toString()` 的逐字符等价物。
///
/// 两个 JS 语义细节：
/// 1. `Math.round(x) === floor(x + 0.5)`（向 +∞ 取整），不是 Rust 的 `f64::round`
///    （远离零取整，`round(-2.5) == -3`，而 JS 给 `-2`）。
/// 2. `Number.prototype.toString()` 输出最短往返表示，整数不带 `.0`，`-0` 输出 `"0"`；
///    Rust 的 `f64: Display` 同为最短往返表示，但 `-0.0` 会打出 `"-0"`，故 0 单独处理。
/// 3. 非有限值：JS 打 `"NaN"` / `"Infinity"` / `"-Infinity"`，Rust 的 `Display` 打
///    `"NaN"` / `"inf"`，因此显式映射（源码路径上不会出现，纯防御）。
pub fn r3(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    let q = (v * 1000.0 + 0.5).floor() / 1000.0;
    if q == 0.0 {
        return "0".to_string();
    }
    format!("{}", q)
}

/// `toFixed(3)` 的等价物（`gauge.ts:448` 的 `rotate(${angle.toFixed(3)} ...)`，port-spec §4 规则 5）。
///
/// JS 的 `(-0).toFixed(3)` 给 `"0.000"`，Rust 的 `format!("{:.3}", -0.0)` 给 `"-0.000"`，故 0 特判。
pub fn fixed3(v: f64) -> String {
    if v == 0.0 {
        return "0.000".to_string();
    }
    format!("{:.3}", v)
}

/// large-arc 标志（`geometry.ts:20`）：**未取整**的 `|a1 - a0| > 180`，`== 180` 走 0。
pub fn large_arc(a0: f64, a1: f64) -> u8 {
    if (a1 - a0).abs() > 180.0 {
        1
    } else {
        0
    }
}

/// 描边弧路径（`geometry.ts:17-22`，源码 `describeArc`）。
///
/// 分隔符是单个空格、没有逗号；`A` 之后顺序为 `rx ry x-axis-rotation large-arc-flag
/// sweep-flag x y`，x 轴旋转角恒 `0`、sweep 恒 `1`（顺时针）。
pub fn arc_path(cx: f64, cy: f64, radius: f64, a0: f64, a1: f64) -> String {
    let p0 = polar(cx, cy, radius, a0);
    let p1 = polar(cx, cy, radius, a1);
    let large = large_arc(a0, a1);
    format!(
        "M {} {} A {} {} 0 {} 1 {} {}",
        r3(p0.x),
        r3(p0.y),
        r3(radius),
        r3(radius),
        large,
        r3(p1.x),
        r3(p1.y)
    )
}

/// 扇形路径（`geometry.ts:25-35`，源码 `describeSector`），用作 sweep 裁剪。
///
/// 顶点顺序：圆心 → a0 点 → 顺时针弧 → a1 点 → `Z`；large-arc 与 [`arc_path`] 同一判定。
pub fn sector_path(cx: f64, cy: f64, radius: f64, a0: f64, a1: f64) -> String {
    let p0 = polar(cx, cy, radius, a0);
    let p1 = polar(cx, cy, radius, a1);
    let large = large_arc(a0, a1);
    format!(
        "M {} {} L {} {} A {} {} 0 {} 1 {} {} Z",
        r3(cx),
        r3(cy),
        r3(p0.x),
        r3(p0.y),
        r3(radius),
        r3(radius),
        large,
        r3(p1.x),
        r3(p1.y)
    )
}

/// `geometry.ts:37-39`：`v < lo ? lo : (v > hi ? hi : v)`。
///
/// 等价于 `v.clamp(lo, hi)`：`NaN` 直通（`f64::clamp` 同样返回 `NaN`），
/// 且调用点恒满足 `lo < hi`，不会触发 `clamp` 的 panic 前提。
pub fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.clamp(lo, hi)
}
