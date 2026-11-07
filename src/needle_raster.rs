//! 指针**整体光栅化**：tiny-skia 把整条指针（含真实渐变）画进一张 RGBA 位图，
//! 由 Slint 的 `Image` 元素**一次**合成到画面上（feature `needle-raster`）。
//!
//! # 为什么必须这么做
//!
//! 软件渲染器画 `Path.fill` 时只取 `Brush::color()`
//! （`i-slint-renderer-software-1.18.1/lib.rs:3022-3030`），渐变画刷的 `color()` 是透明
//! ⇒ 渐变填充路径根本画不出来。于是历史实现把指针沿轴切成 32 片梯形、每片一个平色。
//! 但软件渲染器是**逐路径 `over`**（`path.rs:118-130`）：相邻两片严格共边时，共享边上的
//! 像素会被两片各覆盖 `c` / `1−c`，合成两次后覆盖不足。接缝深度按量纲可写成
//!
//! * alpha 切片：`−a²·c(1−c)·255`（dark 实测 21/255）；
//! * 不透明预合成：`c(1−c)·(P − C_i)`（dark 36、light **+45.9**）。
//!
//! 两项都**与片数无关**，只与局部 alpha / 颜色差有关，所以 N 扫描、重叠补偿、不透明
//! 预合成三条路都被反证失败（见 [`crate::renderer::NEEDLE_SLICES`] 的文档）。
//!
//! 唯一能绕开的是**让每像素只被合成一次**：把整条指针光栅化成一张 RGBA 图，交给
//! `Image` 一次性 alpha 混合。本模块就是这张图的生产者。
//!
//! # 几何与渐变
//!
//! 画布 = 指针元素盒：以 `(143,143)` 为中心、边长 170 uu 的正方形，因此**局部坐标**是
//! `0..170`、圆心在 `(85,85)`（与 `.slint` 里 `viewbox 0 0 170 170` 的元素盒一致）。
//! 路径按 `angle = 0`（指针朝上）建立，实际角度由 `Transform`（先绕 `(85,85)` 旋转、
//! 再按 `s = px/170` 缩放到像素）一次性作用到**路径和渐变**上
//! （`tiny-skia-0.11.4/src/painter.rs:299-310` 对两者调用同一个 transform）。
//!
//! 这样有三条好处：
//! 1. 路径与渐变都只建**一次**（尺寸变化时才重建），每帧只多一个 transform；
//! 2. 渐变坐标不用手算旋转后的 tip/hub，杜绝把 286 世界坐标混进局部坐标；
//! 3. 抗锯齿由 `Pixmap::fill_path` + `FillRule::Winding` 提供（`Paint::anti_alias` 默认开）。

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, Path, PathBuilder, Pixmap, Point, Shader,
    SpreadMode, Transform,
};

use crate::geometry::GEO;
use crate::model::Theme;
use crate::renderer::{needle_stops_u, NEEDLE_BOX};
use crate::GaugeTheme;

/// 指针元素盒边长（uu）= 170，与 [`crate::renderer::NEEDLE_BOX`] 同值。
pub const BOX_UU: f32 = NEEDLE_BOX as f32;

/// 局部坐标下的盒中心（uu）：`(85, 85)`。
pub const CENTER_UU: f32 = BOX_UU / 2.0;

/// 单张位图的像素边长上限（防御：宿主把窗口拉得离谱时不至于申请几个 GB）。
const MAX_PX: u32 = 8192;

/// 指针光栅化器：持有一张 `Pixmap` 与一块可复用的 RGBA8 输出缓冲。
///
/// 稳态（尺寸不变）下 [`Self::update`] 只做 `clear` + 重绘 + 预乘反解，
/// **不重新申请**这两块大缓冲：`Pixmap` 与 `Vec<u8>` 只在像素边长变化时重建，
/// 次数记在 [`Self::resizes`] 里，可被测试断言"帧间不增"。
///
/// 仍存在的每帧小分配只有 tiny-skia 内部的一次 `Paint` 克隆（`painter.rs:307`：
/// 非单位 transform 下它会 clone 一份 shader，几十字节），与本结构体持有的缓冲无关。
pub struct NeedleRaster {
    /// 预乘 RGBA8 画布（`px × px`）。
    pixmap: Pixmap,
    /// 反预乘后的直通 RGBA8（`px × px × 4`），直接喂给 `slint::Image::from_rgba8`。
    out: Vec<u8>,
    /// 局部坐标（0..170）下的指针四边形，**与角度无关**（角度在 transform 里）。
    path: Path,
    /// dark 主题的渐变画刷（局部坐标，恒等 transform）。
    paint_dark: Paint<'static>,
    /// light 主题的渐变画刷。
    paint_light: Paint<'static>,
    /// 当前位图边长（像素）。
    px: u32,
    /// 重建次数（稳态不增 = 零稳态分配的证明）。
    resizes: usize,
}

