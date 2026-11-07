//! 多实例独立性（headless 软件渲染器，同一进程 3 个 `SpeedGauge`）。
//!
//! 三条断言：
//! 1. 不同 (stage, connection, scale, theme, speed) 组合的实例，`angle()` /
//!    `number_text()` / `lit_count()` / `ring_visible_count()` 互不相同，且与**用公开纯函数
//!    独立重算**的结果一致（`scale::angle_for_value` / `renderer::lit_count` /
//!    `renderer::ring_view`）；
//! 2. 改 A 不影响 B：对 A 做 `snap_to` + stage/theme/连接/刻度切换 + 若干帧后，B 的每个可观察
//!    字段（控制器 getter + 视图属性）逐位不变；
//! 3. `snap_to` 是即时定位（帧前帧后角度一致），`set_speed_kbps` 必须推进帧才会动。
//!
//! 平台是**线程本地**的（`i-slint-core::context::GLOBAL_CONTEXT` 是 `thread_local!`），
//! 所以每个测试线程各自装一次 headless 平台；装好的 `WindowSlot` 缓存在 thread-local 里，
//! 避免同一线程重复 `set_platform`。

use std::cell::RefCell;

use speed_gauge_slint::shots::{self, WindowSlot};
use speed_gauge_slint::{
    model, renderer, scale, ConnectionMode, GaugeHandle, GaugeStage, GaugeTheme, ScaleMode,
    SpeedGauge,
};

thread_local! {
    /// 每线程一份 headless 平台（`set_platform` 每线程只能成功一次）。
    static SLOT: RefCell<Option<WindowSlot>> = const { RefCell::new(None) };
}

fn headless_slot() -> WindowSlot {
    SLOT.with(|cell| {
        cell.borrow_mut()
            .get_or_insert_with(|| shots::install().expect("装不上 headless 软件渲染平台"))
            .clone()
    })
}

/// 一个实例的输入组合。
#[derive(Clone, Copy)]
struct Combo {
    stage: GaugeStage,
    connection: ConnectionMode,
    scale: ScaleMode,
    theme: GaugeTheme,
    kbps: f64,
}

/// 组合 A：Download / Multi / Fixed / Dark / 862 Mbps。
const A: Combo = Combo {
    stage: GaugeStage::Download,
    connection: ConnectionMode::Multi,
    scale: ScaleMode::Fixed,
    theme: GaugeTheme::Dark,
    kbps: 862_000.0,
};
/// 组合 B：Upload / Single / Auto / Light / 1500 Mbps（> 1000 ⇒ 自动切 10 档扩展刻度）。
const B: Combo = Combo {
    stage: GaugeStage::Upload,
    connection: ConnectionMode::Single,
    scale: ScaleMode::Auto,
    theme: GaugeTheme::Light,
    kbps: 1_500_000.0,
};
/// 组合 C：Ping / Multi / Fixed / Light / 8 Mbps。
const C: Combo = Combo {
    stage: GaugeStage::Ping,
    connection: ConnectionMode::Multi,
    scale: ScaleMode::Fixed,
    theme: GaugeTheme::Light,
    kbps: 8_000.0,
};

fn model_stage(stage: GaugeStage) -> model::Stage {
    match stage {
        GaugeStage::Idle => model::Stage::Idle,
        GaugeStage::Ping => model::Stage::Ping,
        GaugeStage::Download => model::Stage::Download,
        GaugeStage::Upload => model::Stage::Upload,
        GaugeStage::Done => model::Stage::Done,
    }
}

fn model_connection(mode: ConnectionMode) -> model::ConnectionMode {
    match mode {
        ConnectionMode::Multi => model::ConnectionMode::Multi,
        ConnectionMode::Single => model::ConnectionMode::Single,
    }
}

fn model_scale(mode: ScaleMode) -> model::ScaleMode {
    match mode {
        ScaleMode::Fixed => model::ScaleMode::Fixed,
        ScaleMode::Auto => model::ScaleMode::Auto,
    }
}

/// 用**公开纯函数**独立重算期望值：(角度°, 读数字符串, 点亮数, 彩弧段数)。
fn expected(combo: &Combo) -> (f64, String, usize, usize) {
    let mbps = scale::kbps_to_mbps(combo.kbps);
    let labels = model::scale_kind_for(model_scale(combo.scale), mbps).labels();
    let angle = scale::angle_for_value(labels, mbps);
    let arc = angle.clamp(renderer::START_ANGLE, renderer::END_ANGLE);
    let lit = renderer::lit_count(labels, arc);
    let variant = model::variant_for(model_stage(combo.stage), model_connection(combo.connection));
    let ring = renderer::ring_view(variant, arc).visible;
    (angle, scale::format_speed(combo.kbps), lit, ring)
}

