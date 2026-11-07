//! uu 几何 → Slint `Path.commands` 字符串 / 环段表 / 指针烘焙 / 布局量。
//!
//! 本模块只做**纯数值**：输入是 uu（viewBox `0 0 286 286` 的用户单位）与角度，
//! 输出是可直接喂给 `.slint` 的字符串、颜色与矩形。不依赖 Slint 运行时（不构造
//! `ModelRc`），因此可单独单元测试。
//!
//! 三条硬约束（来自 `PROBE.md` / `docs/port-spec.md`）：
//! 1. 软件渲染器**静默忽略** `transform-rotation` / `transform-scale`（`lib.rs:3096/3100`），
//!    所以指针几何必须在 Rust 里按角度烘焙成 `commands`。
//! 2. `Path` 的 `viewbox-*` 与元素尺寸一一对应时不缩放几何；描边宽度不随 viewBox 缩放，
//!    因此描边长度由 `.slint` 侧写成 `23px * root.k`。
//! 3. 彩弧不用 `clip-path`（Slint 无路径裁剪）：用"前 k 段全画 + 第 k+1 段截到当前角"。

use std::sync::LazyLock;

use crate::geometry::{arc_path, min_nan, polar, r3, GEO};
use crate::gradient::{self, DEFAULT_SEGMENTS, OVERLAP_DEG};
use crate::model::{GradientVariant, Theme};
use crate::scale::label_angle_at;

/// 画布边长（uu）。
pub const SIZE: f64 = GEO.size;
/// 画布中心（= 圆心）。
pub const CENTER: f64 = GEO.center;
/// 起始角（0° 朝上、顺时针为正）。
pub const START_ANGLE: f64 = GEO.start_angle;
/// 结束角。
pub const END_ANGLE: f64 = GEO.end_angle;
/// 指针元素盒的半边长（uu）：指针长 85 = 85，所以盒 = 170×170 且圆心正好在盒中心。
pub const NEEDLE_HALF_BOX: f64 = GEO.needle_length;
/// 指针元素盒的边长（uu）= 170。
pub const NEEDLE_BOX: f64 = NEEDLE_HALF_BOX * 2.0;
/// 指针元素盒左上角在 uu 坐标系里的位置（= 143 - 85 = 58）。
pub const NEEDLE_BOX_ORIGIN: f64 = CENTER - NEEDLE_HALF_BOX;

/// `sweep < 0.05` 时整组彩弧隐藏（`gauge.ts:447-450`）。
pub const SWEEP_MIN_DEG: f64 = 0.05;

// ---------------------------------------------------------------------------
// 轨道弧
// ---------------------------------------------------------------------------

/// 轨道弧 `d`：`arcPath(143,143,130,-135,135)`。
///
/// 黄金串（port-spec 附录 A）：`M 51.076 234.924 A 130 130 0 1 1 234.924 234.924`。
pub fn track_arc() -> &'static str {
    static S: LazyLock<String> =
        LazyLock::new(|| arc_path(CENTER, CENTER, GEO.radius, START_ANGLE, END_ANGLE));
    S.as_str()
}

// ---------------------------------------------------------------------------
// 240 段彩弧
// ---------------------------------------------------------------------------

/// 一个变体的整张段表（240 段）的**预生成**产物。
///
/// `commands[i]` 是该段的 `arcPath`（含 0.28° overlap，末段被 `min(135, …)` 截断），
/// `color[i]` 是段中心采样色；`a0`/`a1` 保留角度，供每帧计算"画到第几段"用。
pub struct RingTable {
    /// 每段的 `M … A …` 字符串。
    pub commands: Vec<String>,
    /// 每段的 8-bit sRGB 采样色。
    pub colors: Vec<[u8; 3]>,
    /// 每段起始角（度）。
    pub a0: Vec<f64>,
    /// 每段结束角（度，含 overlap / 末段截断）。
    pub a1: Vec<f64>,
}

impl RingTable {
    /// 段数。
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// 是否为空表。
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    fn build(variant: GradientVariant) -> Self {
        let segs = gradient::ring_segments(variant, DEFAULT_SEGMENTS, GEO.radius, CENTER, CENTER);
        let n = DEFAULT_SEGMENTS as f64;
        let span = END_ANGLE - START_ANGLE;
        let mut a0 = Vec::with_capacity(DEFAULT_SEGMENTS);
        let mut a1 = Vec::with_capacity(DEFAULT_SEGMENTS);
        let mut commands = Vec::with_capacity(DEFAULT_SEGMENTS);
        let mut colors = Vec::with_capacity(DEFAULT_SEGMENTS);
        for i in 0..DEFAULT_SEGMENTS {
            let index = i as f64;
            a0.push(START_ANGLE + span * index / n);
            // 与 `gradient::ring_segments` 同一公式：+overlap 后再 `min(end)` 截断。
            a1.push(min_nan(
                END_ANGLE,
                START_ANGLE + span * (index + 1.0) / n + OVERLAP_DEG,
            ));
        }
        for seg in segs {
            commands.push(seg.d);
            colors.push(seg.color);
        }
        Self {
            commands,
            colors,
            a0,
            a1,
        }
    }
}

static TABLES: LazyLock<[RingTable; 4]> = LazyLock::new(|| {
    [
        RingTable::build(GradientVariant::DlMulti),
        RingTable::build(GradientVariant::UlMulti),
        RingTable::build(GradientVariant::DlSingle),
        RingTable::build(GradientVariant::UlSingle),
    ]
});

