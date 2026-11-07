# 接真实测速后端

本文只讲一件事：怎么把**你自己的测速引擎**接到本 crate 的数据层上。渲染 / Slint 组件不在这里。

数据层只有三个类型（`src/data_source.rs`）：

| 原项目（`src/gauge/pipeline.ts`） | Rust 等价物 | 说明 |
|---|---|---|
| `SpeedSource` 接口 | `trait SpeedSource` | 契约见 §1 |
| `ManualSource` | `ManualSource` | 宿主直接 `push`，适合"我自己已经算好了"的宿主 |
| `WebSocketSource` | `ChannelSource` + `ChannelSender` | 原实现绑死浏览器 `WebSocket`，Rust 侧改为通道，见 §2 |
| `DemoSource` | `crate::demo_source::DemoSource` | 确定性 demo（`tick(dt_ms)` 显式步进），不接任何网络 |

---

## 1. `SpeedSource` 的契约

```rust
pub trait SpeedSource {
    fn name(&self) -> &'static str;            // "demo" / "manual" / "channel"
    fn start(&mut self, cb: SampleListener);   // 交出回调所有权，开始（或重开）一轮
    fn stop(&mut self);                        // 此后不得再回调
}

pub struct SpeedSample { pub kbps: f64, pub stage: Stage }
pub type SampleListener = Box<dyn FnMut(SpeedSample)>;
```

四条硬约定：

1. **单位恒为 kbps**，不是 Mbps，不是 B/s。`123456.0` = 123.456 Mbps。
2. **`start` 之后到 `stop` 之前**，source 按自己的节拍调用回调；重复 `start` = 先 stop 再 start
   （原实现 `pipeline.ts:59` 就是这样）。
3. **`stop` 之后一条都不许回调**。队列里剩下的样本可以丢弃（`ChannelSource` 会排空丢弃），
   但绝不能"补发"。
4. **`stage` 只从 `Stage` 枚举取值**（`idle/ping/download/upload/done`）。
   数据层不做任何视觉决策，也不 clamp `kbps`——clamp 在 `GaugeState::set_speed` 里。

回调在**调用 `push` / `poll` / `tick` 的那条线程**上执行，因此 `SampleListener` **不要求 `Send`**：
`Rc<RefCell<…>>` 这类非线程安全的捕获是合法的，方便直接改 UI 模型。

---

## 2. `ChannelSource`：引擎线程 → UI 线程

### 2.1 形状

```text
引擎线程（你自己写）                            UI 线程
  sender.send(kbps, stage)  ── mpsc 队列 ──►  source.poll()  ──►  listener(sample)
```

- `ChannelSender` 是 `Clone + Send`：`move` 进引擎线程即可，**不需要 `Arc<Mutex<source>>`**。
- `sender.send(...)` 返回 `false` ⇒ 接收端已销毁（source 被 drop 了），引擎线程应当收尾退出。
- `ChannelSource::poll()` 把队列里**所有**积压样本**按 FIFO 全量**喂给回调，返回本次转发条数。
  **不抽样、不合并、不保留最后一个**：指针动画（`crate::animation`）要靠连续样本做跟踪，
  抽样会在速度曲线上留下跳变台阶。
- `stop()` 之后 `poll()` 只排空、不回调，返回 0。

### 2.2 可编译示例（就是 `examples/real_backend.rs`）

