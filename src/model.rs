//! 类型系统、设计 token 与仪表状态机（不依赖任何渲染代码）。
//!
//! 逐行等价移植：
//! - `src/gauge/gauge.ts:29-32`（枚举）、`283-335`（各 setter 的副作用顺序）、
//!   `344-355`（`currentVariant` / `currentAccent`）、`418-425`（点亮判定）
//! - `src/gauge/tokens.ts:56-131`（`SRGB_FALLBACK` / `ACCENT` / `THEME` / `TIMING`）
//! - `src/gauge/scale.ts:26` [`LABELS_STANDARD`] / `:34` [`LABELS_EXTENDED`]
//!
//! 移植决策（port-spec §6.4）：Slint 的 `#rrggbb` 恒为 sRGB，因此本模块把颜色**全部**
//! 归一成 `[u8; 3]` 的 sRGB 值，等价于源码里 `SUPPORTS_P3 == false` 的运行路径；
//! 宽色域原值仍以 `p3` 字符串形式保留在 [`Accent`] 里备查。

use crate::geometry::GEO;
use crate::scale::{
    angle_for_value, format_speed, kbps_to_mbps, label_angle_at, LABELS_EXTENDED, LABELS_STANDARD,
};

macro_rules! str_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $(
                #[doc = concat!("`", $text, "`")]
                $variant,
            )+
        }

        impl $name {
            /// 源码里的字符串字面量（转字符串的唯一口径）。
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            /// 从字符串解析，未知取值返回 `None`。
            pub fn parse(s: &str) -> Option<Self> {
                match s {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl std::str::FromStr for $name {
            type Err = ();
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s).ok_or(())
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

str_enum! {
    /// 测量阶段（`gauge.ts:29`）。`ping` 不改变任何视觉判定。
    Stage {
        Idle => "idle",
        Ping => "ping",
        Download => "download",
        Upload => "upload",
        Done => "done",
    }
}

str_enum! {
    /// 连接模式（`gauge.ts:30`）。
    ConnectionMode {
        Multi => "multi",
        Single => "single",
    }
}

str_enum! {
    /// 刻度模式（`gauge.ts:31`）。
    ScaleMode {
        Fixed => "fixed",
        Auto => "auto",
    }
}

str_enum! {
    /// 指针跟踪模型（`animator.ts:72` 的 `NeedleConfig.mode`）。
    MotionMode {
        Spring => "spring",
        Tween => "tween",
    }
}

str_enum! {
    /// 主题（`gauge.ts:35`，构造期决定、运行期不可切）。
    Theme {
        Dark => "dark",
        Light => "light",
    }
}

str_enum! {
    /// 圆环渐变的四个变体（`gauge.ts:32`，键名与 `gradient-stops.ts` 一致）。
    GradientVariant {
        DlMulti => "dl-multi",
        UlMulti => "ul-multi",
        DlSingle => "dl-single",
        UlSingle => "ul-single",
    }
}

impl Stage {
    /// 是否属于 upload 系（`gauge.ts:345` / `:351` 的唯一分支条件）。
    pub fn is_upload(self) -> bool {
        matches!(self, Stage::Upload)
    }
}

/// 刻度档位。源码用**引用同一性**（`next === this.labels`）判断是否需要重建刻度
/// （`gauge.ts:406`），Rust 侧用这个枚举做同一性判断（port-spec §8.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScaleKind {
    /// 9 档，`scale.ts:26`。
    Standard,
    /// 10 档，`scale.ts:34`。
    Extended,
}

impl ScaleKind {
    /// 该档位的刻度值表。
    pub fn labels(self) -> &'static [f64] {
        match self {
            ScaleKind::Standard => &LABELS_STANDARD,
            ScaleKind::Extended => &LABELS_EXTENDED,
        }
    }
}

/// 强调色（`tokens.ts:66-83` 的 `AccentColor`）。
///
/// `srgb` 是**实际使用**的那一列。多连接的两个值不是"回退表查出来的"，而是对
/// 参考截图做像素采样后的实测值（见 [`ACCENT_DOWNLOAD`] 的注释）；单连接的两个
/// 因为没有参考图，仍是源码声明的 hex 原值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accent {
    /// 源码里的键名：`download` / `upload` / `downloadSingle` / `uploadSingle`。
    pub name: &'static str,
    /// 宽色域原值（`p3` 字段原文，留档；本 crate 不消费，但宿主可能需要 display-p3）。
    pub p3: &'static str,
    /// sRGB 实测/声明值（本 crate 恒定使用这一列）。
    pub srgb: [u8; 3],
}

