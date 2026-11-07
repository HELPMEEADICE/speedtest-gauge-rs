//! `scale` 模块的等价性测试。
//!
//! 期望值来自 `docs/port-spec.md` 附录 A，以及用 node v26 对原版 `Intl.NumberFormat` /
//! `toFixed` 实测的结果（见 `format_speed` / `kbps_to_mbps` 的文档表格）。

use speed_gauge_slint::scale::{
    angle_for_value, format_speed, kbps_to_mbps, label_angle_at, value_for_angle, LABELS_EXTENDED,
    LABELS_STANDARD, NO_VALUE,
};

const EPS: f64 = 1e-9;

fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < EPS
}

#[test]
fn label_angle_at_matches_golden() {
    assert!(close(label_angle_at(0, 9), -135.0));
    assert!(close(label_angle_at(4, 9), 0.0));
    assert!(close(label_angle_at(7, 9), 101.25));
    assert!(close(label_angle_at(8, 9), 135.0));
    // 10 档 ⇒ step 30
    assert!(close(label_angle_at(0, 10), -135.0));
    assert!(close(label_angle_at(3, 10), -45.0));
    assert!(close(label_angle_at(9, 10), 135.0));
    // count < 2 退回起始角
    assert!(close(label_angle_at(0, 1), -135.0));
    assert!(close(label_angle_at(5, 0), -135.0));
}

#[test]
fn angle_for_value_standard_goldens() {
    let cases = [
        (0.0, -135.0),
        (5.0, -101.25),
        (10.0, -67.5),
        (50.0, -33.75),
        (100.0, 0.0),
        (250.0, 33.75),
        (500.0, 67.5),
        (750.0, 101.25),
        (1000.0, 135.0),
        // 档内插值
        (8.0, -81.0),
        (155.61, 12.51225),
        (341.0, 46.035),
        (862.0, 116.37),
    ];
    for (mbps, expected) in cases {
        let angle = angle_for_value(&LABELS_STANDARD, mbps);
        assert!(
            close(angle, expected),
            "{mbps} Mbps -> {angle}, 期望 {expected}"
        );
    }
}

#[test]
fn angle_for_value_clamps_beyond_last_label() {
    // idx = -1 ⇒ 夹到末档角，不越界。
    assert!(close(angle_for_value(&LABELS_STANDARD, 4200.0), 135.0));
    assert!(close(angle_for_value(&LABELS_STANDARD, 1000.0001), 135.0));
    // 非有限 / 非正 ⇒ 第 0 档角。
    assert!(close(angle_for_value(&LABELS_STANDARD, f64::NAN), -135.0));
    assert!(close(
        angle_for_value(&LABELS_STANDARD, f64::INFINITY),
        -135.0
    ));
    assert!(close(angle_for_value(&LABELS_STANDARD, -3.0), -135.0));
    assert!(close(angle_for_value(&[], 500.0), -135.0));
}

#[test]
fn angle_for_value_auto_scale_4200() {
    // auto 档：EXTENDED 10 档 ⇒ (4200-2500)/2500 = 0.68，75 + 0.68*30 = 95.4
    let angle = angle_for_value(&LABELS_EXTENDED, 4200.0);
    assert!(close(angle, 95.4), "auto 4200 -> {angle}");
    // 其余档位交叉验证
    assert!(close(angle_for_value(&LABELS_EXTENDED, 2500.0), 75.0));
    assert!(close(angle_for_value(&LABELS_EXTENDED, 3000.0), 81.0));
    assert!(close(angle_for_value(&LABELS_EXTENDED, 7500.0), 120.0));
    assert!(close(angle_for_value(&LABELS_EXTENDED, 10000.0), 135.0));
    assert!(close(angle_for_value(&LABELS_EXTENDED, 20000.0), 135.0));
}

#[test]
fn value_for_angle_goldens() {
    assert!(close(value_for_angle(&LABELS_STANDARD, 116.37), 862.0));
    // clamp 到 ±135
    assert!(close(value_for_angle(&LABELS_STANDARD, -200.0), 0.0));
    assert!(close(value_for_angle(&LABELS_STANDARD, 200.0), 1000.0));
    // 刻度角命中
    assert!(close(value_for_angle(&LABELS_STANDARD, -135.0), 0.0));
    assert!(close(value_for_angle(&LABELS_STANDARD, 0.0), 100.0));
    assert!(close(value_for_angle(&LABELS_STANDARD, 135.0), 1000.0));
    assert!(close(value_for_angle(&LABELS_STANDARD, 101.25), 750.0));
    // 少于两个刻度 ⇒ 0；NaN 直通
    assert_eq!(value_for_angle(&[], 0.0), 0.0);
    assert_eq!(value_for_angle(&[1.0], 0.0), 0.0);
    assert!(value_for_angle(&LABELS_STANDARD, f64::NAN).is_nan());
}