/// 取某个变体的段表（进程内只构建一次）。
pub fn table(variant: GradientVariant) -> &'static RingTable {
    let idx = match variant {
        GradientVariant::DlMulti => 0,
        GradientVariant::UlMulti => 1,
        GradientVariant::DlSingle => 2,
        GradientVariant::UlSingle => 3,
    };
    &TABLES[idx]
}

/// 一帧的彩弧视图状态。
#[derive(Debug, Clone, PartialEq)]
pub struct RingView {
    /// 需要**完整**画出的段数（`visible: i < ring-visible-count`）。
    pub visible: usize,
    /// 边界段的 `commands`（截到当前角）；空串表示不画边界段。
    pub boundary: String,
    /// 边界段颜色（= 被截断那一段的采样色）。
    pub boundary_color: [u8; 3],
}

impl RingView {
    /// 整组彩弧是否应该隐藏（`sweep < 0.05` 或没有任何段）。
    pub fn is_empty(&self) -> bool {
        self.visible == 0 && self.boundary.is_empty()
    }
}

/// 用**同一个** `display_angle` 算出彩弧的可见段数与边界段（port-spec §4 规则 1–7）。
///
/// 等价于原实现的"240 段全画 + 扇形 `clipPath(141.5, -135, arcAngle)`"：
/// - sector 半径 141.5 == 环外径，所以裁剪边界是一条径向直线；
/// - 于是"完整覆盖的段" = `a1 <= arc` 的段，跨过边界的那一段截到 `arc` 即可。
///
/// `a0_i + overlap` 让第 `i-1` 段的终点越过 `a0_i`，因此边界段从 `a0_visible`（而不是
/// `a0_visible - overlap`）开始，画到 `arc`：既不会留缝，也不会越过 `arc`。
pub fn ring_view(variant: GradientVariant, display_angle: f64) -> RingView {
    let arc = if display_angle.is_nan() {
        START_ANGLE
    } else {
        display_angle.clamp(START_ANGLE, END_ANGLE)
    };
    let table = table(variant);

    // `paint()` 里 `sweep = arcAngle - (-135) < 0.05` ⇒ 清空 d 且 ringGroup display:none。
    if arc - START_ANGLE < SWEEP_MIN_DEG {
        return RingView {
            visible: 0,
            boundary: String::new(),
            boundary_color: [0, 0, 0],
        };
    }

    // `a1` 单调不减 ⇒ 二分即可。
    let visible = table.a1.partition_point(|a| *a <= arc);
    if visible >= table.len() {
        return RingView {
            visible: table.len(),
            boundary: String::new(),
            boundary_color: [0, 0, 0],
        };
    }

    // 防御：理论上 `a0[visible] = a1[visible-1] - overlap <= arc - overlap < arc`。
    let a0 = table.a0[visible].min(arc);
    RingView {
        visible,
        boundary: arc_path(CENTER, CENTER, GEO.radius, a0, arc),
        boundary_color: table.colors[visible],
    }
}

// ---------------------------------------------------------------------------
// 刻度文字
// ---------------------------------------------------------------------------

/// 一条刻度文字的落点（uu）。
#[derive(Debug, Clone, PartialEq)]
pub struct LabelGeo {
    /// 显示文本（`String(value)`：整数、无小数、无千分位）。
    pub text: String,
    /// 锚点 x（uu）。
    pub x: f64,
    /// 锚点 y（uu）。
    pub y: f64,
}

/// 按 `labelAngleAt` 生成全部刻度落点：`polar(143,143,97,angle)`。
pub fn labels_geo(labels: &[f64]) -> Vec<LabelGeo> {
    let n = labels.len();
    labels
        .iter()
        .enumerate()
        .map(|(i, value)| {
            let p = polar(CENTER, CENTER, GEO.label_radius, label_angle_at(i, n));
            LabelGeo {
                text: js_number_to_string(*value),
                x: p.x,
                y: p.y,
            }
        })
        .collect()
}