/// `download`：`color(display-p3 0.000 0.773 0.749)`。
///
/// **`srgb` 是像素实测值，不是矩阵换算值。** 原 `#00c9c0` 是项目作者用 P3→sRGB
/// 矩阵手算的近似；参考截图是 Chromium 把 display-p3 颜色真实渲染后再落到 sRGB
/// PNG 的结果，同一像素位置（单位行 download 多箭头图标）实测 `rgb(0,197,189)`，
/// 与 `#00c9c0` 差 4 级。以实测为准：`srgb = [0, 197, 189]`。
pub const ACCENT_DOWNLOAD: Accent = Accent {
    name: "download",
    p3: "color(display-p3 0.000 0.773 0.749)",
    srgb: [0x00, 0xc5, 0xbd],
};
/// `upload`：`color(display-p3 0.671 0.380 0.910)`。
///
/// 同 [`ACCENT_DOWNLOAD`]：原矩阵近似 `#b75cf0` 与参考截图实测
/// `rgb(180,90,237)` 差 3 级，改用实测值。
pub const ACCENT_UPLOAD: Accent = Accent {
    name: "upload",
    p3: "color(display-p3 0.671 0.380 0.910)",
    srgb: [0xb4, 0x5a, 0xed],
};
/// `downloadSingle`：`#e56b4f`（源码 p3 列本身就是 hex）。
///
/// **未经像素校准**：参考截图只覆盖多连接状态（单连接强调色没出现在 8 张验收
/// 图里），无从实测，因此保留源码声明值，不做任何矩阵/经验修正。
pub const ACCENT_DOWNLOAD_SINGLE: Accent = Accent {
    name: "downloadSingle",
    p3: "#e56b4f",
    srgb: [0xe5, 0x6b, 0x4f],
};
/// `uploadSingle`：`#7cdb2b`（同样**未经像素校准**，见 [`ACCENT_DOWNLOAD_SINGLE`]）。
pub const ACCENT_UPLOAD_SINGLE: Accent = Accent {
    name: "uploadSingle",
    p3: "#7cdb2b",
    srgb: [0x7c, 0xdb, 0x2b],
};

/// `gauge.ts:344-348` 的 `currentVariant()`：只有 `upload` 阶段切到 upload 系。
pub fn variant_for(stage: Stage, mode: ConnectionMode) -> GradientVariant {
    let upload = stage.is_upload();
    match mode {
        ConnectionMode::Single => {
            if upload {
                GradientVariant::UlSingle
            } else {
                GradientVariant::DlSingle
            }
        }
        ConnectionMode::Multi => {
            if upload {
                GradientVariant::UlMulti
            } else {
                GradientVariant::DlMulti
            }
        }
    }
}

/// `gauge.ts:350-355` 的 `currentAccent()`。
pub fn accent_for(stage: Stage, mode: ConnectionMode) -> Accent {
    let upload = stage.is_upload();
    match mode {
        ConnectionMode::Single => {
            if upload {
                ACCENT_UPLOAD_SINGLE
            } else {
                ACCENT_DOWNLOAD_SINGLE
            }
        }
        ConnectionMode::Multi => {
            if upload {
                ACCENT_UPLOAD
            } else {
                ACCENT_DOWNLOAD
            }
        }
    }
}

/// 刻度档位选择（`gauge.ts:263` / `:325`）：`auto` 且 `mbps > 1000` 用扩展档，否则标准档。
///
/// 注意调用点传入的 `mbps` 口径不同（port-spec §8.4 / 附录 B10）：
/// - `setSpeed` 传**量化后**的 `kbps_to_mbps(safe)`；
/// - `setScaleMode` 传**未量化**的 `speed_kbps / 1000`。
pub fn scale_kind_for(scale_mode: ScaleMode, mbps: f64) -> ScaleKind {
    if scale_mode == ScaleMode::Auto && mbps > LABELS_STANDARD[LABELS_STANDARD.len() - 1] {
        ScaleKind::Extended
    } else {
        ScaleKind::Standard
    }
}