/// 按组合创建一个实例 + 控制器。
///
/// 关掉动画与微颤：角度立刻到位，断言与帧数无关（这是 `snap` 级语义，不是动画语义）。
fn make(combo: &Combo) -> (SpeedGauge, GaugeHandle) {
    let ui = SpeedGauge::new().expect("SpeedGauge::new");
    let handle = GaugeHandle::attach(&ui);
    handle.set_animation_enabled(false);
    handle.set_wobble_enabled(false);
    handle.set_theme(combo.theme);
    handle.set_connection_mode(combo.connection);
    handle.set_scale_mode(combo.scale);
    handle.set_stage(combo.stage);
    // `set_stage(upload|idle)` 会挂一条 500 ms 归位 tween。`snap_to` 现在会先
    // `cancel_pending_return()`（见 `GaugeHandle::snap_to` 的文档），所以这里可以直接精确
    // 冻结读数 —— 以前必须先空推 40 帧把归位跑完，否则后续帧会把指针拽回 −135°。
    handle.snap_to(combo.kbps);
    (ui, handle)
}

fn assert_pairwise_distinct<T: PartialEq + std::fmt::Debug>(name: &str, values: &[T]) {
    for i in 0..values.len() {
        for j in (i + 1)..values.len() {
            assert_ne!(
                values[i], values[j],
                "{name}：实例 {i} 与实例 {j} 取值相同，多实例没有区分开：{values:?}"
            );
        }
    }
}

/// 一个实例的全部可观察状态（浮点用 `to_bits`，逐位比较）。
#[derive(Debug, PartialEq)]
struct Snapshot {
    angle_bits: u64,
    number: String,
    lit: usize,
    ring: usize,
    display_angle_bits: u64,
    boundary: String,
    view_number: String,
    view_lit: i32,
    view_ring: i32,
    view_stage: GaugeStage,
    view_theme: GaugeTheme,
    speed_bits: u64,
    timer_running: bool,
}

fn snapshot(ui: &SpeedGauge, handle: &GaugeHandle) -> Snapshot {
    Snapshot {
        angle_bits: handle.angle().to_bits(),
        number: handle.number_text(),
        lit: handle.lit_count(),
        ring: handle.ring_visible_count(),
        display_angle_bits: (ui.get_display_angle() as f64).to_bits(),
        boundary: ui.get_ring_boundary_commands().to_string(),
        view_number: ui.get_number_text().to_string(),
        view_lit: ui.get_lit_count(),
        view_ring: ui.get_ring_visible_count(),
        view_stage: ui.get_stage(),
        view_theme: ui.get_theme(),
        speed_bits: (ui.get_speed_kbps() as f64).to_bits(),
        timer_running: handle.timer_running(),
    }
}

#[test]
fn three_instances_keep_their_own_state() {
    let slot = headless_slot();
    let combos = [A, B, C];
    let gauges: Vec<(SpeedGauge, GaugeHandle)> = combos.iter().map(make).collect();

    // 各自驱动若干帧（动画泵与软件渲染器都在 headless 平台上跑）。
    for (_, handle) in &gauges {
        for _ in 0..12 {
            handle.step(1.0 / 60.0);
        }
    }

    let mut angles = Vec::new();
    let mut numbers = Vec::new();
    let mut lits = Vec::new();
    let mut rings = Vec::new();
    for (i, combo) in combos.iter().enumerate() {
        let (ui, handle) = &gauges[i];
        let (angle, number, lit, ring) = expected(combo);
        assert!(
            (handle.angle() - angle).abs() < 1e-6,
            "实例 {i}: angle={} 期望 {angle}",
            handle.angle()
        );
        assert_eq!(handle.number_text(), number, "实例 {i}: number_text");
        assert_eq!(handle.lit_count(), lit, "实例 {i}: lit_count");
        assert_eq!(
            handle.ring_visible_count(),
            ring,
            "实例 {i}: ring_visible_count"
        );
        // 视图属性也被推到位（控制器 → 组件属性这条链是通的）。
        assert_eq!(
            ui.get_number_text().to_string(),
            number,
            "实例 {i}: 视图 number-text"
        );
        assert_eq!(ui.get_lit_count() as usize, lit, "实例 {i}: 视图 lit-count");
        assert_eq!(
            ui.get_ring_visible_count() as usize,
            ring,
            "实例 {i}: 视图 ring-visible-count"
        );
        assert!(
            (ui.get_display_angle() as f64 - angle).abs() < 0.05,
            "实例 {i}: 视图 display-angle={}",
            ui.get_display_angle()
        );
        angles.push(handle.angle());
        numbers.push(handle.number_text());
        lits.push(handle.lit_count());
        rings.push(handle.ring_visible_count());
    }

    // 三个实例的四个量两两不同。
    assert_pairwise_distinct("angle", &angles);
    assert_pairwise_distinct("number_text", &numbers);
    assert_pairwise_distinct("lit_count", &lits);
    assert_pairwise_distinct("ring_visible_count", &rings);

    // 软件渲染器确实能画（传的是"平台句柄"，窗口适配器按组件身份解析，与创建顺序无关）。
    let (last_ui, _) = gauges.last().expect("至少一个实例");
    let shot = shots::render(last_ui, &slot, 286.0, 1.0).expect("headless 渲染");
    assert_eq!((shot.width, shot.height), (286, 286));
    assert!(
        shot.non_black_pixels() > 10_000,
        "渲染结果几乎空白：{} 个非黑像素",
        shot.non_black_pixels()
    );
}