/// JS `String(number)`：整数不带 `.0`。
fn js_number_to_string(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e21 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

/// 该角度下被点亮的刻度数量（前缀计数，供 `.slint` 用 `i < lit-count` 判定）。
///
/// 逐字符复刻 `gauge.ts:418-425`：`value == 0` 用**严格大于**，其余用 `>=`。
/// 刻度角单调递增 ⇒ 点亮集合恒为前缀，所以只数前缀长度即可。
pub fn lit_count(labels: &[f64], arc_angle: f64) -> usize {
    let n = labels.len();
    let mut count = 0;
    for (i, value) in labels.iter().enumerate() {
        let a = label_angle_at(i, n);
        let on = if *value == 0.0 {
            arc_angle > a
        } else {
            arc_angle >= a
        };
        if on {
            count = i + 1;
        }
    }
    count
}

// ---------------------------------------------------------------------------
// 指针烘焙
// ---------------------------------------------------------------------------

/// 把指针四顶点绕 `(143,143)` 旋转 `angle_deg`（0° 朝上、顺时针为正），
/// 输出**元素盒局部坐标**（`0..170`，盒子左上角 = uu `(58,58)`）的 path 字符串。
///
/// 指针局部几何（uu，与源码 `gauge.ts:186-194` 逐字一致）：
/// `M 133 143 L 153 143 L 148 58 L 138 58 Z`。
pub fn needle_commands(angle_deg: f64) -> String {
    let angle = if angle_deg.is_finite() {
        angle_deg
    } else {
        0.0
    };
    let (sin, cos) = angle.to_radians().sin_cos();
    let c = CENTER;
    let half_base = GEO.needle_base / 2.0;
    let half_tip = GEO.needle_tip / 2.0;
    let tip_dy = GEO.needle_length;

    // 4 个顶点（uu 绝对坐标），顺序与源码 `d` 相同。
    let verts = [
        (c - half_base, c),
        (c + half_base, c),
        (c + half_tip, c - tip_dy),
        (c - half_tip, c - tip_dy),
    ];

    let mut out = String::with_capacity(72);
    for (i, (x, y)) in verts.iter().enumerate() {
        let (dx, dy) = (x - c, y - c);
        let rx = c + dx * cos - dy * sin;
        let ry = c + dx * sin + dy * cos;
        out.push_str(if i == 0 { "M " } else { " L " });
        out.push_str(&r3(rx - NEEDLE_BOX_ORIGIN));
        out.push(' ');
        out.push_str(&r3(ry - NEEDLE_BOX_ORIGIN));
    }
    out.push_str(" Z");
    out
}

/// Slint 线性渐变在**正方形元素盒**上的渐变轴覆盖长度。
///
/// 依据 `i-slint-core-1.18.1/graphics/brush.rs::line_for_angle`：
/// `angle_rad = (angle + 90).to_radians()`，正方形 `w = h = 170` 时渐变轴长度恒为
/// `w·(|sin| + |cos|)`（对 0°/30°/45°/90°/116.37° 逐点核算过）。
///
/// **注意**：软件渲染器并不支持 `Path.fill` 的渐变画刷
/// （`i-slint-renderer-software-1.18.1/lib.rs:3022-3030` 只取 `fill_brush.color()`，
/// 渐变画刷的 `color()` 是透明 ⇒ 整条指针画不出来）。所以本 crate 的实际渲染走
/// [`needle_slices`] 的 32 切片平色方案；本函数仅用于"如果宿主用渐变画刷"的场景。
pub fn gradient_extent(angle_deg: f64) -> f64 {
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    NEEDLE_BOX * (sin.abs() + cos.abs())
}

/// 指针渐变的一个 stop，用**指针自身参数** `u` 表达（与源码 `tokens.ts` 的
/// `needle[].offset` 同义）：`u = 0` 在尖端，`u = 1` 在圆心（hub）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeedleStopU {
    /// 0 = 尖端（y = 58），1 = 根部（圆心）。
    pub u: f64,
    /// sRGB。
    pub rgb: [u8; 3],
    /// 0..=255 的 alpha。
    pub alpha: u8,
}

/// 两套主题的指针 stop（`tokens.ts:112-129` 的 `THEME[theme].needle`）。
pub fn needle_stops_u(theme: Theme) -> &'static [NeedleStopU] {
    const DARK: [NeedleStopU; 2] = [
        NeedleStopU {
            u: 0.0,
            rgb: [255, 255, 255],
            alpha: 153, // 0.6
        },
        NeedleStopU {
            u: 1.0,
            rgb: [255, 255, 255],
            alpha: 0,
        },
    ];
    const LIGHT: [NeedleStopU; 3] = [
        NeedleStopU {
            u: 0.0,
            rgb: [255, 255, 255],
            alpha: 255,
        },
        NeedleStopU {
            u: 0.58,
            rgb: [0xa4, 0xa4, 0xa4],
            alpha: 255,
        },
        NeedleStopU {
            u: 1.0,
            rgb: [0x22, 0x22, 0x22],
            alpha: 255,
        },
    ];
    match theme {
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
    }
}

/// 在 `u` 处按 stop 表做分段线性采样（逐通道 + alpha，与 SVG 的 stop 插值一致）。
fn sample_needle_stops(theme: Theme, u: f64) -> ([u8; 3], u8) {
    let stops = needle_stops_u(theme);
    let x = u.clamp(0.0, 1.0);
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if x >= a.u && x <= b.u {
            let span = b.u - a.u;
            let k = if span == 0.0 { 0.0 } else { (x - a.u) / span };
            let lerp = |p: u8, q: u8| -> u8 {
                ((p as f64) + ((q as f64) - (p as f64)) * k).clamp(0.0, 255.0) as u8
            };
            return (
                [
                    lerp(a.rgb[0], b.rgb[0]),
                    lerp(a.rgb[1], b.rgb[1]),
                    lerp(a.rgb[2], b.rgb[2]),
                ],
                lerp(a.alpha, b.alpha),
            );
        }
    }
    let last = stops.last().copied().unwrap_or(NeedleStopU {
        u: 1.0,
        rgb: [0, 0, 0],
        alpha: 0,
    });
    (last.rgb, last.alpha)
}

/// 一个指针渐变 stop（沿**元素盒**轴；用渐变画刷时才需要）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStopGeo {
    /// 沿渐变轴的位置，0 = 起点端，1 = 终点端。
    pub position: f64,
    /// sRGB。
    pub rgb: [u8; 3],
    /// 0..=255 的 alpha。
    pub alpha: u8,
}

/// 把 [`needle_stops_u`] 换算成元素盒坐标下的位置（hub = 0.5，尖端 = `0.5 + 85/E`）。
///
/// 只给"渐变画刷"路径用（femtovg 等 GPU 后端）；软件渲染器请用 [`needle_slices`]。
pub fn needle_gradient(theme: Theme, angle_deg: f64) -> Vec<GradientStopGeo> {
    let range = GEO.needle_length / gradient_extent(angle_deg);
    let mut out: Vec<GradientStopGeo> = needle_stops_u(theme)
        .iter()
        .map(|s| GradientStopGeo {
            position: 0.5 + (1.0 - s.u) * range,
            rgb: s.rgb,
            alpha: s.alpha,
        })
        .collect();
    // Slint 的 stop 表要求 position 升序（避免依赖 sanitize 的排序行为）。
    out.sort_by(|a, b| a.position.total_cmp(&b.position));
    out
}

