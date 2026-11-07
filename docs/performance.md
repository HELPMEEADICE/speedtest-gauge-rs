# 性能、内存与体积实测

本文所有数字都是**本机实测**，不是估算；每张表都给出测量命令与采样口径。凡是算出来的
（结构体尺寸、按公式推的缓冲区大小）都会显式标注「计算值」，不与「实测值」混排。

- 最后更新：`2026-11-08`
- 被测提交：工作区当前 HEAD（crate 版本 `speed-gauge-slint 0.1.0`，未提交改动见 `git status`）

---

## §0 测量环境

| 项 | 值 |
|---|---|
| CPU | Intel Core i9-14900HX（24 核 / 32 线程，标称 2200 MHz） |
| 内存 | 31.7 GiB（`(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory`） |
| GPU 1 | NVIDIA GeForce RTX 5080 Laptop GPU，驱动 32.0.16.2114 |
| GPU 2 | Intel UHD Graphics，驱动 32.0.101.7092 |
| OS | Microsoft Windows 10 专业工作站版 `10.0.19045`（64 位） |
| 工具链 | `rustc 1.98.1 (48a229cea 2026-09-01) (Rev1, Built by MSYS2 project)` / `cargo 1.98.1 (797e8a9bc 2026-08-05)` |
| 依赖版本 | Slint `=1.18.1`、`tiny-skia 0.11.4`（`Cargo.toml`） |
| shell | PowerShell 7.6.9 |
| profile | `cargo build --release`（`Cargo.toml` 未自定义 `[profile.release]`，即 opt-level=3 / lto=false / codegen-units=16 / panic=unwind） |
| 显示 | 有窗口后端可用（winit + femtovg 成功开窗，`scale_factor=1.00`），GPU 帧时是**实测**而非方法说明 |

> 采集环境信息：
> ```powershell
> Get-CimInstance Win32_Processor | Select Name,NumberOfCores,NumberOfLogicalProcessors,MaxClockSpeed
> (Get-CimInstance Win32_OperatingSystem | Select Caption,Version,BuildNumber,OSArchitecture)
> rustc --version; cargo --version
> ```

---

## §1 Release 二进制体积

构建：`cargo build --release --examples --lib`（13.36 s，本机）。

| 产物 | 字节 | MiB | 说明 |
|---|---:|---:|---|
| `target/release/examples/demo.exe` | 30,072,485 | 28.68 | 完整 Demo（两个仪表实例 + 控制面板 + Tiny-skia 截图能力） |
| `target/release/examples/minimal.exe` | 29,536,412 | 28.17 | 最小集成示例（单实例） |
| `target/release/examples/shots.exe` | 29,567,778 | 28.20 | 无窗口截图 / 视觉回归工具 |
| `target/release/libspeed_gauge_slint.rlib` | 3,804,022 | 3.63 | 组件库本体（`cargo build --release --lib`） |

采集命令：

```powershell
cargo build --release --examples --lib
Get-ChildItem target\release\examples\demo.exe,target\release\examples\minimal.exe,`
              target\release\examples\shots.exe,target\release\libspeed_gauge_slint.rlib |
  ForEach-Object { "{0,-28} {1,10} B" -f $_.Name, $_.Length }