/// 同一进程里两个**不同尺寸**的组件各自渲染到自己的尺寸（收尾修复 #2 的核心断言）。
///
/// 旧实现的 `WindowSlot` 只记"最近创建的那个窗口适配器"，因此第二个 `SpeedGauge::new()`
/// 会覆盖第一个的适配器，谁先谁后决定渲染跑在哪个窗口上。现在适配器按组件身份解析：
/// - `WindowSlot::for_component(&ui)` 只服务那一个组件（插错组件直接报错）；
/// - 交错创建、交错渲染，尺寸与画面互不串。
#[test]
fn two_components_render_at_their_own_sizes() {
    let slot = headless_slot();

    // 先建 A（286 见方），再建 B（矩形窗口）—— A 的适配器已经不是"最近创建的那个"了。
    let a = SpeedGauge::new().expect("A: SpeedGauge::new");
    let ha = GaugeHandle::attach(&a);
    ha.set_animation_enabled(false);
    ha.set_wobble_enabled(false);
    ha.set_stage(GaugeStage::Download);
    ha.snap_to(862_000.0);

    let b = SpeedGauge::new().expect("B: SpeedGauge::new");
    let hb = GaugeHandle::attach(&b);
    hb.set_animation_enabled(false);
    hb.set_wobble_enabled(false);
    hb.set_stage(GaugeStage::Ping);
    hb.snap_to(8_000.0);

    // 显式取各自实例的 slot（不可能靠顺序出错：绑错组件会返回错误）。
    let slot_a = WindowSlot::for_component(&a).expect("A 的窗口适配器");
    let slot_b = WindowSlot::for_component(&b).expect("B 的窗口适配器");

    let shot_a = shots::render_sized(&a, &slot_a, (286.0, 286.0), 1.0).expect("渲染 A");
    let shot_b = shots::render_sized(&b, &slot_b, (420.0, 300.0), 1.0).expect("渲染 B");
    assert_eq!(
        (shot_a.width, shot_a.height),
        (286, 286),
        "A 必须按自己的 286×286 出图"
    );
    assert_eq!(
        (shot_b.width, shot_b.height),
        (420, 300),
        "B 必须按自己的 420×300 出图"
    );
    assert!(
        shot_a.non_black_pixels() > 10_000 && shot_b.non_black_pixels() > 10_000,
        "两张图都该画出仪表：A={} B={}",
        shot_a.non_black_pixels(),
        shot_b.non_black_pixels()
    );

    // 再换一档：同一实例换尺寸 + DPR，只有它自己跟着变。
    let shot_b2 = shots::render_sized(&b, &slot_b, (286.0, 286.0), 2.0).expect("渲染 B@2x");
    assert_eq!(
        (shot_b2.width, shot_b2.height),
        (572, 572),
        "B 换尺寸 + DPR 后必须是 572×572"
    );
    let shot_a2 = shots::render_sized(&a, &slot_a, (286.0, 286.0), 1.0).expect("重渲染 A");
    assert_eq!(
        (shot_a2.width, shot_a2.height),
        (286, 286),
        "B 换尺寸不能影响 A"
    );

    // 交叉使用（把 B 的 slot 交给 A）必须被拒绝，而不是悄悄渲染到别的窗口上。
    assert!(
        shots::render_sized(&a, &slot_b, (286.0, 286.0), 1.0).is_err(),
        "把手 B 的 slot 传给 A 必须报错"
    );

    // 平台句柄（未绑定）同样按身份解析，且与 slot 参数混用没问题。
    let shot_a3 = shots::render(&a, &slot, 286.0, 1.0).expect("未绑定句柄渲染 A");
    assert_eq!((shot_a3.width, shot_a3.height), (286, 286));
}