// ---------------------------------------------------------------------------
// 指针切片（软件渲染器唯一可行的"渐变"实现）
// ---------------------------------------------------------------------------

/// 沿指针轴等分的切片数（**默认值**；运行期可被 [`set_slice_cfg`] 覆盖）。
///
/// 选 32 的理由：每片的 alpha 量化误差上界 = `0.6·255/(2·32) ≈ 2.39/255`，
/// 已经小于验收阈值 4/255；再多为无收益的几何量。
///
/// **但量化误差不是主要误差**：相邻切片严格共边，共享边上的像素会被两片各覆盖
/// 一部分，合成两次（painter 顺序）后覆盖不足，接缝处比目标**暗** `a²·c(1−c)·255`
/// （`a` = 局部 alpha，`c` = 覆盖率，最坏 `c = 0.5` ⇒ `0.25a²·255`）。该项与片数
/// 无关、只与局部 alpha 有关，所以加大 N 不能消掉它：实测 N ∈ {32,64,96,128,192}
/// 的指针 `max|Δ|` = 9 / 19 / 22 / 19 / 23（都 ≥ 9），因为这几个 N 都是 32 的倍数，
/// 只是把已有接缝保留下来再插新接缝，接缝数（也就是暗像素数）随 N 单调上升。
///
/// 第二轮反证：改成"不透明预合成"（[`SliceComposite::Opaque`]）反而更差 —— 偏差从
/// `a²c(1−c)` 变成 `a·c(1−c)`（放大 `1/a`），0.5uu 密集扫描 `max|Δ|` 22 → 39/255。
/// 根因是软件渲染器逐路径 `over`（`path.rs:118-130`），覆盖率不是"权重"而是"再乘一次
/// 的 alpha"，所以片越不透明、共享边被前一片吃掉的比例越大。
pub const NEEDLE_SLICES: usize = 32;

/// 指针切片的运行期参数（默认 = [`NEEDLE_SLICES`] 片、严格共边，即历史行为）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliceCfg {
    /// 沿轴等分的片数（内部按 `max(1)` 兜底）。
    pub slices: usize,
    /// 每片沿轴**向外**多伸的长度（uu）；`0.0` = 严格共边（不重叠）。
    pub overlap: f64,
}

impl SliceCfg {
    /// 默认参数：32 片、无重叠。
    pub const FLAT: Self = Self {
        slices: NEEDLE_SLICES,
        overlap: 0.0,
    };
}

impl Default for SliceCfg {
    fn default() -> Self {
        Self::FLAT
    }
}

// 运行期覆盖（`0` = 未设置）。用原子而非 `static mut`：热路径只读一次，无锁。
static CFG_SLICES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static CFG_OVERLAP_MILLI: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// 设置全局切片参数（供实验/宿主调参；库本身不读环境变量）。
pub fn set_slice_cfg(cfg: SliceCfg) {
    use std::sync::atomic::Ordering;
    CFG_SLICES.store(cfg.slices.max(1), Ordering::Relaxed);
    CFG_OVERLAP_MILLI.store(
        (cfg.overlap.max(0.0) * 1000.0).round() as u32,
        Ordering::Relaxed,
    );
}

/// 当前全局切片参数。
pub fn slice_cfg() -> SliceCfg {
    use std::sync::atomic::Ordering;
    let slices = CFG_SLICES.load(Ordering::Relaxed);
    let milli = CFG_OVERLAP_MILLI.load(Ordering::Relaxed);
    SliceCfg {
        slices: if slices == 0 { NEEDLE_SLICES } else { slices },
        overlap: f64::from(milli) / 1000.0,
    }
}

// ---------------------------------------------------------------------------
// 合成模式：不透明预合成 vs 历史 alpha（第二轮结构性 A/B）
// ---------------------------------------------------------------------------

/// 指针切片的**合成模式**（唯一的结构性开关；几何与片数由 [`SliceCfg`] 管）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceComposite {
    /// 历史方案：片色 = `rgba(needle_rgb, a)`，由软件渲染器把片色与 `page` 底色
    /// **二次合成**（`path.rs:118-130` + `draw_functions.rs:862` 的逐路径 `over`）。
    Alpha,
    /// 不透明预合成：片色 = `a·needle_rgb + (1−a)·page`，alpha 恒 255。
    ///
    /// 只在前述"指针永远落在 `page` 底色上"成立时逐像素等价（本组件成立：环形轨道内
    /// 半径 118.5 > 指针长度 85，刻度/读数不在扫掠范围内，根 `Rectangle` 自己画 `page`）。
    ///
    /// **注意**：本模式**不能**消除接缝。软件渲染器是逐路径 `over` 合成，共享边像素
    /// 被两片各覆盖 `c` / `1−c`，于是
    ///
    /// `out = P·c(1−c) + C_i·c² + C_{i+1}·(1−c)`，偏差 `= c(1−c)·(P − C_i)`
    ///
    /// （`alpha` 模式同推导为 `−a²c(1−c)·255`）。不透明只把"二次合成造成的凹陷"从
    /// `a²·c(1−c)` 放大成 `a·c(1−c)`（dark：23/255 → 38/255，放大 `1/a = 1.67×`），
    /// 因为不透明片的覆盖权重把前一片的贡献整个盖掉了。
    Opaque,
}