```

体积构成（要点，不是新数字）：

- 三个 exe 都在 28–29 MiB，差值 < 0.6 MiB ⇒ 绝大部分是 **Slint 运行时 + femtovg/OpenGL 后端 +
  winit + tiny-skia**，而非应用代码；库本体只有 3.63 MiB。
- 随包字体 184,764 B（`assets/fonts/montserrat.ttf` 176,404 + `.local-fonts/gaugemono.ttf` 8,360）
  以 `include_bytes!` 内联，占 exe 的约 0.6%。
- 没有动态链接 Chromium/WebView2：见 §7 的字面量扫描。

---

## §2 空闲态内存（窗口态，收敛后稳定 3 秒）

口径：真窗口 `demo.exe`，**不** autoplay；`GaugeHandle` 收敛后 `timer_running == false`，
先等 1.2 s 再开始采样，每 100 ms 一次取 40 个点，**丢弃前 10 个点**，用剩下 30 个点（≈2.9 s 墙钟）
统计。

| 指标 | 最小 | 最大 | 均值 | MiB（均值） |
|---|---:|---:|---:|---:|
| `WorkingSet64` | 155,729,920 | 155,729,920 | 155,729,920 | **148.52** |
| `PrivateMemorySize64` | 66,920,448 | 66,973,696 | 66,939,972 | **63.84** |

`WorkingSet64` 30 个采样点**完全不变**（方差 0）——空闲态没有分配抖动。

采样命令（PowerShell，逐字可复现）：

```powershell
$p = Start-Process .\target\release\examples\demo.exe -PassThru
Start-Sleep -Milliseconds 1200
$rows = for ($i=0; $i -lt 40; $i++) {
  $pr = Get-Process -Id $p.Id
  [pscustomobject]@{ ws=$pr.WorkingSet64; priv=$pr.PrivateMemorySize64; cpu=$pr.TotalProcessorTime.TotalSeconds }
  Start-Sleep -Milliseconds 100
}
Stop-Process -Id $p.Id -Force
$w = $rows[10..39]
$w | Measure-Object ws   -Minimum -Maximum -Average
$w | Measure-Object priv -Minimum -Maximum -Average
```

**无窗口口径**（`SPEED_GAUGE_HEADLESS=1 SPEED_GAUGE_STATIC=862`，软件渲染，出完图即退出；
20 ms 轮询峰值）：

| 进程 | Peak `WorkingSet64` | Peak `PrivateMemorySize64` | 进程 CPU 合计 |
|---|---:|---:|---:|
| `demo.exe`（2 个仪表实例 + 面板，1280×800 软件渲染） | 19,103,744 B（18.22 MiB） | 8,699,904 B（8.30 MiB） | 0.141 s |
| `minimal.exe`（1 个仪表实例） | 12,623,872 B（12.04 MiB） | 4,964,352 B（4.73 MiB） | 0.078 s |

> 无窗口口径比窗口口径小一个数量级（18 MiB vs 149 MiB）：差额几乎全是 winit 窗口 +
> OpenGL/femtovg 上下文 + 交换链，不是组件本身。

---

## §3 动画态内存（持续动画的 3 秒）

口径：`SPEED_GAUGE_AUTOPLAY=1`（`DemoSource` 跑完整序列 Idle→Ping→Download→Upload→Done），
等 1.0 s 后每 100 ms 采样 40 点，取后 30 点（≈2.9 s）。

| 指标 | 最小 | 最大（峰值） | 均值 | MiB（峰值） |
|---|---:|---:|---:|---:|
| `WorkingSet64` | 164,237,312 | 167,272,448 | 166,672,521 | **159.52** |
| `PrivateMemorySize64` | 75,698,176 | 78,475,264 | 78,025,387 | **74.84** |

与 §2 空闲态对比：

| 差值 | 字节 | MiB |
|---|---:|---:|
| Δ `WorkingSet64`（峰值 − 空闲） | 11,542,528 | **+11.01** |
| Δ `PrivateMemorySize64`（峰值 − 空闲） | 11,501,568 | **+10.97** |

动画期多出来的是每帧重建的 Slint 属性载荷（`number-text` 字符串、240 段 `ring-segments` 数组
的临时拷贝、指针 `Image` 上传缓冲），不是持续增长——采样序列里均值与峰值只差 3 MiB，
没有锯齿式爬升。

采样命令：把 §2 命令里的 `Start-Process` 前加 `$env:SPEED_GAUGE_AUTOPLAY="1"`，
`Start-Sleep -Milliseconds 1000`，其余相同。

---

## §4 CPU

`Get-Process ... | Select CPU`（= `TotalProcessorTime`，进程累计 CPU 秒）。百分比按**单核基准**
换算：`ΔCPU秒 / Δ墙钟秒 × 100%`（100% = 占满 1 个逻辑核）。

| 场景 | ΔCPU | Δ墙钟 | 单核占比 | 备注 |
|---|---:|---:|---:|---|
| 窗口空闲（§2 的 30 点窗口） | 0.016 s | 2.9 s | **0.55%** | 含采样本身与 Demo 的 100 ms 心跳 |
| 窗口动画（§3 的 30 点窗口） | 1.203 s | 2.9 s | **41.49%** | 60–300 fps 全窗口重绘 + 每帧 240 段路径重算 |
| `minimal.exe` 窗口（步进演示） | 0.219 s | 1.5 s | 14.6% | 单实例、小窗口 |
| `demo.exe` headless 一次性出图 | 0.141 s | 1.8 s（含 PNG 编码） | — | 无窗口、纯 CPU |

读法：

- **空闲 0.55% 里没有一帧重绘**。这 0.55% 是 Demo 自己的 100 ms 心跳（`refresh_stats` 每拍
  格式化 6 个字符串并写回 `.slint` 属性）+ `Get-Process` 采样开销；组件的动画定时器此时已
  `stop()`（见 §5 的 `timer_running = false` 证据）。
- **动画 41% 是"全窗口 1280×800 × ~60–300 fps"的代价**，其中包含 femtovg 每帧重新上传指针
  RGBA（DPR1、572 逻辑 px 时 462,400 B）与 223 段彩弧的重新三角化；不是单帧 CPU 开销。

---

## §5 帧耗时

分两栏：**① 软件渲染器**（无窗口，`tiny-skia` + Slint software renderer）与
**② GPU 后端**（winit + femtovg）。两者不可直接横向比较：① 是纯 CPU 光栅化 286…572 px 的正方形，
② 是 1280×789 全窗口 GPU 合成。

### ① 软件渲染器（`examples/shots`，release，n=10）

命令：

```powershell
.\target\release\examples\shots.exe       # 或 cargo run --release --example shots
```

| 逻辑尺寸 / DPR | 渲染 `draw_if_needed` 中位 | 最小 | 最大 | 指针生成 | 合计中位 |
|---|---:|---:|---:|---:|---:|
| 286×286 @ DPR 1 | 12.50 ms | 12.41 ms | 12.64 ms | 0.07 ms | **12.57 ms** |
| 572×572 @ DPR 2 | 46.82 ms | 46.33 ms | 47.28 ms | 0.33 ms | **47.05 ms** |

同一次运行还打印（稳态零分配的验收）：

```
光栅画布重建次数：预热后 1 → 10 帧后 1（稳态不增 = 零稳态分配）
```

即：指针光栅画布只在 DPR/尺寸变化时重建一次，10 帧之后仍是 1 次 —— 每帧只 `clear` + 重绘。
DPR 翻倍 ⇒ 像素数 ×4 ⇒ 渲染 3.75×（12.50 → 46.82 ms），与面积成正比，说明这条路是**纯填充率受限**。

### ② GPU 后端（winit + femtovg，实测）

环境：`scale_factor=1.00`，窗口 `1280×789`（请求 1280×800，Windows 扣掉边框后为 789）。

开关：`examples/demo.rs` 里的 `SPEED_GAUGE_FPS`（只在 Demo 里，组件库未改动）：

```powershell
# 动画态（DemoSource 全序列）
$env:SPEED_GAUGE_AUTOPLAY="1"; $env:SPEED_GAUGE_FPS="1"
.\target\release\examples\demo.exe

