//! `model` 模块的决策表 / token 表 / 状态机测试。

use speed_gauge_slint::model::{
    accent_for, labels_for, needle_path, safe_kbps, scale_kind_for, srgb_fallback, theme_tokens,
    variant_for, ConnectionMode, GaugeState, GradientVariant, MotionMode, ScaleKind, ScaleMode,
    Stage, Theme, ACCENT_DOWNLOAD, ACCENT_DOWNLOAD_SINGLE, ACCENT_UPLOAD, ACCENT_UPLOAD_SINGLE,
    SRGB_FALLBACK, THEME_DARK, THEME_LIGHT,
};

const ALL_STAGES: [Stage; 5] = [
    Stage::Idle,
    Stage::Ping,
    Stage::Download,
    Stage::Upload,
    Stage::Done,
];

const ALL_MODES: [ConnectionMode; 2] = [ConnectionMode::Multi, ConnectionMode::Single];

#[test]
fn enums_roundtrip_through_strings() {
    for stage in ALL_STAGES {
        assert_eq!(Stage::parse(stage.as_str()), Some(stage));
        assert_eq!(format!("{stage}"), stage.as_str());
    }
    for mode in ALL_MODES {
        assert_eq!(ConnectionMode::parse(mode.as_str()), Some(mode));
    }
    for mode in [ScaleMode::Fixed, ScaleMode::Auto] {
        assert_eq!(ScaleMode::parse(mode.as_str()), Some(mode));
    }
    for mode in [MotionMode::Tween, MotionMode::Spring] {
        assert_eq!(MotionMode::parse(mode.as_str()), Some(mode));
    }
    for theme in [Theme::Dark, Theme::Light] {
        assert_eq!(Theme::parse(theme.as_str()), Some(theme));
    }
    for variant in [
        GradientVariant::DlMulti,
        GradientVariant::UlMulti,
        GradientVariant::DlSingle,
        GradientVariant::UlSingle,
    ] {
        assert_eq!(GradientVariant::parse(variant.as_str()), Some(variant));
    }

    // 字面量与源码一致
    assert_eq!(Stage::Upload.as_str(), "upload");
    assert_eq!(Stage::Done.as_str(), "done");
    assert_eq!(GradientVariant::DlMulti.as_str(), "dl-multi");
    assert_eq!(GradientVariant::UlSingle.as_str(), "ul-single");
    assert_eq!(Theme::Light.as_str(), "light");
    // 未知取值
    assert_eq!(Stage::parse("bogus"), None);
    assert_eq!(GradientVariant::parse("dl"), None);
    assert!("idle".parse::<Stage>().is_ok());
}

/// 附录 §8.3 的决策表：4 类 stage × 2 种 mode = 8 组。
#[test]
fn variant_and_accent_decision_table() {
    let table = [
        (
            Stage::Upload,
            ConnectionMode::Multi,
            GradientVariant::UlMulti,
            ACCENT_UPLOAD,
        ),
        (
            Stage::Upload,
            ConnectionMode::Single,
            GradientVariant::UlSingle,
            ACCENT_UPLOAD_SINGLE,
        ),
        (
            Stage::Download,
            ConnectionMode::Multi,
            GradientVariant::DlMulti,
            ACCENT_DOWNLOAD,
        ),
        (
            Stage::Download,
            ConnectionMode::Single,
            GradientVariant::DlSingle,
            ACCENT_DOWNLOAD_SINGLE,
        ),
        // 其余阶段与 download 同类（ping 不改变任何视觉判定）
        (
            Stage::Idle,
            ConnectionMode::Multi,
            GradientVariant::DlMulti,
            ACCENT_DOWNLOAD,
        ),
        (
            Stage::Ping,
            ConnectionMode::Multi,
            GradientVariant::DlMulti,
            ACCENT_DOWNLOAD,
        ),
        (
            Stage::Done,
            ConnectionMode::Multi,
            GradientVariant::DlMulti,
            ACCENT_DOWNLOAD,
        ),
        (
            Stage::Ping,
            ConnectionMode::Single,
            GradientVariant::DlSingle,
            ACCENT_DOWNLOAD_SINGLE,
        ),
        (
            Stage::Done,
            ConnectionMode::Single,
            GradientVariant::DlSingle,
            ACCENT_DOWNLOAD_SINGLE,
        ),
        (
            Stage::Idle,
            ConnectionMode::Single,
            GradientVariant::DlSingle,
            ACCENT_DOWNLOAD_SINGLE,
        ),
    ];
    for (stage, mode, variant, accent) in table {
        assert_eq!(variant_for(stage, mode), variant, "{stage}/{mode}");
        assert_eq!(accent_for(stage, mode), accent, "{stage}/{mode}");
    }

    // sRGB 值：多连接的两个是**像素实测值**（不是源码 hex 回退列的矩阵近似），
    // 依据 = 参考截图同一像素位置的采样：download 箭头 rgb(0,197,189)、upload 箭头
    // rgb(180,90,237)。旧的 #00c9c0 / #b75cf0 与实测差 4 / 3 级，属于近似误差。
    assert_eq!(ACCENT_DOWNLOAD.srgb, [0x00, 0xc5, 0xbd]);
    assert_eq!(ACCENT_UPLOAD.srgb, [0xb4, 0x5a, 0xed]);
    // 单连接没有参考图覆盖 ⇒ 保留源码声明值（未经像素校准）
    assert_eq!(ACCENT_DOWNLOAD_SINGLE.srgb, [0xe5, 0x6b, 0x4f]);
    assert_eq!(ACCENT_UPLOAD_SINGLE.srgb, [0x7c, 0xdb, 0x2b]);
    // p3 原值保留备查
    assert_eq!(ACCENT_DOWNLOAD.p3, "color(display-p3 0.000 0.773 0.749)");
    assert_eq!(ACCENT_UPLOAD.p3, "color(display-p3 0.671 0.380 0.910)");
    assert_eq!(ACCENT_DOWNLOAD_SINGLE.p3, "#e56b4f");
}