/// 同 [`scale_kind_for`]，直接返回刻度值表。
pub fn labels_for(scale_mode: ScaleMode, mbps: f64) -> &'static [f64] {
    scale_kind_for(scale_mode, mbps).labels()
}

/// `tokens.ts:56-64` 的 `SRGB_FALLBACK` 全表（7 条，键为 display-p3 原值字符串）。
///
/// **故意偏离源码的一处**（其余 5 条形如原表）：末两条强调色不用作者手算的矩阵近似
/// `#00c9c0` / `#b75cf0`，而用参考截图上同一像素位置的实测值 `rgb(0,197,189)` /
/// `rgb(180,90,237)`（依据见 [`ACCENT_DOWNLOAD`]）。矩阵近似与 Chromium 的真实
/// P3→sRGB 落盘结果差 3–4 级，实测优先。
pub const SRGB_FALLBACK: [(&str, [u8; 3]); 7] = [
    ("color(display-p3 1 1 1)", [0xff, 0xff, 0xff]),
    ("color(display-p3 0 0 0)", [0x00, 0x00, 0x00]),
    ("color(display-p3 0.399 0.399 0.399)", [0x66, 0x66, 0x66]),
    ("color(display-p3 0.635 0.635 0.635)", [0xa3, 0xa3, 0xa3]),
    ("color(display-p3 0.886 0.886 0.886)", [0xe2, 0xe2, 0xe2]),
    ("color(display-p3 0.000 0.773 0.749)", [0x00, 0xc5, 0xbd]),
    ("color(display-p3 0.671 0.380 0.910)", [0xb4, 0x5a, 0xed]),
];

/// `gauge.ts:46-48` 的 `color(cssValue)`：`SUPPORTS_P3 ? cssValue : (SRGB_FALLBACK[v] ?? v)`。
///
/// 本 crate 恒定走回退分支；表里查不到时返回 `None`（此时应视为非法输入）。
pub fn srgb_fallback(css_value: &str) -> Option<[u8; 3]> {
    SRGB_FALLBACK
        .iter()
        .find(|(k, _)| *k == css_value)
        .map(|(_, v)| *v)
}

/// 指针渐变的单个 stop（`tokens.ts:98` 的 `needle: { offset, color, opacity }[]`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeedleStop {
    /// 纵向位置，0 = 尖端（y = 58），1 = 根部（y = 143）。
    pub offset: f64,
    /// `stop-color` 的 sRGB 值。
    pub color: [u8; 3],
    /// `stop-opacity`。
    pub opacity: f64,
}

/// 一套主题的全部颜色 token（`tokens.ts:85-99` 的 `GaugeTheme`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeTokens {
    /// 页面底色。
    pub page: [u8; 3],
    /// `gaugeArcInactiveTrack`：未点亮的轨道弧。
    pub track: [u8; 3],
    /// `highEmphasisText`：被指针扫过的刻度。
    pub label_on: [u8; 3],
    /// `inverseSecondaryText`：未扫过的刻度。
    pub label_off: [u8; 3],
    /// 数字颜色（`highEmphasisText`）。
    pub number: [u8; 3],
    /// 单位行颜色（`inverseSecondaryText`）。
    pub unit: [u8; 3],
    /// 指针渐变 stop，上（尖端）→ 下（根部）。
    pub needle: &'static [NeedleStop],
}

/// 暗色主题（`tokens.ts:102-116`）。
pub const THEME_DARK: ThemeTokens = ThemeTokens {
    page: [0x00, 0x00, 0x00],
    // `track` 在源码里本身就是 hex，不经 SRGB_FALLBACK。
    track: [0x26, 0x27, 0x3b],
    label_on: [0xff, 0xff, 0xff],
    label_off: [0x66, 0x66, 0x66],
    number: [0xff, 0xff, 0xff],
    unit: [0x66, 0x66, 0x66],
    needle: &[
        NeedleStop {
            offset: 0.0,
            color: [0xff, 0xff, 0xff],
            opacity: 0.6,
        },
        NeedleStop {
            offset: 1.0,
            color: [0xff, 0xff, 0xff],
            opacity: 0.0,
        },
    ],
};

