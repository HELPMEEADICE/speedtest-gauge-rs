//! `demo_source` / `data_source` 行为测试：黄金向量、可复现性、通道语义。
//!
//! 黄金文件由 `tools/gen_demo_golden.mjs` **独立**生成（不 import 原 TS，按公式重写一份），
//! 所以这里的逐条比对是"两份独立实现互相印证"，不是自己跟自己对答案。

use std::cell::RefCell;
use std::rc::Rc;
use std::thread;

use speed_gauge_slint::data_source::{
    ChannelSample, ChannelSource, ManualSource, SampleListener, SpeedSample, SpeedSource,
};
use speed_gauge_slint::demo_source::{DemoOptions, DemoSource};
use speed_gauge_slint::model::Stage;

/// 黄金文件相对 `CARGO_MANIFEST_DIR` 的路径。
const GOLDEN_JSON: &str = "tests/golden/demo_samples.json";
/// 逐条比对的绝对容差（任务口径）。
const TOLERANCE: f64 = 1e-9;

#[derive(Debug, Clone, Copy)]
struct GoldenSample {
    stage: Stage,
    kbps: f64,
}

/// 取 `key` 之后冒号右侧的剩余文本（本文件的 JSON 是"一个样本一行"的固定格式）。
fn field_body<'a>(line: &'a str, key: &str) -> &'a str {
    let start = line
        .find(key)
        .unwrap_or_else(|| panic!("缺少字段 {key}: {line}"))
        + key.len();
    line[start..]
        .trim_start()
        .strip_prefix(':')
        .unwrap_or_else(|| panic!("字段 {key} 后缺冒号: {line}"))
        .trim_start()
}

fn json_string_field(line: &str, key: &str) -> String {
    let body = field_body(line, key);
    let body = body
        .strip_prefix('"')
        .unwrap_or_else(|| panic!("字段 {key} 应以引号开头: {line}"));
    let end = body.find('"').expect("字符串字段未闭合");
    body[..end].to_string()
}

fn json_number_field(line: &str, key: &str) -> f64 {
    let body = field_body(line, key);
    let end = body
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
        .unwrap_or(body.len());
    body[..end]
        .parse::<f64>()
        .unwrap_or_else(|err| panic!("字段 {key} 不是数字 ({body}): {err}"))
}

/// 读黄金文件。JSON 是"一个样本一行"的固定格式，因此无需引入 serde。
fn load_golden() -> Vec<GoldenSample> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_JSON);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "读取黄金文件失败 {}: {err}；先跑 `node tools/gen_demo_golden.mjs`",
            path.display()
        )
    });
    let mut out = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if !line.starts_with('{') || !line.contains("\"stage\"") {
            continue;
        }
        let stage_text = json_string_field(line, "\"stage\"");
        let stage = Stage::parse(&stage_text)
            .unwrap_or_else(|| panic!("第 {} 行 stage 非法: {stage_text}", index + 1));
        out.push(GoldenSample {
            stage,
            kbps: json_number_field(line, "\"kbps\""),
        });
    }
    assert!(!out.is_empty(), "黄金文件里没有样本: {}", path.display());
    out
}

/// 构造一个把样本收进 `Vec` 的回调，并返回可读的另一半。
fn collector() -> (SampleListener, Rc<RefCell<Vec<SpeedSample>>>) {
    let cell: Rc<RefCell<Vec<SpeedSample>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&cell);
    (Box::new(move |sample| sink.borrow_mut().push(sample)), cell)
}

/// 取走已收集的样本（不动回调，因此 source 还活着也能取）。
fn take(cell: &Rc<RefCell<Vec<SpeedSample>>>) -> Vec<SpeedSample> {
    std::mem::take(&mut *cell.borrow_mut())
}

/// 用默认参数跑 `ticks` 次 `tick(dt_ms)`，返回收集到的样本序列。
fn run_demo(ticks: usize, dt_ms: f64) -> Vec<SpeedSample> {
    let (listener, cell) = collector();
    let mut source = DemoSource::new(DemoOptions::default());
    source.set_listener(listener);
    for _ in 0..ticks {
        source.tick(dt_ms);
    }
    take(&cell)
}