#[test]
fn scale_selection_thresholds() {
    // fixed 恒为标准档
    assert_eq!(
        scale_kind_for(ScaleMode::Fixed, 4200.0),
        ScaleKind::Standard
    );
    // auto 严格大于 1000 才扩展
    assert_eq!(scale_kind_for(ScaleMode::Auto, 1000.0), ScaleKind::Standard);
    assert_eq!(
        scale_kind_for(ScaleMode::Auto, 1000.001),
        ScaleKind::Extended
    );
    assert_eq!(scale_kind_for(ScaleMode::Auto, 4200.0), ScaleKind::Extended);
    assert_eq!(
        scale_kind_for(ScaleMode::Auto, f64::NAN),
        ScaleKind::Standard
    );

    assert_eq!(labels_for(ScaleMode::Auto, 4200.0).len(), 10);
    assert_eq!(labels_for(ScaleMode::Fixed, 4200.0).len(), 9);
    assert_eq!(ScaleKind::Standard.labels().len(), 9);
    assert_eq!(ScaleKind::Extended.labels().len(), 10);
    assert_eq!(labels_for(ScaleMode::Auto, 4200.0)[9], 10000.0);
}

#[test]
fn srgb_fallback_table_is_complete() {
    assert_eq!(SRGB_FALLBACK.len(), 7);
    assert_eq!(
        srgb_fallback("color(display-p3 1 1 1)"),
        Some([0xff, 0xff, 0xff])
    );
    assert_eq!(
        srgb_fallback("color(display-p3 0.399 0.399 0.399)"),
        Some([0x66, 0x66, 0x66])
    );
    assert_eq!(
        srgb_fallback("color(display-p3 0.635 0.635 0.635)"),
        Some([0xa3, 0xa3, 0xa3])
    );
    assert_eq!(
        srgb_fallback("color(display-p3 0.886 0.886 0.886)"),
        Some([0xe2, 0xe2, 0xe2])
    );
    // 两条强调色 = 实测值（见 variant_and_accent_decision_table 的注释：实测纠正，
    // 不是为了让断言通过而降级期望）
    assert_eq!(
        srgb_fallback("color(display-p3 0.000 0.773 0.749)"),
        Some([0x00, 0xc5, 0xbd])
    );
    assert_eq!(
        srgb_fallback("color(display-p3 0.671 0.380 0.910)"),
        Some([0xb4, 0x5a, 0xed])
    );
    // 表里没有的键返回 None（源码是 `?? cssValue` 原样透传）
    assert_eq!(srgb_fallback("#26273b"), None);
}

