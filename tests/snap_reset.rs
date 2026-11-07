//! `snap_to` 必须取消待执行的归位（收尾修复 #1），而 `set_speed_kbps` 必须保留源码 B12 的
//! "归位期间 set_target 不打断"行为。
//!
//! 背景：`reset()` / `set_stage(Upload|Idle)` 会挂一条 500 ms 归位 tween
//! （`springResetTween`），`NeedleMotion::step` 优先走它。旧实现里 `snap_to` 只写
//! `angle` / `target`，于是"归位途中 snap"会在后续帧被拽回 −135°。修复方式是给
//! `NeedleMotion` 加 `cancel_pending_return()`，由 `GaugeHandle::snap_to` 先调用它；
//! `set_target` / `reset` / `step` 的既有行为一点没动（下面的第二个测试就锁这条）。
//!
//! 平台是线程本地的（`i-slint-core::context::GLOBAL_CONTEXT` 是 `thread_local!`），所以每个测试
//! 线程各自装一次 headless 平台，装好的 `WindowSlot` 缓存在 thread-local 里。

use std::cell::RefCell;

use speed_gauge_slint::shots::{self, WindowSlot};
use speed_gauge_slint::{model, renderer, scale, GaugeHandle, GaugeStage, SpeedGauge};

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

/// 用公开纯函数算某个读数的目标角（标准刻度 / Fixed）。
fn angle_for(kbps: f64) -> f64 {
    let mbps = scale::kbps_to_mbps(kbps);
    let labels = model::scale_kind_for(model::ScaleMode::Fixed, mbps).labels();
    scale::angle_for_value(labels, mbps)
}

/// 建实例：关掉动画与微颤，角度纯粹由 `snap_to` / `step` 决定（1e-9 级断言的前提）。
fn make() -> (SpeedGauge, GaugeHandle) {
    let ui = SpeedGauge::new().expect("SpeedGauge::new");
    let handle = GaugeHandle::attach(&ui);
    handle.set_animation_enabled(false);
    handle.set_wobble_enabled(false);
    handle.set_stage(GaugeStage::Download);
    (ui, handle)
}

#[test]
fn snap_to_cancels_pending_return_tween() {
    let _slot = headless_slot();
    let (_ui, handle) = make();

    // 先摆到一个明显非起始角的位置（116.37° ≈ 862 Mbps）。
    handle.snap_to(862_000.0);
    let start = handle.angle();
    assert!(
        (start - angle_for(862_000.0)).abs() < 1e-9,
        "snap_to(862 Mbps) 应精确到位：{start}"
    );

    // `set_stage(Upload)` 触发 `motion.reset()`：挂一条 500 ms 归位 tween。
    handle.set_stage(GaugeStage::Upload);
    assert!(
        !handle.is_settled(),
        "set_stage(Upload) 之后应有待执行的归位动作"
    );

    // 只推进 2 帧：归位仍在进行中（指针已经被拉回去一点）。
    for _ in 0..2 {
        handle.step(1.0 / 60.0);
    }
    let during = handle.angle();
    assert!(
        during < start - 1.0,
        "归位应该已经把指针往回拉了：during={during} start={start}"
    );
    assert!(!handle.is_settled(), "2 帧后归位仍应进行中");

    // 关键动作：归位途中 snap 到 500 Mbps。
    let want = angle_for(500_000.0);
    handle.snap_to(500_000.0);
    assert!(
        (handle.angle() - want).abs() < 1e-9,
        "snap_to(500 Mbps) 必须即时且精确：{} vs {want}",
        handle.angle()
    );
    assert!(
        handle.is_settled(),
        "snap_to 之后不应再有待执行的归位（否则后续帧会把指针拽回 −135°）"
    );

    // 再推 30 帧（= 500 ms，足够跑完那条归位 tween）：角度必须原地不动。
    for _ in 0..30 {
        handle.step(1.0 / 60.0);
    }
    assert!(
        (handle.angle() - want).abs() < 1e-9,
        "30 帧后指针被归位 tween 拽走了：{} vs 期望 {want}",
        handle.angle()
    );

    // 读数与组件属性也停在 snap 的目标上。
    assert_eq!(handle.number_text(), scale::format_speed(500_000.0));
}

/// 源码 B12 的既有行为必须原样保留：归位进行中调 `set_speed_kbps`（内部走 `set_target`）
/// **不打断**归位，指针先把 500 ms 归位跑完再谈追踪。
#[test]
fn set_speed_kbps_still_waits_for_the_pending_return() {
    let _slot = headless_slot();
    let (_ui, handle) = make();

    handle.snap_to(862_000.0);
    handle.set_stage(GaugeStage::Upload); // 挂归位 tween（目标 = 起始角 −135°）
    for _ in 0..2 {
        handle.step(1.0 / 60.0);
    }

    // 归位途中给一个新目标：只改 target，不清 springResetTween（B12）。
    // 动画关掉时 set_speed_kbps 走 `snap` 分支，所以这一帧它就精确落在目标角上……
    handle.set_speed_kbps(500_000.0);
    let want = angle_for(500_000.0);
    assert!(
        (handle.angle() - want).abs() < 1e-9,
        "snap 级 set_speed_kbps 应当即时到位：{} vs {want}",
        handle.angle()
    );

    // ……但归位 tween 依然优先（`set_target` 不打断它）：接下来 28 帧把指针全程拉回
    // −135°（此前已推进 2 帧，合计 30 帧 = 500 ms）。这正是 `snap_to` 修复前的老行为：
    // `snap_to` 现在会先 `cancel_pending_return()`，所以它停得住；`set_speed_kbps` 保持
    // 原样 —— 它停不住。
    for _ in 0..28 {
        handle.step(1.0 / 60.0);
    }
    assert!(
        (handle.angle() - renderer::START_ANGLE).abs() < 1e-6,
        "归位 tween 必须先跑完（B12）：angle={} 期望 {}",
        handle.angle(),
        renderer::START_ANGLE
    );

    // 归位结束后才轮到追踪那个新目标（弹簧往 500 Mbps 的角度走）。
    for _ in 0..10 {
        handle.step(1.0 / 60.0);
    }
    assert!(
        handle.angle() > renderer::START_ANGLE + 1.0,
        "归位跑完之后应开始追踪 500 Mbps：angle={}",
        handle.angle()
    );
}