fn assert_matches_golden(actual: &[SpeedSample], golden: &[GoldenSample], limit: usize) {
    let n = limit.min(golden.len()).min(actual.len());
    assert!(n > 0, "没有可比较的样本");
    for (i, (got, want)) in actual.iter().zip(golden.iter()).take(n).enumerate() {
        assert_eq!(
            got.stage, want.stage,
            "第 {i} 条 stage 不一致: 实际 {} vs 期望 {}",
            got.stage, want.stage
        );
        let delta = (got.kbps - want.kbps).abs();
        assert!(
            delta < TOLERANCE,
            "第 {i} 条 kbps 超差: 实际 {:.17} vs 期望 {:.17}，差 {:.3e}",
            got.kbps,
            want.kbps,
            delta
        );
    }
}

#[test]
fn golden_file_covers_the_whole_run() {
    let golden = load_golden();
    // ping 1 + download 57 + upload 57 + done 1 = 116
    assert_eq!(golden.len(), 116, "黄金样本总数应为 116");
    let count = |stage: Stage| golden.iter().filter(|s| s.stage == stage).count();
    assert_eq!(count(Stage::Ping), 1);
    assert_eq!(count(Stage::Download), 57);
    assert_eq!(count(Stage::Upload), 57);
    assert_eq!(count(Stage::Done), 1);
    // 任务要求的"前 40 个"就是这份全序列的前缀。
    assert!(golden.len() >= 40);
    assert_eq!(golden[0].stage, Stage::Ping);
    assert_eq!(golden[0].kbps, 0.0);
    assert_eq!(golden[golden.len() - 1].stage, Stage::Done);
    assert_eq!(golden[golden.len() - 1].kbps, 862_000.0);
}

#[test]
fn tick_100ms_matches_golden_after_200_ticks() {
    let golden = load_golden();
    let actual = run_demo(200, 100.0);
    assert_eq!(
        actual.len(),
        golden.len(),
        "200 次 tick(100) 应产出与黄金文件等长的序列"
    );
    assert_matches_golden(&actual, &golden, actual.len());

    // 报告里贴的证据：前 5 条与后 5 条，以及全序列的最大绝对误差。
    for (i, (got, want)) in actual.iter().zip(golden.iter()).take(5).enumerate() {
        eprintln!(
            "[{i}] 实际 {:<8} {:<20.12} 期望 {:<20.12}",
            got.stage.as_str(),
            got.kbps,
            want.kbps
        );
    }
    let tail = actual.len() - 5;
    for (i, (got, want)) in actual.iter().zip(golden.iter()).enumerate().skip(tail) {
        eprintln!(
            "[{i}] 实际 {:<8} {:<20.12} 期望 {:<20.12}",
            got.stage.as_str(),
            got.kbps,
            want.kbps
        );
    }
    let max_delta = actual
        .iter()
        .zip(golden.iter())
        .map(|(got, want)| (got.kbps - want.kbps).abs())
        .fold(0.0_f64, f64::max);
    eprintln!("最大绝对误差 {max_delta:.3e} kbps（容差 {TOLERANCE:.0e}）");
}

#[test]
fn first_40_golden_samples_match() {
    let golden = load_golden();
    let actual = run_demo(200, 100.0);
    assert_matches_golden(&actual, &golden, 40);
}