#[test]
fn theme_tokens_dark_and_light() {
    let dark = theme_tokens(Theme::Dark);
    assert_eq!(dark.page, [0x00, 0x00, 0x00]);
    assert_eq!(dark.track, [0x26, 0x27, 0x3b]);
    assert_eq!(dark.label_on, [0xff, 0xff, 0xff]);
    assert_eq!(dark.label_off, [0x66, 0x66, 0x66]);
    assert_eq!(dark.number, [0xff, 0xff, 0xff]);
    assert_eq!(dark.unit, [0x66, 0x66, 0x66]);
    assert_eq!(dark.needle.len(), 2);
    assert_eq!(dark.needle[0].offset, 0.0);
    assert_eq!(dark.needle[0].color, [0xff, 0xff, 0xff]);
    assert_eq!(dark.needle[0].opacity, 0.6);
    assert_eq!(dark.needle[1].offset, 1.0);
    assert_eq!(dark.needle[1].opacity, 0.0);

    let light = theme_tokens(Theme::Light);
    assert_eq!(light.page, [0xf2, 0xf2, 0xf2]);
    assert_eq!(light.track, [0xe2, 0xe2, 0xe2]);
    assert_eq!(light.label_on, [0x00, 0x00, 0x00]);
    assert_eq!(light.label_off, [0xa3, 0xa3, 0xa3]);
    assert_eq!(light.needle.len(), 3);
    assert_eq!(light.needle[1].offset, 0.58);
    assert_eq!(light.needle[1].color, [0xa4, 0xa4, 0xa4]);
    assert_eq!(light.needle[2].color, [0x22, 0x22, 0x22]);
    assert_eq!(light.needle[2].opacity, 1.0);

    assert_eq!(theme_tokens(Theme::Dark), &THEME_DARK);
    assert_eq!(theme_tokens(Theme::Light), &THEME_LIGHT);
}

#[test]
fn needle_path_is_character_exact() {
    assert_eq!(needle_path(), "M 133, 143 L 153, 143 L 148, 58 L 138, 58 Z");
}

#[test]
fn label_lit_boundary_rules() {
    let state = GaugeState::new(Theme::Dark, ConnectionMode::Multi, ScaleMode::Fixed);
    // "0" 的刻度角是 -135；指针恰好在 -135 时它**不**亮（唯一一处严格大于）。
    assert!(!state.is_label_lit(0, -135.0));
    assert!(state.is_label_lit(0, -134.999));
    // "100" 的刻度角是 0；指针恰好在 0 时它亮（>=）。
    assert!(state.is_label_lit(4, 0.0));
    assert!(!state.is_label_lit(4, -0.001));
    // "1000" 的刻度角是 135
    assert!(state.is_label_lit(8, 135.0));
    assert!(!state.is_label_lit(8, 134.0));
    // 中间刻度用 >=
    assert!(state.is_label_lit(7, 101.25));
    assert!(!state.is_label_lit(7, 101.0));
    assert_eq!(state.label_angle(4), 0.0);
}

#[test]
fn gauge_state_defaults_and_stage_effects() {
    let mut state = GaugeState::new(Theme::Dark, ConnectionMode::Multi, ScaleMode::Fixed);
    assert_eq!(state.stage, Stage::Idle);
    assert_eq!(state.scale, ScaleKind::Standard);
    assert_eq!(state.speed_kbps, 0.0);
    // 构造期 numberText 硬编码 "0.00"，而 lastNumber 初值是空串。
    assert_eq!(state.number_text, "0.00");
    assert_eq!(state.last_number, "");
    assert_eq!(state.variant, GradientVariant::DlMulti);
    assert_eq!(state.accent, ACCENT_DOWNLOAD);

    // idle -> download：变体不变，不重置指针、不清零数字
    let effects = state.set_stage(Stage::Download);
    assert!(effects.changed);
    assert!(!effects.variant_changed);
    assert!(!effects.icon_uploading);
    assert!(!effects.reset_needle);
    assert!(!effects.clear_number);

    state.set_speed(500_000.0);
    assert_eq!(state.number_text, "500.00");

    // download -> upload：变体变化 + 重置 + 清零
    let effects = state.set_stage(Stage::Upload);
    assert!(effects.changed);
    assert!(effects.variant_changed);
    assert!(effects.icon_uploading);
    assert!(effects.reset_needle);
    assert!(effects.clear_number);
    assert_eq!(state.variant, GradientVariant::UlMulti);
    assert_eq!(state.accent, ACCENT_UPLOAD);
    assert_eq!(state.number_text, "0.00");

    // 同一 stage 幂等早退（连图标都不碰）
    let effects = state.set_stage(Stage::Upload);
    assert!(!effects.changed);
    assert_eq!(effects, Default::default());

    // upload -> done：换回 download 系变体，但**不**归位、**不**清零（done 保留最终读数）
    let effects = state.set_stage(Stage::Done);
    assert!(effects.changed);
    assert!(effects.variant_changed);
    assert!(!effects.reset_needle);
    assert!(!effects.clear_number);
}