/// 浅色主题（`tokens.ts:117-130`）。
pub const THEME_LIGHT: ThemeTokens = ThemeTokens {
    page: [0xf2, 0xf2, 0xf2],
    track: [0xe2, 0xe2, 0xe2],
    label_on: [0x00, 0x00, 0x00],
    label_off: [0xa3, 0xa3, 0xa3],
    number: [0x00, 0x00, 0x00],
    unit: [0xa3, 0xa3, 0xa3],
    needle: &[
        NeedleStop {
            offset: 0.0,
            color: [0xff, 0xff, 0xff],
            opacity: 1.0,
        },
        NeedleStop {
            offset: 0.58,
            color: [0xa4, 0xa4, 0xa4],
            opacity: 1.0,
        },
        NeedleStop {
            offset: 1.0,
            color: [0x22, 0x22, 0x22],
            opacity: 1.0,
        },
    ],
};

/// 按主题取 token 表（`tokens.ts:101` 的 `THEME[theme]`）。
pub fn theme_tokens(theme: Theme) -> &'static ThemeTokens {
    match theme {
        Theme::Dark => &THEME_DARK,
        Theme::Light => &THEME_LIGHT,
    }
}

/// `tokens.ts:134-141` 的 `TIMING`（秒）。
pub const NEEDLE_TRACKING: f64 = 0.2;
/// 相位切换归位的时长（秒）。
pub const NEEDLE_RETURN: f64 = 0.5;
/// 刻度逐个入场的错峰间隔（秒）。
pub const LABEL_STAGGER: f64 = 0.08;
/// 刻度淡入时长（秒）。
pub const LABEL_FADE_IN: f64 = 0.08;

/// `setStage` 造成的副作用集合（`gauge.ts:283-302`，顺序敏感，见 port-spec §8.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StageEffects {
    /// `false` ⇒ 命中了 ① 幂等早退，本结构其余字段无意义。
    pub changed: bool,
    /// 是否需要重建 240 段圆环（源码 `renderVariant` 的 `lastVariant` 缓存）。
    pub variant_changed: bool,
    /// `true` ⇒ 显示 upload 图标、隐藏 download 图标。
    pub icon_uploading: bool,
    /// ⑤ 是否需要 `motion.reset()`（500 ms easeInOut 归位）。
    pub reset_needle: bool,
    /// ⑤ 是否把数字清零成 `"0.00"`。
    pub clear_number: bool,
}

/// `setConnectionMode` 造成的副作用（`gauge.ts:312-319`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModeEffects {
    /// `false` ⇒ 幂等早退。
    pub changed: bool,
    /// 是否需要重建圆环段表。
    pub variant_changed: bool,
}

/// 仪表的全部非渲染状态 + 各 setter 的**纯决策**语义。
///
/// 与源码的对应关系：这个结构持有 `Gauge` 里除 DOM 节点以外的所有私有字段
/// （`gauge.ts:112-122`）。需要渲染的一侧（Slint 组件层）持有本结构 +
/// [`crate::animation::NeedleMotion`]，并按 [`StageEffects`] 执行副作用。
#[derive(Debug, Clone, PartialEq)]
pub struct GaugeState {
    /// 当前阶段（`gauge.ts:113`）。
    pub stage: Stage,
    /// 连接模式（`gauge.ts:114`）。
    pub connection_mode: ConnectionMode,
    /// 刻度模式（`gauge.ts:115`）。
    pub scale_mode: ScaleMode,
    /// 主题（`gauge.ts:97`，`readonly`，构造期决定）。
    pub theme: Theme,
    /// 当前刻度档位（对应源码 `gauge.ts:112` 的 `labels` 引用）。
    pub scale: ScaleKind,
    /// 最近一次 `setSpeed` / `snapTo` 的 kbps（已把非法值折成 0，`gauge.ts:116`）。
    pub speed_kbps: f64,
    /// 数字 `<text>` 的当前内容（`gauge.ts:213` 初值 `"0.00"`）。
    pub number_text: String,
    /// `lastNumber` 去抖用（`gauge.ts:118`，初值空串，故首次写入必定生效）。
    pub last_number: String,
    /// 当前圆环渐变变体（`gauge.ts:119` 的 `lastVariant` 的当前值）。
    pub variant: GradientVariant,
    /// `lastVariant` 缓存（`gauge.ts:119`），初值 `None`；构造期 `renderVariant` 之后即被填充。
    pub last_variant: Option<GradientVariant>,
    /// 当前强调色（`currentAccent()` 的结果）。
    pub accent: Accent,
}