#[test]
fn inverse_function_roundtrip_inside_bands() {
    for labels in [&LABELS_STANDARD[..], &LABELS_EXTENDED[..]] {
        for band in 0..labels.len() - 1 {
            let lo = labels[band];
            let hi = labels[band + 1];
            let band_hi = band + 1;
            for t in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
                let value = lo + t * (hi - lo);
                let angle = angle_for_value(labels, value);
                let back = value_for_angle(labels, angle);
                let tolerance = 1e-9 * value.abs().max(1.0);
                assert!(
                    (back - value).abs() <= tolerance,
                    "往返失败: {value} -> {angle}° -> {back} (band {band}..{band_hi}, t={t})"
                );
            }
        }
    }
}

#[test]
fn format_speed_matches_intl_en_us() {
    // 0 / 非有限 / 负数
    assert_eq!(format_speed(0.0), "0.00");
    assert_eq!(format_speed(-0.0), "0.00");
    assert_eq!(format_speed(-1.0), NO_VALUE);
    assert_eq!(format_speed(f64::NAN), NO_VALUE);
    assert_eq!(format_speed(f64::INFINITY), NO_VALUE);
    assert_eq!(format_speed(f64::NEG_INFINITY), NO_VALUE);
    assert_eq!(NO_VALUE, "\u{2014}");

    // 恒定两位小数 + en-US 千分位
    assert_eq!(format_speed(300.0), "0.30");
    assert_eq!(format_speed(862_000.0), "862.00");
    assert_eq!(format_speed(4_200_000.0), "4,200.00");
    assert_eq!(format_speed(12_345.678), "12.35");
    assert_eq!(format_speed(12_345.0), "12.35");
    assert_eq!(format_speed(12_344.0), "12.34");
    assert_eq!(format_speed(1_000_001.0), "1,000.00");
    assert_eq!(format_speed(1_234_567_891.0), "1,234,567.89");
    assert_eq!(format_speed(99_999.0), "100.00");
}

/// `Intl`（最短往返表示 + halfExpand）与 `toFixed`（精确展开 + 平局取较大 n）口径不同，
/// 这几个值恰好把差异暴露出来 —— 必须两边都对。
#[test]
fn format_speed_vs_kbps_to_mbps_divergence() {
    // 2.675 的最短表示是 "2.675"（进位），精确展开是 2.674999999…（不进位）。
    assert_eq!(format_speed(2_675.0), "2.68");
    assert_eq!(kbps_to_mbps(2_675.0), 2.67);
    // 1.005
    assert_eq!(format_speed(1_005.0), "1.01");
    assert_eq!(kbps_to_mbps(1_005.0), 1.0);
    // 0.015
    assert_eq!(format_speed(15.0), "0.02");
    assert_eq!(kbps_to_mbps(15.0), 0.01);
    // 999.995：两条路径都进位到 1000，但 Intl 带千分位
    assert_eq!(format_speed(999_995.0), "1,000.00");
    assert_eq!(kbps_to_mbps(999_995.0), 1000.0);
    // 0.125 是精确平局：Intl 的 halfExpand 与 toFixed 的"取较大 n"对正数一致
    assert_eq!(format_speed(125.0), "0.13");
    assert_eq!(kbps_to_mbps(125.0), 0.13);
}

#[test]
fn kbps_to_mbps_goldens() {
    assert_eq!(kbps_to_mbps(0.0), 0.0);
    assert_eq!(kbps_to_mbps(862_000.0), 862.0);
    assert_eq!(kbps_to_mbps(12_345.0), 12.35);
    assert_eq!(kbps_to_mbps(12_344.0), 12.34);
    assert_eq!(kbps_to_mbps(4_200_000.0), 4200.0);
    // toFixed(2) 的量化：1000.001 -> "1000.00" -> 1000.0
    assert_eq!(kbps_to_mbps(1_000_001.0), 1000.0);
    assert!(kbps_to_mbps(f64::NAN).is_nan());
    assert_eq!(kbps_to_mbps(f64::INFINITY), f64::INFINITY);
}

#[test]
fn label_tables_are_verbatim() {
    assert_eq!(
        LABELS_STANDARD,
        [0.0, 5.0, 10.0, 50.0, 100.0, 250.0, 500.0, 750.0, 1000.0]
    );
    assert_eq!(
        LABELS_EXTENDED,
        [0.0, 10.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0]
    );
    assert_eq!(LABELS_STANDARD.len(), 9);
    assert_eq!(LABELS_EXTENDED.len(), 10);
}