#[test]
fn connection_mode_only_recomputes_variant_and_accent() {
    let mut state = GaugeState::new(Theme::Light, ConnectionMode::Multi, ScaleMode::Fixed);
    state.set_stage(Stage::Download);
    let effects = state.set_connection_mode(ConnectionMode::Single);
    assert!(effects.changed);
    assert!(effects.variant_changed);
    assert_eq!(state.variant, GradientVariant::DlSingle);
    assert_eq!(state.accent, ACCENT_DOWNLOAD_SINGLE);
    // 幂等
    let effects = state.set_connection_mode(ConnectionMode::Single);
    assert!(!effects.changed);
    assert_eq!(effects, Default::default());
}

/// 附录 B10：两处 `> 1000` 判定的口径不一致，必须复刻。
#[test]
fn b10_auto_scale_boundary_diverges_between_setters() {
    // setSpeed 用量化后的 mbps：1000.001 -> toFixed(2) -> 1000.0 -> 不 > 1000 -> STANDARD
    let mut via_speed = GaugeState::new(Theme::Dark, ConnectionMode::Multi, ScaleMode::Auto);
    via_speed.set_speed(1_000_001.0);
    assert_eq!(via_speed.scale, ScaleKind::Standard);

    // setScaleMode 用未量化的 speed_kbps / 1000 = 1000.001 -> > 1000 -> EXTENDED
    let mut via_mode = GaugeState::new(Theme::Dark, ConnectionMode::Multi, ScaleMode::Fixed);
    via_mode.set_speed(1_000_001.0);
    assert_eq!(via_mode.scale, ScaleKind::Standard);
    assert!(via_mode.set_scale_mode(ScaleMode::Auto));
    assert_eq!(via_mode.scale, ScaleKind::Extended);

    // 同一 stage 下再 set_speed 一次，量化口径又把档位压回 STANDARD
    via_mode.set_speed(1_000_001.0);
    assert_eq!(via_mode.scale, ScaleKind::Standard);

    // 幂等：重复设置同一 mode 不再重建
    assert!(!via_mode.set_scale_mode(ScaleMode::Auto));
}

#[test]
fn set_speed_angle_and_number() {
    let mut state = GaugeState::new(Theme::Dark, ConnectionMode::Multi, ScaleMode::Fixed);
    assert_eq!(state.set_speed(0.0), -135.0);
    assert!((state.set_speed(862_000.0) - 116.37).abs() < 1e-9);
    assert_eq!(state.number_text, "862.00");
    assert!((state.set_speed(1_000_000.0) - 135.0).abs() < 1e-9);

    // 非法入参折成 0
    assert_eq!(state.set_speed(f64::NAN), -135.0);
    assert_eq!(state.speed_kbps, 0.0);
    assert_eq!(state.set_speed(-5.0), -135.0);
    assert_eq!(state.speed_kbps, 0.0);
    assert_eq!(state.set_speed(f64::INFINITY), -135.0);

    // snapTo 同口径
    assert!((state.snap_to(4_200_000.0) - 135.0).abs() < 1e-9);
    assert_eq!(state.number_text, "4,200.00");
}

#[test]
fn update_number_dedupes_text() {
    let mut state = GaugeState::new(Theme::Dark, ConnectionMode::Multi, ScaleMode::Fixed);
    // 首帧：lastNumber 是空串，所以第一次写入必定生效
    assert!(state.update_number(0.0));
    assert!(!state.update_number(0.0));
    assert!(state.update_number(1000.0));
    assert_eq!(state.number_text, "1.00");
    assert!(state.reset());
    assert_eq!(state.number_text, "0.00");
    assert_eq!(state.speed_kbps, 0.0);
}

#[test]
fn safe_kbps_matches_js_guard() {
    assert_eq!(safe_kbps(f64::NAN), 0.0);
    assert_eq!(safe_kbps(f64::INFINITY), 0.0);
    assert_eq!(safe_kbps(-1.0), 0.0);
    assert_eq!(safe_kbps(0.0), 0.0);
    assert_eq!(safe_kbps(862_000.0), 862_000.0);
}