impl NeedleRaster {
    /// 建一个空实例（还没有画布；第一次 [`Self::update`] 会建）。
    pub fn new() -> Self {
        Self {
            pixmap: Pixmap::new(1, 1).expect("1×1 位图总是可分配"),
            out: vec![0; 4],
            path: unit_path(),
            paint_dark: paint_for(Theme::Dark),
            paint_light: paint_for(Theme::Light),
            px: 0,
            resizes: 0,
        }
    }

    /// 已重建次数（只在像素边长变化时 +1）。
    pub fn resizes(&self) -> usize {
        self.resizes
    }

    /// 当前位图边长（像素）；`0` 表示还没画过。
    pub fn px(&self) -> u32 {
        self.px
    }

    /// 重画一帧，返回 `(RGBA8 字节, 宽, 高)`；字节本身就是本结构体持有的缓冲。
    ///
    /// - `theme`：渐变 stop 表（与切片路径共用 [`needle_stops_u`] 这唯一一份真源）；
    /// - `angle`：`display_angle`（0° 朝上、顺时针为正，NaN 按 0 处理）；
    /// - `px_size`：目标像素边长 = `170 * k * dpr`（不足 1 按 1 处理）。
    pub fn update(&mut self, theme: GaugeTheme, angle: f64, px_size: u32) -> (&[u8], u32, u32) {
        let px = px_size.clamp(1, MAX_PX);
        if self.px != px {
            self.pixmap = Pixmap::new(px, px).expect("边长已夹在 1..=8192");
            let len = (px as usize) * (px as usize) * 4;
            self.out.clear();
            self.out.resize(len, 0);
            self.px = px;
            self.resizes += 1;
        }
        self.draw(theme, angle);
        (&self.out, self.px, self.px)
    }

    /// 真正的一次光栅化：清空 → 填路径 → 反预乘回 [`Self::out`]。
    fn draw(&mut self, theme: GaugeTheme, angle: f64) {
        self.pixmap.fill(Color::TRANSPARENT);
        let s = self.px as f32 / BOX_UU;
        let transform = rotate_scale(angle, s);
        let paint = match theme {
            GaugeTheme::Dark => &self.paint_dark,
            GaugeTheme::Light => &self.paint_light,
        };
        self.pixmap
            .fill_path(&self.path, paint, FillRule::Winding, transform, None);
        unpremultiply(self.pixmap.data(), &mut self.out);
    }
}

impl Default for NeedleRaster {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for NeedleRaster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeedleRaster")
            .field("px", &self.px)
            .field("resizes", &self.resizes)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// 局部几何 / 变换
// ---------------------------------------------------------------------------

/// 指针四边形（局部坐标 `0..170`，`angle = 0` 朝上）。
///
/// 与 [`crate::renderer::needle_commands`] 的 `M 133 143 L 153 143 L 148 58 L 138 58 Z`
/// 是同一组点：减去盒原点 `58` 后即 `(75,85) (95,85) (90,0) (80,0)`。
fn unit_path() -> Path {
    let half_base = (GEO.needle_base / 2.0) as f32;
    let half_tip = (GEO.needle_tip / 2.0) as f32;
    let len = GEO.needle_length as f32;
    let cx = CENTER_UU;
    let cy = CENTER_UU;

    let mut pb = PathBuilder::new();
    pb.move_to(cx - half_base, cy); // 根部左
    pb.line_to(cx + half_base, cy); // 根部右
    pb.line_to(cx + half_tip, cy - len); // 尖端右
    pb.line_to(cx - half_tip, cy - len); // 尖端左
    pb.close();
    pb.finish().expect("4 点闭合多边形总是有效的")
}

/// 局部坐标 → 像素：先绕 `(85,85)` 顺时针旋转 `angle`（0° 朝上），再整体乘 `s`。
///
/// 显式按 `Transform::from_row(sx, ky, kx, sy, tx, ty)` 的分量写出（映射为
/// `x' = sx·x + kx·y + tx`、`y' = ky·x + sy·y + ty`），避免依赖 `pre/post_concat`
/// 的结合顺序。
fn rotate_scale(angle_deg: f64, s: f32) -> Transform {
    let a = if angle_deg.is_finite() {
        angle_deg
    } else {
        0.0
    };
    let (sin, cos) = a.to_radians().sin_cos();
    let (sin, cos) = (sin as f32, cos as f32);
    let c = CENTER_UU;
    Transform::from_row(
        s * cos,
        s * sin,
        -s * sin,
        s * cos,
        s * (c - c * cos + c * sin),
        s * (c - c * sin - c * cos),
    )
}

// ---------------------------------------------------------------------------
// 渐变
// ---------------------------------------------------------------------------

/// 按主题建一个**局部坐标**的线性渐变画刷。
///
/// * 起点 = 尖端 `(85, 0)`、终点 = 根部 `(85, 85)` ⇒ `offset 0` 落在尖端，
///   与 [`needle_stops_u`] 的 `u = 0` 语义（`u = 1 − d/85`）一致；
/// * stop 表直接取自 [`needle_stops_u`]（切片路径用的同一份），不另抄一遍颜色；
/// * `SpreadMode::Pad`：指针永远落在 `[tip, hub]` 这段轴上，Pad 不会被触发；
/// * 自己的 transform 用恒等 —— 旋转/缩放交给 `Pixmap::fill_path` 的 transform，
///   tiny-skia 会把它同时作用到路径和 shader 上。
fn paint_for(theme: Theme) -> Paint<'static> {
    let stops: Vec<GradientStop> = needle_stops_u(theme)
        .iter()
        .map(|st| {
            GradientStop::new(
                st.u as f32,
                Color::from_rgba8(st.rgb[0], st.rgb[1], st.rgb[2], st.alpha),
            )
        })
        .collect();
    let shader = LinearGradient::new(
        Point::from_xy(CENTER_UU, 0.0),
        Point::from_xy(CENTER_UU, CENTER_UU),
        stops,
        SpreadMode::Pad,
        Transform::identity(),
    )
    .expect("stop 表非空且首尾不同点，渐变必然可建");
    debug_assert!(
        matches!(shader, Shader::LinearGradient(_)),
        "指针渐变必须真的是渐变（stop 少于 2 个会退化成纯色）"
    );
    Paint {
        shader,
        // 抗锯齿必须开：`Pixmap::fill_path` 默认就带 AA，这一行只是把它写死。
        anti_alias: true,
        ..Paint::default()
    }
}