#[test]
fn reset_reproduces_an_identical_run() {
    let first = run_demo(200, 100.0);

    let (listener, cell) = collector();
    let mut source = DemoSource::new(DemoOptions::default());
    source.set_listener(listener);
    for _ in 0..200 {
        source.tick(100.0);
    }
    assert!(take(&cell).len() > 100);

    source.reset();
    assert_eq!(source.stage(), Stage::Idle);
    assert_eq!(source.elapsed_ms(), 0.0);
    assert!(source.is_running(), "reset 之后应重新武装时间线");
    for _ in 0..200 {
        source.tick(100.0);
    }

    let second = take(&cell);
    assert_eq!(first.len(), second.len(), "两轮样本条数必须一致");
    for (i, (a, b)) in first.iter().zip(second.iter()).enumerate() {
        assert_eq!(a.stage, b.stage, "第 {i} 条 stage 不一致");
        assert_eq!(a.kbps, b.kbps, "第 {i} 条 kbps 必须逐位一致（可复现性）");
    }
}

#[test]
fn tick_partition_does_not_change_the_sequence() {
    let many = run_demo(200, 100.0);
    let one_shot = run_demo(1, 20_000.0);
    assert_eq!(many.len(), one_shot.len());
    for (i, (a, b)) in many.iter().zip(one_shot.iter()).enumerate() {
        assert_eq!(a.stage, b.stage, "第 {i} 条 stage 受 tick 分割影响");
        assert_eq!(a.kbps, b.kbps, "第 {i} 条 kbps 受 tick 分割影响");
    }
}

#[test]
fn done_sample_reads_download_peak_and_stops_running() {
    let samples = run_demo(200, 100.0);
    let last = *samples.last().expect("应有样本");
    assert_eq!(last.stage, Stage::Done);
    // 源码 finish() 用 dl * 1000（pipeline.ts:102），是"看起来像 bug"的真实行为。
    assert_eq!(last.kbps, 862_000.0);

    let mut source = DemoSource::new(DemoOptions::default());
    source.set_listener(Box::new(|_| {}));
    for _ in 0..200 {
        source.tick(100.0);
    }
    assert_eq!(source.stage(), Stage::Done);
    assert!(!source.is_running(), "done 之后不应再有待发事件");
    assert_eq!(source.tick(1000.0), 0, "done 之后不应再吐样本");
}

#[test]
fn stage_changes_land_on_the_source_timeline() {
    let phases: Rc<RefCell<Vec<Stage>>> = Rc::new(RefCell::new(Vec::new()));
    let first_download: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
    let mut source = DemoSource::new(DemoOptions::default());

    let phase_sink = Rc::clone(&phases);
    source.set_phase_listener(Box::new(move |stage| phase_sink.borrow_mut().push(stage)));
    let sample_sink = Rc::clone(&first_download);
    source.set_listener(Box::new(move |s| {
        if s.stage != Stage::Download {
            return;
        }
        let mut slot = sample_sink.borrow_mut();
        if slot.is_none() {
            *slot = Some(s.kbps);
        }
    }));
    assert_eq!(source.stage(), Stage::Idle);

    // t=1000：ping 事件在首次 tick 时到期，之后整段停在这里。
    for _ in 0..10 {
        source.tick(100.0);
    }
    assert_eq!(source.stage(), Stage::Ping);
    assert_eq!(source.elapsed_ms(), 1000.0);

    // t=1200：进 download，并立刻打第一个样本（段内 t_local=0 ⇒ 数值恰为 0）。
    source.tick(200.0);
    assert_eq!(source.stage(), Stage::Download);
    assert_eq!(*first_download.borrow(), Some(0.0));

    for _ in 0..150 {
        source.tick(100.0);
    }
    assert_eq!(source.elapsed_ms(), 16_200.0);
    // 源码 phase() 不去重：upload 由 download 段结束（t=6900）与 upload 段开始
    // （t=7520）各通知一次。
    assert_eq!(
        *phases.borrow(),
        vec![
            Stage::Ping,
            Stage::Download,
            Stage::Upload,
            Stage::Upload,
            Stage::Done
        ]
    );
}

#[test]
fn stop_freezes_the_timeline() {
    let (listener, cell) = collector();
    let mut source = DemoSource::new(DemoOptions::default());
    source.set_listener(listener);
    assert_eq!(source.tick(100.0), 1);
    source.stop();
    assert!(!source.is_running());
    assert_eq!(source.tick(100_000.0), 0, "stop 之后 tick 不得回调");
    assert_eq!(take(&cell).len(), 1);
}