```rust
use std::thread;
use std::time::{Duration, Instant};

use speed_gauge_slint::data_source::{ChannelSource, SpeedSource};
use speed_gauge_slint::model::Stage;

fn main() {
    // ① 建通道：sender 给引擎线程，source 留在 UI 线程。
    let (sender, mut source) = ChannelSource::channel();

    // ② 引擎线程：真实场景里这里是 HTTP/WS 客户端读循环。
    let engine = thread::spawn(move || {
        for beat in 0..13u32 {
            let kbps = 120_000.0 + f64::from(beat) * 41_500.0;
            let stage = if beat < 7 { Stage::Download } else { Stage::Upload };
            if !sender.send(kbps, stage) {
                break; // 对端已销毁
            }
            thread::sleep(Duration::from_millis(100));
        }
    });

    // ③ UI 线程：注册回调，每帧 / 定时器里 poll。
    source.start(Box::new(|sample| {
        println!("{:>10.1} kbps  stage={}", sample.kbps, sample.stage);
    }));

    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut seen = 0usize;
    while Instant::now() < deadline {
        seen += source.poll(); // 每帧一次，非阻塞
        if engine.is_finished() {
            seen += source.poll(); // 收尾再排空一次
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }

    source.stop();
    engine.join().expect("引擎线程 panic");
    assert!(seen >= 10);
}
```

跑法：`cargo run --example real_backend`（1.5 s 内打印 13 条样本后退出）。

### 2.3 换成真引擎（HTTP / WebSocket）

原 `WebSocketSource` 的 message handler 干了两件事：`JSON.parse` → `cb({kbps, stage})`。
Rust 侧把第一步放在**引擎线程**里，第二步就是 `sender.send(...)`：

```rust
use std::{thread, time::Duration};
use speed_gauge_slint::data_source::ChannelSource;
use speed_gauge_slint::model::Stage;

let (sender, source) = ChannelSource::channel();

thread::spawn(move || {
    // 伪代码：换成你常用的 HTTP/WS 客户端即可（ureq / reqwest(blocking) / tungstenite …）
    // for frame in my_client.stream() {
    //     let Ok(v) = serde_json::from_str::<serde_json::Value>(&frame) else { continue };
    //     let Some(kbps) = v["kbps"].as_f64() else { continue };
    //     let stage = v["stage"].as_str().and_then(Stage::parse).unwrap_or(Stage::Download);
    //     if !sender.send(kbps, stage) { break; }
    // }
    let _ = (&sender, Duration::from_millis(0));
});
```

三处与原实现一一对应：

| 原 `WebSocketSource` | Rust |
|---|---|
| `new WebSocket(url)` | 引擎线程里建客户端连接 |
| `JSON.parse(String(ev.data))`，脏帧忽略 | `serde_json` 解包失败就 `continue` |
| `data.stage ?? 'download'` | `Stage::parse(s).unwrap_or(Stage::Download)` |
| `typeof data.kbps === 'number'` 才转发 | `as_f64()` 取不到就 `continue` |

> 原实现不校验 `stage` 是否合法（`pipeline.ts:166` 直接透传）。Rust 侧 `Stage` 是枚举，
> 非法字符串在这里折成 `Download`——与"`setStage` 收到非法值只会走 download 系分支"同结果。

---

## 3. 别做的事

1. **不要在 UI 线程里做阻塞 IO。** `poll()` 必须是"看一眼队列就返回"的非阻塞操作。
   `TcpStream::read` / `HttpClient::get` / `join()` 一律放进引擎线程；UI 侧只留 `poll()`。
   一旦 UI 线程被网络调用卡住，指针动画（依赖每帧 `animation` 推进）会直接冻住，
   而这是本 crate 唯一"不可恢复"的体验故障。
2. **不要为了"异步"引入 tokio / async-std 运行时。** 数据层是纯同步的：
   引擎线程 `send`，UI 线程 `poll`。`mpsc` 已经提供了跨线程解耦，再套一层 executor 只会
   多出一个调度器、多一份 `Send + 'static` 约束、多一处 `block_on` 卡 UI 的机会。
   需要 Slint 侧的定时器就只用 `slint::Timer`（或宿主自己的帧回调），别把 future 塞进来。
3. 顺带：不要把 `ChannelSource` 塞进 `Arc<Mutex<…>>` 共享。它本来只需要被 UI 线程独占；
   `Mutex` 会把 `poll()` 里的回调调用变成"持锁执行用户代码"，很容易和 UI 的重入调用互锁。
   要跨线程就克隆 `ChannelSender`，那是唯一需要跨线程的端点。