// ---------------------------------------------------------------------------
// 预乘 → 直通
// ---------------------------------------------------------------------------

/// `tiny-skia` 的 `Pixmap` 是预乘 RGBA8；`slint::Image::from_rgba8` 要直通 alpha。
///
/// 两个主题下这一往返都是**无损**的：
/// * dark：`rgb` 恒为白，预乘值 = `(a,a,a,a)`，反解回 `(255,255,255,a)`；
/// * light：三个 stop 的 alpha 全 255，预乘是恒等变换。
fn unpremultiply(src: &[u8], dst: &mut [u8]) {
    let n = src.len().min(dst.len()) / 4;
    for i in 0..n {
        let o = i * 4;
        let a = u32::from(src[o + 3]);
        let d = &mut dst[o..o + 4];
        if a == 0 {
            d.fill(0);
        } else if a == 255 {
            d.copy_from_slice(&src[o..o + 4]);
        } else {
            for c in 0..3 {
                d[c] = ((u32::from(src[o + c]) * 255 + a / 2) / a).min(255) as u8;
            }
            d[3] = src[o + 3];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_path_covers_the_needle_box_at_zero_angle() {
        let b = unit_path().bounds();
        // 根部 75..95、纵向 0..85
        assert!((b.left() - 75.0).abs() < 1e-6);
        assert!((b.right() - 95.0).abs() < 1e-6);
        assert!((b.top() - 0.0).abs() < 1e-6);
        assert!((b.bottom() - 85.0).abs() < 1e-6);
    }

    #[test]
    fn rotate_scale_maps_the_tip_onto_the_expected_edge() {
        let mapped = |t: Transform, x: f32, y: f32| {
            let mut p = Point::from_xy(x, y);
            t.map_point(&mut p);
            p
        };
        // 0°：恒等（s = 1）
        let p = mapped(rotate_scale(0.0, 1.0), CENTER_UU, 0.0);
        assert!((p.x - 85.0).abs() < 1e-5 && p.y.abs() < 1e-5);
        // 90°：尖端指向右边界 (170, 85)
        let p = mapped(rotate_scale(90.0, 1.0), CENTER_UU, 0.0);
        assert!((p.x - 170.0).abs() < 1e-4 && (p.y - 85.0).abs() < 1e-4);
        // 旋转不移动盒中心
        let p = mapped(rotate_scale(116.37, 2.0), CENTER_UU, CENTER_UU);
        assert!((p.x - 170.0).abs() < 1e-3 && (p.y - 170.0).abs() < 1e-3);
    }

    #[test]
    fn steady_state_does_not_reallocate() {
        let mut r = NeedleRaster::new();
        let _ = r.update(GaugeTheme::Dark, 0.0, 680);
        let base = r.resizes();
        assert_eq!(base, 1);
        for i in 0..8 {
            let _ = r.update(GaugeTheme::Dark, i as f64 * 17.0, 680);
        }
        assert_eq!(r.resizes(), base, "同尺寸的帧不许重建画布");
        // 尺寸变化才重建
        let _ = r.update(GaugeTheme::Dark, 0.0, 340);
        assert_eq!(r.resizes(), base + 1);
    }

    #[test]
    fn tip_is_opaque_white_and_box_corner_is_transparent() {
        let mut r = NeedleRaster::new();
        let (bytes, w, h) = r.update(GaugeTheme::Light, 0.0, 680);
        assert_eq!((w, h), (680, 680));
        let px = |x: u32, y: u32| -> [u8; 4] {
            let i = ((y * w + x) * 4) as usize;
            [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
        };
        // 角（离指针很远）全透明
        assert_eq!(px(2, 678), [0, 0, 0, 0]);
        // 尖端内侧：白、不透明
        let p = px(340, 20);
        assert_eq!(p[3], 255);
        assert!(p[0] > 240, "尖端应为白：{p:?}");
    }
}