#[test]
fn manual_source_pushes_through_the_listener() {
    let (listener, cell) = collector();
    let mut source = ManualSource::new();
    assert_eq!(source.name(), "manual");
    assert!(!source.is_started());

    // 未 start：静默丢弃（源码 `this.listener?.()`）。
    source.push(1.0, Stage::Download);
    assert!(cell.borrow().is_empty());

    source.start(listener);
    source.push(1234.5, Stage::Upload);
    source.push_download(-42.0); // 源码不 clamp
    source.stop();
    source.push(9.0, Stage::Done);

    let got = take(&cell);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0], SpeedSample::new(1234.5, Stage::Upload));
    assert_eq!(got[1], SpeedSample::new(-42.0, Stage::Download));
}

#[test]
fn channel_source_forwards_across_threads_in_order() {
    let (sender, mut source) = ChannelSource::channel();
    assert_eq!(source.name(), "channel");

    let (listener, cell) = collector();
    source.start(listener);

    let producer = thread::spawn(move || {
        assert!(sender.send(1_000.0, Stage::Download));
        assert!(sender.send_sample(ChannelSample::new(2_000.0, Stage::Download)));
        assert!(sender.send(3_000.0, Stage::Upload));
        // sender 在这里 drop；poll 仍应把队列排空。
    });
    producer.join().expect("生产者线程不应 panic");

    assert_eq!(source.poll(), 3);
    let got = take(&cell);
    assert_eq!(got.len(), 3, "必须按序拿到 3 条");
    assert_eq!(got[0], SpeedSample::new(1_000.0, Stage::Download));
    assert_eq!(got[1], SpeedSample::new(2_000.0, Stage::Download));
    assert_eq!(got[2], SpeedSample::new(3_000.0, Stage::Upload));

    assert_eq!(source.poll(), 0, "队列已空");
}

#[test]
fn channel_source_stops_calling_back_after_stop() {
    let (sender, mut source) = ChannelSource::channel();
    let (listener, cell) = collector();
    source.start(listener);

    assert!(sender.send(1.0, Stage::Download));
    assert_eq!(source.poll(), 1);
    assert_eq!(take(&cell).len(), 1);

    source.stop();
    assert!(sender.send(2.0, Stage::Download));
    assert!(sender.send(3.0, Stage::Upload));
    assert_eq!(source.poll(), 0, "stop 之后 poll 不再转发");
    assert!(take(&cell).is_empty(), "stop 之后回调次数不得增长");

    // 重新 start：新样本照常投递（stop 期间积压的样本已被丢弃）。
    let (listener, cell2) = collector();
    source.start(listener);
    assert!(sender.send(4.0, Stage::Upload));
    assert_eq!(source.poll(), 1);
    assert_eq!(take(&cell2), vec![SpeedSample::new(4.0, Stage::Upload)]);
}

#[test]
fn sources_are_object_safe_behind_box_dyn() {
    let (sender, channel) = ChannelSource::channel();
    let mut sources: Vec<Box<dyn SpeedSource>> = vec![
        Box::new(ManualSource::new()),
        Box::new(DemoSource::new(DemoOptions::default())),
        Box::new(channel),
    ];
    let names: Vec<&'static str> = sources.iter().map(|s| s.name()).collect();
    assert_eq!(names, vec!["manual", "demo", "channel"]);

    let (listener, cell) = collector();
    sources[2].start(listener);
    assert!(sender.send(777.0, Stage::Download));
    // 通过 trait 对象观察不到 poll（不在 trait 内），这里只验证 name/start/stop 契约。
    sources[2].stop();
    assert!(take(&cell).is_empty(), "trait 对象只验证契约，不驱动 poll");

    let (listener, demo_cell) = collector();
    sources[1].start(listener);
    sources[1].stop();
    assert!(take(&demo_cell).is_empty(), "stop 之后 demo 也不吐样本");
}
