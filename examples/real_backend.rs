//! 用一条真实的测速线程驱动 [`ChannelSource`] 的最小可编译示例。
//!
//! 运行：
//! ```text
//! cargo run --example real_backend
//! ```
//! 预期：1.5s 内打印 13 条样本（每条间隔约 100ms）后正常退出。
//!
//! 这里用 `std::thread` + `sleep` 模拟一个"100ms 打点"的假引擎；换成真引擎时，
//! 把 `engine` 里的循环体替换成你自己的 HTTP / WebSocket 客户端读循环即可——
//! 唯一接口就是 `sender.send(kbps, stage)`。详见 `docs/real-backend.md`。

use std::thread;
use std::time::{Duration, Instant};

use speed_gauge_slint::data_source::{ChannelSource, SpeedSource};
use speed_gauge_slint::model::Stage;

/// 假引擎的打点节拍（ms）。
const BEAT_MS: u64 = 100;
/// 假引擎一共打几个点。
const BEATS: u32 = 13;

fn main() {
    // ① 建通道：sender 给引擎线程，source 留在 UI 线程。
    let (sender, mut source) = ChannelSource::channel();

    // ② 引擎线程：真实场景里这里是 HTTP/WS 客户端读循环。
    let engine = thread::spawn(move || {
        for beat in 0..BEATS {
            // 随便编一条"爬坡"曲线，只为演示单位是 kbps。
            let kbps = 120_000.0 + f64::from(beat) * 41_500.0;
            let stage = if beat < 7 {
                Stage::Download
            } else {
                Stage::Upload
            };
            // send 返回 false ⇒ 对端已销毁，收尾退出。
            if !sender.send(kbps, stage) {
                break;
            }
            thread::sleep(Duration::from_millis(BEAT_MS));
        }
    });

    // ③ UI 线程：注册回调，每帧/定时器里 poll。回调在本线程执行，不需要 Send。
    source.start(Box::new(|sample| {
        println!("{:>10.1} kbps  stage={}", sample.kbps, sample.stage);
    }));

    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut seen = 0usize;
    while Instant::now() < deadline {
        seen += source.poll();
        if engine.is_finished() {
            // 引擎收尾后可能还有几条积压在队列里，最后再排空一次。
            seen += source.poll();
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }

    source.stop();
    engine.join().expect("引擎线程 panic");

    println!("共收到 {seen} 条样本（name = {}）", source.name());
    assert!(seen >= 10, "1.5s 内至少应有 10 条样本，实际 {seen}");
}