#[test]
fn changing_one_instance_leaves_others_untouched() {
    let _slot = headless_slot();
    let (b_ui, b) = make(&B);
    let (a_ui, a) = make(&A);
    for _ in 0..8 {
        a.step(1.0 / 60.0);
        b.step(1.0 / 60.0);
    }
    let before = snapshot(&b_ui, &b);

    // 把 A 彻底改一遍：换阶段 / 主题 / 连接 / 刻度，精确 snap，再推 30 帧。
    // 注意不用 Upload/Idle：它们会挂归位 tween（见 `make()` 的注释）。
    a.set_stage(GaugeStage::Done);
    a.set_theme(GaugeTheme::Dark);
    a.set_connection_mode(ConnectionMode::Single);
    a.set_scale_mode(ScaleMode::Auto);
    a.snap_to(9_999_000.0);
    for _ in 0..30 {
        a.step(1.0 / 60.0);
    }

    let after = snapshot(&b_ui, &b);
    assert_eq!(before, after, "改 A 之后 B 的状态变了");

    // A 自己确实变了（防止"两边都没动"这种假通过）。
    assert_eq!(a_ui.get_stage(), GaugeStage::Done);
    assert_eq!(a_ui.get_theme(), GaugeTheme::Dark);
    assert!(
        (a.angle() - f64::from_bits(before.angle_bits)).abs() > 1.0,
        "A 的角度没变：{} vs {}",
        a.angle(),
        f64::from_bits(before.angle_bits)
    );
}

#[test]
fn snap_to_is_instant_while_set_speed_needs_frames() {
    let _slot = headless_slot();
    let ui = SpeedGauge::new().expect("SpeedGauge::new");
    let handle = GaugeHandle::attach(&ui);
    handle.set_wobble_enabled(false);
    handle.set_stage(GaugeStage::Download);

    // 归零 → 精确 snap 到 862 Mbps。
    handle.snap_to(0.0);
    assert!(
        (handle.angle() - renderer::START_ANGLE).abs() < 1e-9,
        "归零后角度应为起始角 {}°：{}",
        renderer::START_ANGLE,
        handle.angle()
    );

    let combo = Combo {
        stage: GaugeStage::Download,
        connection: ConnectionMode::Multi,
        scale: ScaleMode::Fixed,
        theme: GaugeTheme::Dark,
        kbps: 862_000.0,
    };
    let (target_angle, _, _, _) = expected(&combo);
    handle.snap_to(combo.kbps);
    assert!(
        (handle.angle() - target_angle).abs() < 1e-9,
        "snap_to 必须即时：angle={} 期望 {target_angle}",
        handle.angle()
    );
    let snapped = handle.angle();
    handle.step(1.0 / 60.0);
    assert!(
        (handle.angle() - snapped).abs() < 1e-9,
        "snap_to 之后帧前后角度必须一致：{} vs {snapped}",
        handle.angle()
    );

    // set_speed_kbps 只是设目标：不推进就不动。
    let before = handle.angle();
    handle.set_speed_kbps(100_000.0);
    assert!(
        (handle.angle() - before).abs() < 1e-9,
        "set_speed_kbps 不应立刻改角度：{} vs {before}",
        handle.angle()
    );
    assert!(handle.timer_running(), "set_speed_kbps 应启动动画泵");
    assert_eq!(handle.number_text(), scale::format_speed(100_000.0));

    for _ in 0..8 {
        handle.step(1.0 / 60.0);
    }
    assert!(
        (handle.angle() - before).abs() > 1.0,
        "推进 8 帧后角度应已移动：{} vs {before}",
        handle.angle()
    );
    assert!(handle.angle() < before, "100 Mbps 的角度必须小于 862 Mbps");

    // 收敛到 100 Mbps 的精确角（标准刻度第 4 档，即 0°）。
    // 弹簧的收敛判据是 |angle-target| < 0.02°，所以这里给 0.05° 的容差。
    let (target, target_number, _, _) = expected(&Combo {
        kbps: 100_000.0,
        ..combo
    });
    for _ in 0..600 {
        if handle.is_settled() {
            break;
        }
        handle.step(1.0 / 60.0);
    }
    assert!(
        (handle.angle() - target).abs() < 0.05,
        "收敛角 {} 期望 {target}",
        handle.angle()
    );
    assert_eq!(handle.number_text(), target_number);
}
