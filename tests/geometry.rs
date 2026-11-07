//! `geometry` 模块的等价性测试。
//!
//! 黄金串来自 `docs/port-spec.md` 附录 A 与原项目 audit 实测值（README §4.4）。
//! 全部数值容差 1e-9；路径字符串是**逐字符**比较，没有容差。

use speed_gauge_slint::geometry::{
    arc_path, clamp, fixed3, large_arc, max_nan, min_nan, polar, r3, sector_path, GEO,
    SWEEP_RADIUS, TEXT_BLOCK_BOTTOM, TEXT_BLOCK_NUMBER_CENTER, TEXT_BLOCK_UNIT_CENTER,
};

const EPS: f64 = 1e-9;

fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < EPS
}

#[test]
fn polar_cardinal_directions() {
    // 0° 朝上、顺时针为正（SVG y 轴向下）。
    let up = polar(143.0, 143.0, 130.0, 0.0);
    assert!(close(up.x, 143.0), "0° x = {}", up.x);
    assert!(close(up.y, 13.0), "0° y = {}", up.y);

    let right = polar(143.0, 143.0, 130.0, 90.0);
    assert!(close(right.x, 273.0), "90° x = {}", right.x);
    assert!(close(right.y, 143.0), "90° y = {}", right.y);

    let left_down = polar(143.0, 143.0, 130.0, -135.0);
    assert!(
        close(left_down.x, 51.076118445748804),
        "-135° x = {}",
        left_down.x
    );
    assert!(
        close(left_down.y, 234.92388155425118),
        "-135° y = {}",
        left_down.y
    );

    let right_down = polar(143.0, 143.0, 130.0, 135.0);
    assert!(
        close(right_down.x, 234.92388155425118),
        "135° x = {}",
        right_down.x
    );
    assert!(
        close(right_down.y, 234.92388155425118),
        "135° y = {}",
        right_down.y
    );
}

#[test]
fn polar_label_anchors() {
    // 刻度锚点 = polar(center, center, labelRadius = 97, angle)
    let zero = polar(143.0, 143.0, 97.0, -135.0);
    assert_eq!(r3(zero.x), "74.411");
    assert_eq!(r3(zero.y), "211.589");

    let hundred = polar(143.0, 143.0, 97.0, 0.0);
    assert!(close(hundred.x, 143.0));
    assert!(close(hundred.y, 46.0), "100 刻度 y = {}", hundred.y);
}

/// 硬证据：轨道弧 `d` 必须与 audit 实测逐字符一致。
#[test]
fn arc_path_golden_track() {
    let d = arc_path(143.0, 143.0, 130.0, -135.0, 135.0);
    assert_eq!(
        d, "M 51.076 234.924 A 130 130 0 1 1 234.924 234.924",
        "轨道弧黄金串不匹配"
    );
}

#[test]
fn arc_path_golden_ring_segments() {
    // 第 0 段：a1 含 +0.28° overlap。
    assert_eq!(
        arc_path(143.0, 143.0, 130.0, -135.0, -133.595),
        "M 51.076 234.924 A 130 130 0 0 1 48.85 232.642"
    );
    // 第 239 段：被 min(end, ...) 截断，无 overlap 余量（附录 B7）。
    assert_eq!(
        arc_path(143.0, 143.0, 130.0, 133.875, 135.0),
        "M 236.711 233.101 A 130 130 0 0 1 234.924 234.924"
    );
}

#[test]
fn sector_path_goldens() {
    assert_eq!(
        sector_path(143.0, 143.0, 141.5, -135.0, 0.0),
        "M 143 143 L 42.944 243.056 A 141.5 141.5 0 0 1 143 1.5 Z"
    );
    // |45 - (-135)| == 180，严格大于才置 1，故 large 仍为 0。
    assert_eq!(
        sector_path(143.0, 143.0, 141.5, -135.0, 45.0),
        "M 143 143 L 42.944 243.056 A 141.5 141.5 0 0 1 243.056 42.944 Z"
    );
    // 862 Mbps 的实际裁剪扇形：|116.37 + 135| = 251.37 > 180 ⇒ large = 1。
    assert_eq!(
        sector_path(143.0, 143.0, 141.5, -135.0, 116.37),
        "M 143 143 L 42.944 243.056 A 141.5 141.5 0 1 1 269.776 205.85 Z"
    );
}

#[test]
fn large_arc_is_strict_greater_than_180() {
    assert_eq!(large_arc(0.0, 180.0), 0);
    assert_eq!(large_arc(-135.0, 45.0), 0);
    assert_eq!(large_arc(0.0, 180.001), 1);
    assert_eq!(large_arc(-135.0, 116.37), 1);
    assert_eq!(large_arc(135.0, -135.0), 1); // |a1-a0| = 270
}