impl GaugeState {
    /// 构造（`gauge.ts:124-133` 的状态部分；渲染节点不在此模块）。
    ///
    /// 源码构造期会调用一次 `renderVariant(currentVariant())`（`gauge.ts:243`），
    /// 因此 `last_variant` 在构造后即等于 `variant_for(Idle, mode)`。
    pub fn new(theme: Theme, connection_mode: ConnectionMode, scale_mode: ScaleMode) -> Self {
        let variant = variant_for(Stage::Idle, connection_mode);
        Self {
            stage: Stage::Idle,
            connection_mode,
            scale_mode,
            theme,
            scale: ScaleKind::Standard,
            speed_kbps: 0.0,
            number_text: format_speed(0.0),
            last_number: String::new(),
            variant,
            last_variant: Some(variant),
            accent: accent_for(Stage::Idle, connection_mode),
        }
    }

    /// 当前刻度值表（`gauge.ts:112` 的 `this.labels`）。
    pub fn labels(&self) -> &'static [f64] {
        self.scale.labels()
    }

    /// 当前主题的 token 表。
    pub fn tokens(&self) -> &'static ThemeTokens {
        theme_tokens(self.theme)
    }

    /// `gauge.ts:405-409` 的 `applyLabels`：同一性相同时不做任何事。返回是否重建了刻度。
    pub fn apply_scale(&mut self, next: ScaleKind) -> bool {
        if next == self.scale {
            return false;
        }
        self.scale = next;
        true
    }

    /// 第 `index` 个刻度的角度（`gauge.ts:385` / `scale.ts:36-39`）。
    pub fn label_angle(&self, index: usize) -> f64 {
        label_angle_at(index, self.labels().len())
    }

    /// 第 `index` 个刻度是否被点亮（`gauge.ts:418-425`，逐字符复刻）。
    ///
    /// `value === 0 ? angle > labelAngle : angle >= labelAngle`：
    /// `0` 刻度用**严格大于**（静止在 -135° 时不亮），其余用 `>=`（指针正好落在刻度角上时亮）。
    /// `angle` 应传 `paint()` 里已 clamp 到 `[-135, 135]` 的 `arcAngle`。
    pub fn is_label_lit(&self, index: usize, angle: f64) -> bool {
        let value = self.labels()[index];
        let label_angle = self.label_angle(index);
        if value == 0.0 {
            angle > label_angle
        } else {
            angle >= label_angle
        }
    }

    /// `gauge.ts:283-302` 的 `setStage`，返回需要由渲染层执行的副作用。
    pub fn set_stage(&mut self, stage: Stage) -> StageEffects {
        if self.stage == stage {
            return StageEffects::default();
        }
        self.stage = stage;

        let variant = variant_for(stage, self.connection_mode);
        // 源码判据：`lastVariant === variant && ringGroup.childNodes.length > 0` 才早退。
        // `childNodes.length > 0` 是渲染层事实（首次渲染后恒真），故这里只比变体。
        let variant_changed = self.last_variant != Some(variant);
        self.last_variant = Some(variant);
        self.variant = variant;

        let uploading = stage.is_upload();
        self.accent = accent_for(stage, self.connection_mode);

        let reset_needle = matches!(stage, Stage::Upload | Stage::Idle);
        let clear_number = reset_needle;
        if clear_number {
            self.update_number(0.0);
        }

        StageEffects {
            changed: true,
            variant_changed,
            icon_uploading: uploading,
            reset_needle,
            clear_number,
        }
    }

    /// `gauge.ts:257-267` 的 `setSpeed`，返回指针目标角（度）。
    ///
    /// 非法/非正入参折成 0；`auto` 档用**量化后**的 Mbps 判 `> 1000`（附录 B10 的第一处）。
    pub fn set_speed(&mut self, kbps: f64) -> f64 {
        let safe = safe_kbps(kbps);
        self.speed_kbps = safe;
        let mbps = kbps_to_mbps(safe);
        if self.scale_mode == ScaleMode::Auto {
            self.apply_scale(scale_kind_for(self.scale_mode, mbps));
        }
        let angle = angle_for_value(self.labels(), mbps);
        self.update_number(safe);
        angle
    }

    /// `gauge.ts:270-281` 的 `snapTo`：与 [`Self::set_speed`] 同口径，但调用方需用
    /// `snap = true` 直接把指针摆到目标角（无动画），并立刻重绘。
    pub fn snap_to(&mut self, kbps: f64) -> f64 {
        let safe = safe_kbps(kbps);
        self.speed_kbps = safe;
        let mbps = kbps_to_mbps(safe);
        if self.scale_mode == ScaleMode::Auto {
            self.apply_scale(scale_kind_for(self.scale_mode, mbps));
        }
        let angle = angle_for_value(self.labels(), mbps);
        self.update_number(safe);
        angle
    }

    /// `gauge.ts:312-319` 的 `setConnectionMode`：只重算变体与强调色，不动指针/图标显隐。
    pub fn set_connection_mode(&mut self, mode: ConnectionMode) -> ModeEffects {
        if self.connection_mode == mode {
            return ModeEffects::default();
        }
        self.connection_mode = mode;
        let variant = variant_for(self.stage, mode);
        let variant_changed = self.last_variant != Some(variant);
        self.last_variant = Some(variant);
        self.variant = variant;
        self.accent = accent_for(self.stage, mode);
        ModeEffects {
            changed: true,
            variant_changed,
        }
    }

    /// `gauge.ts:321-329` 的 `setScaleMode`。
    ///
    /// 与 [`Self::set_speed`] 的**口径差异**（附录 B10，真实行为，逐字复刻）：
    /// 这里判 `speed_kbps / 1000 > 1000`，用**未量化**的 Mbps。
    pub fn set_scale_mode(&mut self, mode: ScaleMode) -> bool {
        if self.scale_mode == mode {
            return false;
        }
        self.scale_mode = mode;
        let next = if mode == ScaleMode::Auto && self.speed_kbps / 1000.0 > LABELS_STANDARD[8] {
            ScaleKind::Extended
        } else {
            ScaleKind::Standard
        };
        self.apply_scale(next)
    }

    /// `gauge.ts:331-335` 的 `reset`：只清读数并要求归位，**不改** stage/labels。
    ///
    /// 调用方需同时执行 `motion.reset()`。返回数字文本是否变化。
    pub fn reset(&mut self) -> bool {
        self.speed_kbps = 0.0;
        self.update_number(0.0)
    }

    /// `gauge.ts:411-416` 的 `updateNumber`（按文本去抖）。返回是否真的改了 DOM 文本。
    pub fn update_number(&mut self, kbps: f64) -> bool {
        let text = format_speed(kbps);
        if text == self.last_number {
            return false;
        }
        self.last_number = text.clone();
        self.number_text = text;
        true
    }
}

/// `gauge.ts:258` 的 `safe`：`Number.isFinite(kbps) && kbps > 0 ? kbps : 0`。
pub fn safe_kbps(kbps: f64) -> f64 {
    if kbps.is_finite() && kbps > 0.0 {
        kbps
    } else {
        0.0
    }
}

/// 指针多边形的 `d`（`gauge.ts:188-192`）。
///
/// 源码用模板字符串直接插入数字、分隔符是 `", "`（逗号 + 空格），输出逐字符为
/// `"M 133, 143 L 153, 143 L 148, 58 L 138, 58 Z"`（`f64: Display` 对整数值不给 `.0`，与 JS 一致）。
pub fn needle_path() -> String {
    let c = GEO.center;
    let half_base = GEO.needle_base / 2.0;
    let half_tip = GEO.needle_tip / 2.0;
    let tip_y = GEO.center - GEO.needle_length;
    format!(
        "M {}, {} L {}, {} L {}, {} L {}, {} Z",
        c - half_base,
        c,
        c + half_base,
        c,
        c + half_tip,
        tip_y,
        c - half_tip,
        tip_y
    )
}