# 空闲态
$env:SPEED_GAUGE_FPS="1"; Remove-Item Env:SPEED_GAUGE_AUTOPLAY
.\target\release\examples\demo.exe
```

| 场景 | 帧数 / 采样窗口 | 均值 | p50 | p95 | p99 | 最小 | 最大 |
|---|---:|---:|---:|---:|---:|---:|---:|
| 动画态 | 813 / 3004 ms | 3.374 ms（296.3 fps） | 3.918 ms（255.3 fps） | **4.820 ms（207.5 fps）** | 8.279 ms | 0.012 ms | 21.398 ms |
| 空闲态 | 837 / 3000 ms | 3.287 ms（304.3 fps） | 3.903 ms（256.2 fps） | **4.518 ms（221.3 fps）** | 8.571 ms | 0.015 ms | 27.887 ms |

原始输出（动画态，逐字）：

```
[fps] 采样窗口 3004 ms：帧数 813（丢掉首帧），均值 3.374 ms（296.3 fps）p50 3.918 ms（255.3 fps）p95 4.820 ms（207.5 fps）p99 8.279 ms 最小 0.012 最大 21.398
```

**采样口径与局限（必读）**：`Window` 不暴露 paint 回调，所以探针是这样测的 —— 起一个 1 ms 的
`slint::Timer`，每次回调先 `window().request_redraw()` 再记录与上次回调的间隔（丢掉第 1 个样本）。
因此：

- 测得值 = `max(1 ms 定时器, 后端 render 耗时, vsync 节流)`，是**帧间隔的上界**，不是纯着色耗时；
- 本机后端**不被 vsync 卡住**（否则 p50 会钉在 16.7 ms）——实测 p50 3.9 ms 说明事件循环真的在
  ~256 fps 地出帧，即 1280×789 的一整帧（含 240 段彩弧 + 指针 RGBA 上传）**在 4.8 ms 内（p95）完成**；
- 「空闲态」这一栏不是"空闲时不重绘"的证据（探针自己在强制重绘），它测的是同一条渲染路径的
  吞吐上限；空闲不重绘的证据是 §4 的 0.55% CPU 与 §5① 的 `timer_running = false`。

### ① vs ② 一句话

同一套矢量内容：软件光栅化 286² 要 12.5 ms、572² 要 46.8 ms（≈21 fps）；GPU 把 1280×789
全窗口压到 p95 4.8 ms（≈208 fps）。差的是填充率与合成方式，不是几何复杂度。

---

## §6 静态资源占用

### 6.1 240 段彩弧的 `commands` 字符串

每段是一条 `arcPath`（`M x y A 130 130 0 0 1 x y`，含 0.28° overlap，末段被 `min(135°)` 截断），
由 `src/renderer.rs::RingTable::build` 预生成。按 `geometry::r3` 的取整与最短往返格式化规则
逐段复算（Python 复刻 `src/geometry.rs::polar/r3/arc_path`，与
`tests/gradient.rs:193` 的黄金串 `M 51.076 234.924 A 130 130 0 0 1 48.85 232.642` 一致）：

| 项 | 值 |
|---|---:|
| 段数 | 240 |
| **字符数据总字节（UTF-8）** | **11,181 B** |
| 平均每段 | 46.59 B |
| 最短 / 最长一段 | 39 B / 49 B |
| `String` 头部（64 位，24 B × 240） | 5,760 B |
| **一个变体的段表合计** | **16,941 B** |

### 6.2 四组变体各一份的常驻内存

| 组成 | 计算值 | 说明 |
|---|---:|---|
| 渐变停点表 `.rodata` | 1,792 B | 4 变体 × 28 条 × `(f64, [u8;3])` = 16 B/条（`src/gradient_data.rs`） |
| 段表（`LazyLock`，首次使用才建） | 67,764 B | 4 × 16,941 B（6.1 的字符数据 44,724 B + String 头 23,040 B） |
| **合计** | **≈ 69.6 KB（0.068 MiB）** | 与 §2 的 148.52 MiB 相比在 RSS 噪声之下，无法用 RSS 差值实测，故给计算值 |

段表是**进程级共享**（`static TABLES: LazyLock<[RingTable; 4]>`），不随实例数增长：
开两个仪表实例仍然是这 67,764 B。

### 6.3 指针光栅画布（DPR 1–4）

公式（`src/gauge.rs:373-396` + `src/needle_raster.rs`）：
`px = round(170 × (组件逻辑宽 / 286) × dpr)`，缓冲区 = `px² × 4` 字节 **× 2**（`Pixmap` 预乘画布 +
反预乘输出 `Vec<u8>`，两者都常驻，稳态不再重分配）。上限 `MAX_PX = 8192`。

| 逻辑宽 | DPR | 画布像素 | 单张 | 双缓冲常驻 |
|---:|---:|---|---:|---:|
| 286 | 1 | 170×170 | 115,600 B（112.9 KiB） | 231,200 B（0.22 MiB） |
| 286 | 2 | 340×340 | 462,400 B（451.6 KiB） | 924,800 B（0.88 MiB） |
| 286 | 3 | 510×510 | 1,040,400 B（1016.0 KiB） | 2,080,800 B（1.98 MiB） |
| 286 | 4 | 680×680 | 1,849,600 B（1806.2 KiB） | 3,699,200 B（3.53 MiB） |
| 572 | 1 | 340×340 | 462,400 B（451.6 KiB） | 924,800 B（0.88 MiB） |
| 572 | 2 | 680×680 | 1,849,600 B（1806.2 KiB） | 3,699,200 B（3.53 MiB） |
| 572 | 3 | 1020×1020 | 4,161,600 B（4064.1 KiB） | 8,323,200 B（7.94 MiB） |
| 572 | 4 | 1360×1360 | 7,398,400 B（7225.0 KiB） | 14,796,800 B（14.11 MiB） |

（572 = Demo 主仪表在 800 高窗口下的逻辑边长 `800 × 0.715`。`comparison/render/*.png` 用的
1144×1144 @ DPR 4 就是上表「286 逻辑 × DPR 4」那一行的 4 倍面积版本。）

> 光栅化是**可选**的：关掉 `needle-raster` feature 就退回 32 片梯形平色路径，常驻画布归零，
> 代价是接缝（见 `docs/known-deviations.md` ⑥）。

### 6.4 每个 `GaugeHandle` 实例的额外开销

**（a）计算值**（Demo 主仪表 572 逻辑 px / DPR 1，feature `needle-raster` 打开）：

| 组成 | 大小 | 归属 |
|---|---:|---|
| 指针画布（`Pixmap` + 输出 `Vec`） | 924,800 B（0.88 MiB） | 每实例独占 |
| 组件侧 `ring-segments` 数组（240 × `String` + color） | ≈ 19 KiB | 每实例独占（`.slint` 属性副本） |
| 刻度 / 状态 / 运动状态 | ≈ 2 KiB | 每实例独占 |
| **合计** | **≈ 0.90 MiB / 实例** | —— |

Demo 的第二实例只有 200 逻辑 px：`round(170×200/286) = 119` ⇒ 119²×4×2 = 113,288 B
≈ **0.11 MiB**。

**（b）实测上界**（RSS 差值法，两个实例 vs 一个实例；两边都是无窗口软件渲染，避免 GPU 上下文干扰）：

| 进程 | 仪表实例数 | Peak WS | Peak PRIV |
|---|---:|---:|---:|
| `demo.exe` headless | 2（572 + 200 逻辑 px） | 18.22 MiB | 8.30 MiB |
| `minimal.exe` headless | 1 | 12.04 MiB | 4.73 MiB |
| 差值 | +1 | **+6.18 MiB** | **+3.57 MiB** |

**结论（这条要说清楚）**：`+3.57 MiB` 的私有内存差里，光 1280×800 的 RGBA 画布就是 4.10 MB
（`1280×800×4`），已经超过整个差值 —— 说明窗口/画布成本完全盖住了"多一个实例"的成本，
**RSS 差值法在本机分辨率下分辨不出 < 1 MiB 的实例开销**。因此本节以 (a) 的
**0.90 MiB / 实例（572 逻辑 px、DPR 1）** 为准，(b) 只用来证明上界 < 6.18 MiB。

---

## §7 结论：为什么这些数字是"原生"的量级

### 7.1 不引 Chromium / WebView

- `Cargo.toml` 的依赖只有 `slint`（`default-features = false`，只开 `std` / `compat-1-2` /
  `renderer-software` / `software-renderer-path`）+ 可选 `tiny-skia`；`desktop` feature 额外拉
  `slint/backend-winit` + `slint/renderer-femtovg`。`cargo tree`（626 行）在**当前目标平台**上
  对 `wry|tao|web-sys|wasm-bindgen|tokio|async-std|futures|pollster` 全部 0 命中。
  （`Cargo.lock` 里能看到 `web-sys` / `wasm-bindgen` / `tokio` 等名字，那是锁文件为其它目标平台
  与构建期收录的条目，`cargo tree` 不会把它们算进当前依赖图。）
- 对 `demo.exe`（30,072,485 B）做字面量扫描：

  | 模式 | 命中 |
  |---|---:|
  | `WebView2` / `webview2` | 0 |
  | `libcef` | 0 |
  | `Chromium` | 0 |
  | `chromium` | 1（`parley` 的换行规则表 `CHROMIUM_LINE_BREAK_TABLE`，是**断行数据**，不是浏览器引擎） |
  | `tokio` | 0（`cargo tree -i tokio` → "nothing to print"，即不在依赖图里） |
- 对照量级（**定性**，不编数字）：WebView 方案在 Linux 上要随包带一份 Chromium（典型量级是
  上百 MB、运行时常驻是数百 MB 起），Windows 上依赖系统 WebView2 运行时；本项目的
  **整个进程**空闲常驻 148.52 MiB `WorkingSet64`（其中 63.84 MiB 私有），在这个量级上，
  WebView 光是运行时内核就比它大一个数量级。**这不是同一类东西：这里没有 HTML/CSS/JS 层，
  没有渲染进程，也没有 IPC 边界。**

### 7.2 无 async runtime

- `Cargo.lock` 里出现 `tokio` / `async-executor` / `futures-executor` 只是因为锁文件收录了
  **其它目标平台/开发期**的条目；`cargo tree -i tokio` 打不出任何东西，说明当前目标平台的
  依赖图里没有它。`demo.exe` 里 `tokio` 字面量 0 命中。
- 所有动画都是 UI 线程上的 `slint::Timer` + 纯函数步进（`GaugeHandle::step(dt)`），
  没有 `async fn`、没有 `block_on`、没有后台线程。

### 7.3 动画停止后没有高频刷新

四条独立证据：

1. `GaugeHandle::timer_running()` 在收敛后为 `false`。`shots.exe` 的输出：
   ```
   启动后 timer_running = true
   收敛于 31 帧泵送后：angle=116.379° settled=true timer_running=false
   ✓ 收敛后 Timer 已停
   ```
   Demo 出图时也打印 `timer_running=false`（`[demo] main: ... timer_running=false`）。
2. 空闲 3 秒 `WorkingSet64` 30 个采样点**完全一致**（155,729,920 B，方差 0）⇒ 没有逐帧分配。
3. 空闲 3 秒 CPU 只涨 0.016 s（0.55%/单核），且这部分是 Demo 的 100 ms 心跳与采样开销。
4. 指针光栅画布重建次数稳态恒为 1（§5① ）⇒ 没有"每帧重建资源"的隐藏刷新。

换句话讲：**停止动画之后，这个进程是彻底静止的**（只有宿主自己的心跳），没有 rAF、没有
`requestAnimationFrame`、没有持续合成。

---

## §8 一键复现清单

```powershell
cd C:\oss\Project\Rust\speedtest-rs\slint

# 0) 构建
cargo build --release --examples --lib

# 1) 体积
Get-ChildItem target\release\examples\*.exe,target\release\libspeed_gauge_slint.rlib |
  Select-Object Name,Length

# 2) / 3) / 4) 空闲与动画的内存、CPU（把两段各跑一遍）
$env:SPEED_GAUGE_FPS=$null
$p = Start-Process .\target\release\examples\demo.exe -PassThru   # 动画态加 $env:SPEED_GAUGE_AUTOPLAY="1"
Start-Sleep -Milliseconds 1200
$rows = for ($i=0; $i -lt 40; $i++) {
  $pr = Get-Process -Id $p.Id
  [pscustomobject]@{ ws=$pr.WorkingSet64; priv=$pr.PrivateMemorySize64; cpu=$pr.TotalProcessorTime.TotalSeconds }
  Start-Sleep -Milliseconds 100
}
Stop-Process -Id $p.Id -Force
$rows[10..39] | Measure-Object ws,priv -Minimum -Maximum -Average
# CPU% = ($rows[39].cpu - $rows[10].cpu) / (29 * 0.1) * 100

# 5①) 软件渲染帧耗时（同时打印光栅画布重建次数）
.\target\release\examples\shots.exe

# 5②) GPU 帧间隔 p50/p95（3 秒后自动退出）
$env:SPEED_GAUGE_FPS="1"; .\target\release\examples\demo.exe
$env:SPEED_GAUGE_AUTOPLAY="1"; .\target\release\examples\demo.exe

# 6) 静态资源
#   6.1 段表字节数：按 src/geometry.rs 的 r3/polar/arc_path 复算 240 段（见 §6.1 的校验串）
#   6.3 画布尺寸：px = round(170 * logical/286 * dpr)，缓冲区 = px*px*4*2
```

---

## §9 未做/未测（诚实清单）

| 项 | 状态 | 原因 |
|---|---|---|
| 纯 GPU 着色耗时（不含事件循环） | 未测 | `slint::Window` 不暴露 paint 回调；只能给帧间隔上界（§5②） |
| 显存 / GPU 进程内存 | 未测 | 本机无稳定可脚本化的 per-process VRAM 计数器 |
| 长时间（分钟级）内存漂移 | 未测 | 本任务是 3 秒窗口口径；稳态零分配已由 `resizes` 与方差 0 佐证 |
| 在 DPR 2/3/4 的真窗口 GSync 帧率 | 未测 | 本机窗口 `scale_factor` 固定 1.00 |
| ARM / macOS / Linux | 未测 | 本机只有 x86_64 Windows |
