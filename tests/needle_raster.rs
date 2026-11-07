//! `NeedleRaster`（feature `needle-raster`）的数值验收。
//!
//! 这里量的是**光栅图本身**，不经过 Slint：把 `update()` 吐出的 RGBA 按"直通 alpha
//! over 黑色页底"合成后取 luma，再和解析目标（[`renderer::needle_stops_u`] 的线性 stop
//! 插值）逐点比对。它就是 `tools/needle_profile_dense.py --theme dark|light` 的
//! 同口径测量，只是搬进了 `cargo test`，免得每次都要跑一遍截图流水线。
//!
//! 两个硬阈值的来历：
//! - dark 切片基线 `max|Δ|` = 21/255（接缝 `−a²c(1−c)·255`），光栅化后应 ≤ 3；
//! - light 切片基线 = 46/255（接缝 `c(1−c)(P−C_i)`），光栅化后应 ≤ 6。

#![cfg(feature = "needle-raster")]

use speed_gauge_slint::needle_raster::{NeedleRaster, BOX_UU, CENTER_UU};
use speed_gauge_slint::GaugeTheme;

/// 指针角（`high` 状态的 sweep 终点）。
const ANGLE: f64 = 116.37;
/// 参考口径的像素边长：286 uu × DPR 4 = 1144 px 的画布里，指针盒是 170 × 4 = 680。
const PX: u32 = 680;

/// 直通 RGBA → over 黑底后的 luma。
fn over_black(px: [u8; 4]) -> f64 {
    let a = f64::from(px[3]) / 255.0;
    let luma = 0.2126 * f64::from(px[0]) + 0.7152 * f64::from(px[1]) + 0.0722 * f64::from(px[2]);
    luma * a
}