#[test]
fn r3_matches_js_math_round_and_to_string() {
    assert_eq!(r3(234.9239323199615), "234.924");
    assert_eq!(r3(51.07606768003849), "51.076");
    assert_eq!(r3(48.85), "48.85");
    // 整数不带 ".0"。
    assert_eq!(r3(130.0), "130");
    assert_eq!(r3(143.0), "143");
    // 量化发生在第三位小数上（v*1000 后取整再 /1000），所以 2.5 原样返回。
    assert_eq!(r3(2.5), "2.5");
    assert_eq!(r3(-2.5), "-2.5");
    // Math.round 是 floor(x + 0.5)（向 +∞），不是 f64::round（远离零）：
    // -2.5005*1000 = -2500.5 -> floor(-2500.0) = -2500 -> -2.5（f64::round 会给 -2.501）
    assert_eq!(r3(-2.5005), "-2.5");
    assert_eq!(r3(2.5005), "2.501");
    assert_eq!(r3(1.2345), "1.235");
    assert_eq!(r3(-1.2345), "-1.234");
    // -0 打印成 "0"。
    assert_eq!(r3(-0.0), "0");
    assert_eq!(r3(0.0), "0");
    assert_eq!(r3(0.0004), "0");
}

#[test]
fn clamp_keeps_nan_passthrough() {
    assert_eq!(clamp(5.0, 0.0, 10.0), 5.0);
    assert_eq!(clamp(-1.0, 0.0, 10.0), 0.0);
    assert_eq!(clamp(11.0, 0.0, 10.0), 10.0);
    // 源码 `NaN < lo` 与 `NaN > hi` 均为 false ⇒ 原样穿透。
    assert!(clamp(f64::NAN, 0.0, 10.0).is_nan());
    assert_eq!(clamp(f64::INFINITY, -135.0, 135.0), 135.0);
    assert_eq!(clamp(f64::NEG_INFINITY, -135.0, 135.0), -135.0);
}

#[test]
fn min_max_nan_follow_js() {
    assert_eq!(min_nan(1.0, 2.0), 1.0);
    assert_eq!(max_nan(1.0, 2.0), 2.0);
    // JS Math.min/Math.max 任一 NaN 即 NaN（Rust 的 f64::min 会返回另一操作数）。
    assert!(min_nan(f64::NAN, 2.0).is_nan());
    assert!(max_nan(1.0, f64::NAN).is_nan());
}

#[test]
fn fixed3_matches_to_fixed_3() {
    assert_eq!(fixed3(116.37), "116.370");
    assert_eq!(fixed3(-135.0), "-135.000");
    assert_eq!(fixed3(0.0), "0.000");
    // JS 的 (-0).toFixed(3) === "0.000"
    assert_eq!(fixed3(-0.0), "0.000");
}

#[test]
fn geo_and_text_block_constants() {
    assert_eq!(GEO.radius, 130.0);
    assert_eq!(GEO.stroke_width, 23.0);
    assert_eq!(GEO.size, 286.0);
    assert_eq!(GEO.center, 143.0);
    assert_eq!(GEO.start_angle, -135.0);
    assert_eq!(GEO.end_angle, 135.0);
    assert_eq!(GEO.label_radius, 97.0);
    assert_eq!(GEO.label_font_size, 14.0);
    assert_eq!(GEO.label_font_weight, 700);
    assert_eq!(GEO.needle_base, 20.0);
    assert_eq!(GEO.needle_tip, 10.0);
    assert_eq!(GEO.needle_length, 85.0);
    assert_eq!(GEO.number_size, 36.0);
    assert_eq!(GEO.number_weight, 500);
    assert_eq!(GEO.unit_size, 14.0);
    assert_eq!(GEO.unit_gap, 4.0);
    assert_eq!(GEO.unit_icon_size, 16.0);
    assert_eq!(GEO.bottom_inset, 16.0);
    assert_eq!(GEO.number_line_height, 40.0);
    assert_eq!(GEO.unit_line_height, 20.0);

    // sweep 裁剪半径 = 130 + 23/2
    assert_eq!(SWEEP_RADIUS, 141.5);
    // TEXT_BLOCK：270 / 260 / 226
    assert_eq!(TEXT_BLOCK_BOTTOM, 270.0);
    assert_eq!(TEXT_BLOCK_UNIT_CENTER, 260.0);
    assert_eq!(TEXT_BLOCK_NUMBER_CENTER, 226.0);
}