impl SliceComposite {
    /// 全局配置里的编码（`0` 从不使用，`1`/`2` 显式）。
    const fn code(self) -> u8 {
        match self {
            Self::Opaque => 1,
            Self::Alpha => 2,
        }
    }

    const fn from_code(v: u8) -> Self {
        if v == 2 {
            Self::Alpha
        } else {
            Self::Opaque
        }
    }
}

// 运行期覆盖（`0` = 未设置）。
static CFG_COMPOSITE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// 设置全局合成模式（实验/宿主调参；库本身不读环境变量）。
pub fn set_slice_composite(mode: SliceComposite) {
    CFG_COMPOSITE.store(mode.code(), std::sync::atomic::Ordering::Relaxed);
}

/// 当前全局合成模式。
pub fn slice_composite() -> SliceComposite {
    let v = CFG_COMPOSITE.load(std::sync::atomic::Ordering::Relaxed);
    if v == 0 {
        SLICE_COMPOSITE
    } else {
        SliceComposite::from_code(v)
    }
}

/// 不透明预合成：`a·rgb + (1−a)·page`，逐通道四舍五入。
///
/// dark（`rgb = #fff`、`page = #000`）时恒等于 `(a)` 的灰：`a = 0.6·d/85` ⇒ `round(153·d/85)`，
/// 与"渐变画刷 + 黑底合成"读数完全一致（`examples/shots.rs` 的标定期望值即此）。
pub fn composite_over_page(rgb: [u8; 3], alpha: u8, page: [u8; 3]) -> [u8; 3] {
    let a = f64::from(alpha) / 255.0;
    let mix = |src: u8, dst: u8| -> u8 {
        (f64::from(src) * a + f64::from(dst) * (1.0 - a))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    [
        mix(rgb[0], page[0]),
        mix(rgb[1], page[1]),
        mix(rgb[2], page[2]),
    ]
}

/// 主题的 `page` 底色（`tokens.ts` 的 `page`；dark `#000000`、light `#f2f2f2`）。
///
/// 宿主若把量表放在别的底色上，应改用 [`needle_slices_on`] 显式传入实际底色。
pub fn theme_page(theme: Theme) -> [u8; 3] {
    crate::model::theme_tokens(theme).page
}

/// 一片指针（梯形）的平色几何。
#[derive(Debug, Clone, PartialEq)]
pub struct NeedleSliceGeo {
    /// 局部坐标（`0..170` 的指针元素盒）下的梯形 path。
    pub commands: String,
    /// 该片中心的采样色。
    pub rgb: [u8; 3],
    /// 该片中心的 alpha（0..=255）。
    pub alpha: u8,
}

/// 指针在距离圆心 `d` 处的半宽：`needleBase/2` 线性收到 `needleTip/2`。
fn needle_half_width(d: f64) -> f64 {
    let t = (d / GEO.needle_length).clamp(0.0, 1.0);
    GEO.needle_base / 2.0 * (1.0 - t) + GEO.needle_tip / 2.0 * t
}

/// 指针在距离圆心 `d` 处、横向偏移 `side`（`-1` = 左，`+1` = 右）的点，已转到局部坐标。
fn needle_point(angle_deg: f64, d: f64, side: f64) -> (f64, f64) {
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    let half = needle_half_width(d) * side;
    // 轴向前进 (sin, -cos)；横向 = (cos, sin)
    let x = CENTER + sin * d + cos * half;
    let y = CENTER - cos * d + sin * half;
    (x - NEEDLE_BOX_ORIGIN, y - NEEDLE_BOX_ORIGIN)
}

/// 默认合成模式（**发布口径**）。
///
/// 第二轮实测：不透明预合成把接缝从 `a²c(1−c)` 放大到 `a·c(1−c)`，指针轴 0.5uu 密集
/// 扫描的 `max|Δ|` 反而从 21/255 涨到 36/255 ⇒ 发布口径保留 `Alpha` + 32 片。
/// 复现：`python tools/needle_profile_dense.py --sweep`（对照表见该脚本 docstring）。
pub const SLICE_COMPOSITE: SliceComposite = SliceComposite::Alpha;

/// 把指针切成 [`slice_cfg`] 片梯形，每片一个平色。
///
/// 兼容入口：`page` 取主题默认底色（[`theme_page`]），合成模式取全局
/// [`slice_composite`]。宿主应优先用 [`needle_slices_on`] 显式传底色。
pub fn needle_slices(theme: Theme, angle_deg: f64) -> Vec<NeedleSliceGeo> {
    needle_slices_on(theme, angle_deg, theme_page(theme))
}

/// 同 [`needle_slices`]，但**显式传入量表的底色** `page`（不透明预合成要用它）。
///
/// 调用处唯一需要保证的事实：指针只落在 `page` 上（不跨过彩弧/刻度/读数），否则
/// 预合成色会在别的底色上露馅。
pub fn needle_slices_on(theme: Theme, angle_deg: f64, page: [u8; 3]) -> Vec<NeedleSliceGeo> {
    slices_with_mode(theme, angle_deg, page, slice_cfg(), slice_composite())
}

/// 同 [`needle_slices_on`]，但切片参数显式传入（不读全局 `SliceCfg`，合成模式仍取全局）。
pub fn needle_slices_on_with(
    theme: Theme,
    angle_deg: f64,
    page: [u8; 3],
    cfg: SliceCfg,
) -> Vec<NeedleSliceGeo> {
    slices_with_mode(theme, angle_deg, page, cfg, slice_composite())
}

/// 历史入口：**固定 alpha 语义**（= 上一轮基线），用于 A/B 与几何单测。
///
/// `overlap > 0` 的语义（结构性 A/B）：每片沿轴**向外**多伸 `overlap` uu，于是相邻
/// 两片在 `[d0_i, d0_i + overlap]` 重叠；painter 顺序是内→外，所以重叠区里"已有的"
/// alpha 是前一片的平色 alpha `a`。把后一片的 alpha 设成
/// `b' = (target − a) / (1 − a)`（`target` = 重叠区中点的目标 alpha），重叠区二次
/// 合成后正好回到 `target`。
///
/// **注意**：`b'` 是整片的平色 alpha，所以这一片**非重叠**的主体也变成 `b'`。递推
/// 关系 `a_i ≈ target_i − a_{i−1}` 收敛到 `a_i ≈ target_i / 2` —— 整条指针只剩半亮，
/// 误差只是从接缝搬到了片体内部。实测（N=32、overlap=0.35）：指针 `mean|Δ|`
/// 1.72 → 33.88、`max|Δ|` 9 → 60，全图 `mean|Δ|` 0.579 → 0.966 ⇒ 该变体被放弃。
pub fn needle_slices_with(theme: Theme, angle_deg: f64, cfg: SliceCfg) -> Vec<NeedleSliceGeo> {
    slices_with_mode(
        theme,
        angle_deg,
        theme_page(theme),
        cfg,
        SliceComposite::Alpha,
    )
}

/// 切片主循环：只产"片中心采样色 + alpha"（与合成模式无关），最后按 `mode` 收尾。
fn slices_with_mode(
    theme: Theme,
    angle_deg: f64,
    page: [u8; 3],
    cfg: SliceCfg,
    mode: SliceComposite,
) -> Vec<NeedleSliceGeo> {
    let angle = if angle_deg.is_finite() {
        angle_deg
    } else {
        0.0
    };
    let len = GEO.needle_length;
    let n = cfg.slices.max(1);
    let overlap = if cfg.overlap.is_finite() {
        cfg.overlap.max(0.0)
    } else {
        0.0
    };
    let step = len / n as f64;
    let mut out = Vec::with_capacity(n);
    // 每片的理想平色 alpha（0..1），供重叠补偿读取"前一片的实际 alpha"。
    let mut prev_alpha = 0.0f64;
    for i in 0..n {
        let d0 = step * i as f64;
        let d1 = if i + 1 == n {
            len
        } else {
            step * (i + 1) as f64
        };
        // 有重叠时才向外多伸（否则保持历史几何，逐字节可复现）。
        let d_out = if overlap > 0.0 {
            (d1 + overlap).min(len)
        } else {
            d1
        };
        let (l0x, l0y) = needle_point(angle, d0, -1.0);
        let (r0x, r0y) = needle_point(angle, d0, 1.0);
        let (r1x, r1y) = needle_point(angle, d_out, 1.0);
        let (l1x, l1y) = needle_point(angle, d_out, -1.0);
        let commands = format!(
            "M {} {} L {} {} L {} {} L {} {} Z",
            r3(l0x),
            r3(l0y),
            r3(r0x),
            r3(r0y),
            r3(r1x),
            r3(r1y),
            r3(l1x),
            r3(l1y)
        );
        // 片中心距离 → u（u = 1 在圆心，0 在尖端）
        let u = 1.0 - (d0 + d1) / 2.0 / len;
        let (rgb, alpha) = sample_needle_stops(theme, u);
        let mut alpha = alpha;
        if overlap > 0.0 {
            let packed = if i == 0 {
                // 首片没有前邻：取"伸展后整片"中点处的理想 alpha。
                let d_mid = (d0 + d_out) / 2.0;
                sample_needle_stops(theme, 1.0 - d_mid / len).1
            } else {
                // 重叠区中点 → 目标 alpha
                let d_mid = d0 + overlap / 2.0;
                let target = f64::from(sample_needle_stops(theme, 1.0 - d_mid / len).1) / 255.0;
                let a = prev_alpha.clamp(0.0, 0.999);
                let b = ((target - a) / (1.0 - a)).clamp(0.0, 1.0);
                (b * 255.0).round() as u8
            };
            alpha = packed;
        }
        prev_alpha = f64::from(alpha) / 255.0;
        out.push(NeedleSliceGeo {
            commands,
            rgb,
            alpha,
        });
    }
    if mode == SliceComposite::Opaque {
        // 预合成进颜色、alpha 拉满：渲染器不再做"片色 over 底色"，只剩覆盖权重。
        for s in &mut out {
            s.rgb = composite_over_page(s.rgb, s.alpha, page);
            s.alpha = 255;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_arc_is_the_golden_string() {
        assert_eq!(
            track_arc(),
            "M 51.076 234.924 A 130 130 0 1 1 234.924 234.924"
        );
    }

    #[test]
    fn ring_tables_have_240_segments_with_overlap_and_truncated_tail() {
        let t = table(GradientVariant::DlMulti);
        assert_eq!(t.len(), 240);
        assert!((t.a0[0] - -135.0).abs() < 1e-12);
        assert!((t.a1[0] - -133.595).abs() < 1e-12);
        assert!((t.a0[239] - 133.875).abs() < 1e-12);
        assert!((t.a1[239] - 135.0).abs() < 1e-12);
        // 其余段的角宽 = 1.125 + 0.28
        assert!((t.a1[1] - t.a0[1] - 1.405).abs() < 1e-12);
    }

    #[test]
    fn ring_view_prefix_and_boundary() {
        // 862 Mbps：sweep 终点 116.37°
        let v = ring_view(GradientVariant::DlMulti, 116.37);
        assert_eq!(v.visible, 223);
        // 边界段 = 第 223 段（a0 = 115.875°）截到 116.37°，终点就是 polar 端点
        let p = polar(CENTER, CENTER, GEO.radius, 116.37);
        assert!(v.boundary.starts_with("M "));
        assert!(v.boundary.ends_with(&format!("{} {}", r3(p.x), r3(p.y))));
        // 起始角：整组隐藏
        let v = ring_view(GradientVariant::DlMulti, -135.0);
        assert!(v.is_empty());
        // 满量程：240 段全画、无边界段
        let v = ring_view(GradientVariant::DlMulti, 135.0);
        assert_eq!(v.visible, 240);
        assert!(v.boundary.is_empty());
        // 超冲（微颤）：clamp 到 135
        let v = ring_view(GradientVariant::DlMulti, 137.0);
        assert_eq!(v.visible, 240);
    }

    #[test]
    fn lit_count_matches_reference_states() {
        let labels: &[f64] = &crate::scale::LABELS_STANDARD;
        assert_eq!(lit_count(labels, -135.0), 0); // 静止：0 档不亮
        assert_eq!(lit_count(labels, -134.9), 1);
        assert_eq!(lit_count(labels, 0.0), 5); // 0,5,10,50,100
        assert_eq!(lit_count(labels, 116.37), 8); // …750 亮，1000 不亮
        assert_eq!(lit_count(labels, 135.0), 9);
    }

    #[test]
    fn needle_zero_angle_is_straight_up_inside_the_box() {
        let d = needle_commands(0.0);
        // 盒子局部坐标：圆心 (85,85)，尖端 (85,0)，根部 (75,85)-(95,85)
        assert_eq!(d, "M 75 85 L 95 85 L 90 0 L 80 0 Z");
    }

    #[test]
    fn needle_quarter_turn_points_right() {
        // 90°：尖端指向 +x（局部 x = 170 就是盒右边界），根部仍在盒中心 (85,85)。
        let d = needle_commands(90.0);
        assert!(d.starts_with("M 85 75 L 85 95 L 170 "), "{d}");
        assert!(d.ends_with(" Z"), "{d}");
    }

    #[test]
    fn gradient_extent_at_cardinal_angles() {
        assert!((gradient_extent(0.0) - 170.0).abs() < 1e-9);
        assert!((gradient_extent(90.0) - 170.0).abs() < 1e-9);
        assert!((gradient_extent(45.0) - 170.0 * 2f64.sqrt()).abs() < 1e-9);
        assert!((gradient_extent(116.37) - 227.78).abs() < 0.05);
    }

    #[test]
    fn dark_gradient_tip_lands_on_the_box_edge_at_zero_angle() {
        let stops = needle_gradient(Theme::Dark, 0.0);
        // 升序：hub 在 0.5（透明），尖端在 1.0（0.6 alpha）
        assert!((stops[0].position - 0.5).abs() < 1e-12);
        assert_eq!(stops[0].alpha, 0);
        assert!((stops[1].position - 1.0).abs() < 1e-9);
        assert_eq!(stops[1].alpha, 153);
    }

    #[test]
    fn needle_slices_tile_the_needle_without_gaps() {
        let slices = needle_slices(Theme::Dark, 0.0);
        assert_eq!(slices.len(), NEEDLE_SLICES);
        // 第一片：根部整条底边（半宽 10）→ 顶边收窄到半宽 85-2.656=82.344 处
        assert_eq!(
            slices[0].commands,
            "M 75 85 L 95 85 L 94.844 82.344 L 75.156 82.344 Z"
        );
        // alpha 沿轴单调不减（越靠尖端越亮），末片最接近 0.6
        let mut prev = 0u8;
        for s in &slices {
            assert!(s.alpha >= prev);
            prev = s.alpha;
        }
        assert!(slices[NEEDLE_SLICES - 1].alpha > 140);
        assert!(slices[NEEDLE_SLICES - 1].alpha < 153);
    }

    #[test]
    fn slice_cfg_scales_the_count_without_touching_the_default() {
        // 显式参数路径：片数可变，alpha 仍单调、末片仍逼近 0.6。
        for n in [64usize, 96, 128, 192] {
            let slices = needle_slices_with(
                Theme::Dark,
                0.0,
                SliceCfg {
                    slices: n,
                    overlap: 0.0,
                },
            );
            assert_eq!(slices.len(), n);
            let mut prev = 0u8;
            for s in &slices {
                assert!(s.alpha >= prev, "n={n} 的 alpha 不再单调");
                prev = s.alpha;
            }
            assert!(slices[n - 1].alpha > 140);
        }
        // 默认仍是 32 片、零重叠（全局参数未被上面的实验污染）。
        assert_eq!(slice_cfg(), SliceCfg::FLAT);
        assert_eq!(needle_slices(Theme::Dark, 0.0).len(), NEEDLE_SLICES);
    }

    #[test]
    fn overlap_compensation_only_fixes_the_band_and_sinks_the_rest() {
        // 结构性 A/B 的机制：`b' = (target − a)/(1 − a)` 只对重叠区成立，而整片的
        // 平色 alpha 都变成 b' ⇒ 片体（非重叠区）远暗于理想值。
        //
        // 递推一下：`a_i ≈ target_i − a_{i−1}`，而 a_{i−1} 自己也是被压过的，于是
        // 收敛到 `a_i ≈ target_i / 2` —— 整条指针只剩半亮（实测 mean|Δ| 1.72 → 33.88、
        // max|Δ| 9 → 60，全图 mean|Δ| 0.579 → 0.966）。
        let flat = needle_slices_with(Theme::Dark, 0.0, SliceCfg::FLAT);
        let over = needle_slices_with(
            Theme::Dark,
            0.0,
            SliceCfg {
                slices: NEEDLE_SLICES,
                overlap: 0.35,
            },
        );
        assert_eq!(over.len(), flat.len());
        let last = NEEDLE_SLICES - 1;
        // 除首片外每片都比理想值暗（误差从接缝搬到了片体）
        for i in 1..NEEDLE_SLICES {
            assert!(
                over[i].alpha < flat[i].alpha,
                "第 {i} 片没有变暗：{} vs 理想 {}",
                over[i].alpha,
                flat[i].alpha
            );
        }
        let drop = i32::from(flat[last].alpha) - i32::from(over[last].alpha);
        assert!(
            drop > 40,
            "末片应掉一半以上：理想 {} → 实际 {}（掉 {drop} 级）",
            flat[last].alpha,
            over[last].alpha
        );
        // 几何确实向外多伸了 0.35uu（末片已被 len 截断 ⇒ 比较倒数第二片）
        assert_ne!(over[last - 1].commands, flat[last - 1].commands);
    }

    #[test]
    fn opaque_composite_reproduces_the_dark_gray_ramp() {
        // dark：白 over 黑 ⇒ round(255a) = a，与"渐变画刷 + 黑底合成"的读数逐值相等。
        for a in [0u8, 17, 64, 128, 153, 255] {
            assert_eq!(
                composite_over_page([255, 255, 255], a, [0, 0, 0]),
                [a, a, a]
            );
        }
        // light：alpha=255 保持原色；alpha=0 退化成 page 底色；中间值夹在两者之间。
        assert_eq!(
            composite_over_page([0x22, 0x22, 0x22], 255, [0xf2, 0xf2, 0xf2]),
            [0x22, 0x22, 0x22]
        );
        assert_eq!(
            composite_over_page([0x22, 0x22, 0x22], 0, [0xf2, 0xf2, 0xf2]),
            [0xf2, 0xf2, 0xf2]
        );
        let mid = composite_over_page([0x22, 0x22, 0x22], 128, [0xf2, 0xf2, 0xf2]);
        assert!(mid[0] > 0x22 && mid[0] < 0xf2, "{mid:?}");
    }

    #[test]
    fn opaque_mode_only_touches_colour_and_alpha() {
        let page = theme_page(Theme::Dark);
        assert_eq!(page, [0, 0, 0]);
        let alpha = slices_with_mode(
            Theme::Dark,
            116.37,
            page,
            SliceCfg::FLAT,
            SliceComposite::Alpha,
        );
        let opaque = slices_with_mode(
            Theme::Dark,
            116.37,
            page,
            SliceCfg::FLAT,
            SliceComposite::Opaque,
        );
        assert_eq!(alpha.len(), opaque.len());
        for (a, o) in alpha.iter().zip(&opaque) {
            // 几何逐字节不变：合成模式不碰梯形。
            assert_eq!(a.commands, o.commands);
            assert_eq!(o.alpha, 255);
            // dark：needle 白 over page 黑 ⇒ 三通道同值且等于原 alpha。
            assert_eq!(o.rgb, [a.alpha, a.alpha, a.alpha]);
        }
    }

    #[test]
    fn legacy_entries_are_pinned_to_alpha_semantics() {
        // `needle_slices_with` 是 A/B 基线：不管全局模式怎么设，它必须还出 alpha 片
        // （= 显式 `SliceComposite::Alpha` 的结果，逐片相同）。
        for theme in [Theme::Dark, Theme::Light] {
            let legacy = needle_slices_with(theme, 0.0, SliceCfg::FLAT);
            let pinned = slices_with_mode(
                theme,
                0.0,
                theme_page(theme),
                SliceCfg::FLAT,
                SliceComposite::Alpha,
            );
            assert_eq!(legacy, pinned);
        }
        // dark 的片色恒为 needle 白（渐变量的载体是 alpha，不是 rgb）。
        assert!(needle_slices_with(Theme::Dark, 0.0, SliceCfg::FLAT)
            .iter()
            .all(|x| x.rgb == [255, 255, 255]));
        // 默认（未设置全局）等于发布口径。
        assert_eq!(slice_composite(), SLICE_COMPOSITE);
    }

    #[test]
    fn light_theme_slices_are_already_opaque() {
        // light 的三个 stop alpha 全是 255 ⇒ 预合成是恒等变换（片色/alpha 都不变）。
        let page = theme_page(Theme::Light);
        let alpha = slices_with_mode(
            Theme::Light,
            0.0,
            page,
            SliceCfg::FLAT,
            SliceComposite::Alpha,
        );
        let opaque = slices_with_mode(
            Theme::Light,
            0.0,
            page,
            SliceCfg::FLAT,
            SliceComposite::Opaque,
        );
        for (a, o) in alpha.iter().zip(&opaque) {
            assert_eq!(a.rgb, o.rgb);
            assert_eq!(a.alpha, 255);
            assert_eq!(o.alpha, 255);
        }
    }
}