/// 取一个像素（越界 panic：坐标都是算出来的，越界说明几何错了）。
fn px_at(bytes: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    assert!(x < w, "x={x} 越界（w={w}）");
    let i = ((y * w + x) * 4) as usize;
    [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
}

/// 沿指针轴、距圆心 `d` uu 处的采样（最近像素，与 `needle_profile_dense.py` 同一取整）。
fn axis_luma(bytes: &[u8], w: u32, angle: f64, d: f64) -> f64 {
    let s = f64::from(w) / f64::from(BOX_UU);
    let rad = (angle - 90.0).to_radians();
    let x = (f64::from(CENTER_UU) + d * rad.cos()) * s;
    let y = (f64::from(CENTER_UU) + d * rad.sin()) * s;
    let xi = (x.round().max(0.0) as u32).min(w - 1);
    let yi = (y.round().max(0.0) as u32).min(w - 1);
    over_black(px_at(bytes, w, xi, yi))
}

/// dark 的解析目标：白针 over 黑底 ⇒ luma = 255·0.6·d/85。
fn target_dark(d: f64) -> f64 {
    255.0 * 0.6 * d / 85.0
}

/// light 的解析目标：三个不透明 stop 沿 `u = 1 − d/85` 分段线性（灰度）。
fn target_light(d: f64) -> f64 {
    const STOPS: [(f64, f64); 3] = [(0.0, 255.0), (0.58, 164.0), (1.0, 34.0)];
    let u = (1.0 - d / 85.0).clamp(0.0, 1.0);
    for pair in STOPS.windows(2) {
        let (u0, c0) = pair[0];
        let (u1, c1) = pair[1];
        if u >= u0 && u <= u1 {
            let k = if u1 == u0 { 0.0 } else { (u - u0) / (u1 - u0) };
            return c0 + (c1 - c0) * k;
        }
    }
    STOPS[2].1
}

/// 0.5 uu 步长扫过 `d ∈ [2, 84]`，返回 `(max|Δ|, mean|Δ|, 最坏 d)`。
fn dense_scan(theme: GaugeTheme, target: fn(f64) -> f64) -> (f64, f64, f64) {
    let mut raster = NeedleRaster::new();
    let (bytes, w, h) = raster.update(theme, ANGLE, PX);
    assert_eq!(w, PX);
    assert_eq!(h, PX);
    let steps = 164; // (84 - 2) / 0.5
    let mut max = 0.0f64;
    let mut sum = 0.0f64;
    let mut worst_d = 0.0f64;
    for i in 0..=steps {
        let d = 2.0 + f64::from(i) * 0.5;
        let dev = (axis_luma(bytes, w, ANGLE, d) - target(d)).abs();
        sum += dev;
        if dev > max {
            max = dev;
            worst_d = d;
        }
    }
    (max, sum / f64::from(steps + 1), worst_d)
}

#[test]
fn dark_dense_axis_profile_is_flat_to_3() {
    let (max, mean, worst_d) = dense_scan(GaugeTheme::Dark, target_dark);
    println!("dark   max|Δ| {max:.2}/255  mean|Δ| {mean:.2}/255  最坏 d={worst_d:.1}uu");
    assert!(
        max <= 3.0,
        "dark 指针轴均匀剖面 max|Δ| = {max:.2}/255（阈值 3，切片基线 21）"
    );
    assert!(mean <= 1.5, "dark mean|Δ| = {mean:.2}/255（阈值 1.5）");
}

#[test]
fn light_dense_axis_profile_is_flat_to_6() {
    let (max, mean, worst_d) = dense_scan(GaugeTheme::Light, target_light);
    println!("light  max|Δ| {max:.2}/255  mean|Δ| {mean:.2}/255  最坏 d={worst_d:.1}uu");
    assert!(
        max <= 6.0,
        "light 指针轴均匀剖面 max|Δ| = {max:.2}/255（阈值 6，切片基线 46）"
    );
    assert!(mean <= 3.0, "light mean|Δ| = {mean:.2}/255（阈值 3）");
}

#[test]
fn no_seam_dip_along_the_axis() {
    // 接缝的特征是"剖面非单调"：共享边被合成两次 ⇒ 该像素比前一个更暗。
    // 真实斜率是 255·0.6/85 = 1.8/uu ⇒ 0.5uu 一步涨 0.9，量化抖动不超过 1。
    let mut raster = NeedleRaster::new();
    let (bytes, w, _) = raster.update(GaugeTheme::Dark, ANGLE, PX);
    let mut prev = f64::NEG_INFINITY;
    let mut worst_drop = 0.0f64;
    for i in 0..=164 {
        let d = 2.0 + f64::from(i) * 0.5;
        let got = axis_luma(bytes, w, ANGLE, d);
        worst_drop = worst_drop.max(prev - got);
        prev = got;
    }
    println!("dark 轴向最大回退 {worst_drop:.2}/255（切片基线有 3~21 的深坑）");
    assert!(
        worst_drop <= 1.5,
        "轴向出现回退 {worst_drop:.2}/255 ⇒ 残留接缝"
    );
}

#[test]
fn steady_state_keeps_the_canvas() {
    let mut raster = NeedleRaster::new();
    let _ = raster.update(GaugeTheme::Dark, 0.0, PX);
    assert_eq!(raster.resizes(), 1, "首次必须建一次画布");
    assert_eq!(raster.px(), PX);
    for i in 0..16 {
        let _ = raster.update(GaugeTheme::Dark, f64::from(i) * 8.0, PX);
        let _ = raster.update(GaugeTheme::Light, f64::from(i) * -8.0, PX);
    }
    assert_eq!(raster.resizes(), 1, "同尺寸的 33 帧不许再重建画布");
    let _ = raster.update(GaugeTheme::Dark, 0.0, 340);
    assert_eq!(raster.resizes(), 2, "尺寸变了才重建");
    assert_eq!(raster.px(), 340);
}

#[test]
fn one_pixel_edge_cases_are_safe() {
    let mut raster = NeedleRaster::new();
    // 0 会被夹到 1；NaN 角度按 0 处理；不 panic 就算过。
    let (bytes, w, h) = raster.update(GaugeTheme::Dark, f64::NAN, 0);
    assert_eq!((w, h), (1, 1));
    assert_eq!(bytes.len(), 4);
    let (bytes, w, _) = raster.update(GaugeTheme::Light, f64::INFINITY, 1);
    assert_eq!(w, 1);
    assert_eq!(bytes.len(), 4);
}

#[test]
fn tip_is_bright_and_root_fades_out() {
    let mut raster = NeedleRaster::new();
    let (bytes, w, _) = raster.update(GaugeTheme::Dark, 0.0, PX);
    // 0° 朝上：轴就是盒中心竖线
    let root = axis_luma(bytes, w, 0.0, 2.0);
    let mid = axis_luma(bytes, w, 0.0, 42.5);
    let tip = axis_luma(bytes, w, 0.0, 84.0);
    assert!(
        root < mid && mid < tip,
        "应沿轴单调变亮：{root} {mid} {tip}"
    );
    assert!((tip - target_dark(84.0)).abs() < 3.0, "tip={tip}");
    // 盒角在指针之外 ⇒ 全透明
    assert_eq!(px_at(bytes, w, 4, PX - 4)[3], 0);
}

#[test]
fn rotation_is_consistent_with_the_baked_path() {
    // 90° 时尖端应指向盒右边界中点：局部 (170, 85) → 像素 (680, 340) 附近
    let mut raster = NeedleRaster::new();
    let (bytes, w, _) = raster.update(GaugeTheme::Light, 90.0, PX);
    let p = px_at(bytes, w, PX - 12, PX / 2);
    assert_eq!(p[3], 255, "90° 尖端附近应不透明：{p:?}");
    assert!(p[0] > 240, "light 尖端应为白：{p:?}");
    // 同一帧的 0° 位置（盒正上方）应已经空掉
    let q = px_at(bytes, w, PX / 2, 6);
    assert_eq!(q[3], 0, "90° 时正上方不该有指针：{q:?}");
}

#[test]
fn light_theme_is_opaque_inside_the_needle() {
    let mut raster = NeedleRaster::new();
    let (bytes, w, _) = raster.update(GaugeTheme::Light, ANGLE, PX);
    let mut checked = 0;
    for i in 1..=160 {
        let d = 2.0 + f64::from(i) * 0.5;
        let s = f64::from(w) / f64::from(BOX_UU);
        let rad = (ANGLE - 90.0).to_radians();
        let x = (f64::from(CENTER_UU) + d * rad.cos()) * s;
        let y = (f64::from(CENTER_UU) + d * rad.sin()) * s;
        let p = px_at(bytes, w, x.round() as u32, y.round() as u32);
        assert_eq!(p[3], 255, "light 指针体内部必须全不透明（d={d}）");
        checked += 1;
    }
    assert_eq!(checked, 160);
}
