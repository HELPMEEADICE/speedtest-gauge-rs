# comparison —— 视觉回归体系

这套目录是"Rust + Slint 端口 vs 参考截图 `report/local-*.png`"的**可复跑回归体系**。
一句话：几何对到 1 px、色差对到 1 级，边缘的 1–2 px 过渡带**不可能**逐像素相同——
所以这里的验收线是量化指标，不是"看起来一样"。

## 1. 目录结构

| 路径 | 是什么 | 谁生成 | 能否手改 |
|---|---|---|---|
| `render/` | 我们的 11 张暗色渲染（7 个对拍状态 + `gigabit` + 2 张指针剖面 + `minimal-headless` + `demo-ui`） | `cargo run --release --example shots` / `--example minimal` / `--example demo` | **不要手改**，重跑命令即可 |
| `render-light/` | 浅色主题 7 个状态的首个可视化基线，**无官方参考图** | `cargo run --release --example invariance` | 同上 |
| `ref-metrics/` | 从参考截图反解出来的锚点（环半径、sweep 终点、ink bbox、9 档刻度中心），6 个 JSON | `tools/measure_reference.py` | **只读基线**，别改 |
| `report/` | 报告产物：`pair-<state>.png` 四联图、`stats.json`、`summary.md`、`invariance.md`、`scales/*.png` | `tools/compare_report.py`、`tools/invariance.py`、`examples/invariance.rs` | 自动生成，别改 |
| `evidence/` | 指针实现方案迭代的证据（`raster/` 整条光栅化 vs `slices/` 32 片切片，含放大图与单帧耗时） | 历史实验留档 | 只读 |
| `minimal.log` / `minimal.err` | `examples/minimal.rs` 的运行日志 | 该 example | 自动生成 |

## 2. 一键复现

```powershell
cd slint

# ① 重建我们的渲染（会重写 render/ 里的状态图）
cargo run --release --example shots

# ② 重建尺寸/DPR 矩阵 + 浅色基线
cargo run --release --example invariance

# ③ 用既有仪器逐项对拍（几何 + 全图像素统计）
python tools/verify_states.py

# ④ 四联图 + stats.json + summary.md（幂等：两次运行字节一致）
python tools/compare_report.py

# ⑤ 尺寸/DPR 不变性报告
python tools/invariance.py

# ⑥ 绝对不变量集成测试（8 条，不依赖外部参考图目录）
cargo test --test visual
cargo test --all-targets          # 全量：111 passed
```

依赖：Python 3 + `numpy` + `Pillow`；Rust 工具链（`cargo fmt --check` / `clippy -D warnings` 均为 0 退出）。

## 3. 当前总体结论（7 个有参考的状态）

数据来自 `report/summary.md`（`python tools/compare_report.py` 生成，1144×1144，`uu = px/4`，
`|Δ|` = 三通道绝对差的最大值）。

| 状态 | mean\|Δ\| | max\|Δ\| | >8 像素 | >32 像素 | 完全一致 | 墨迹 IoU |
|---|---|---|---|---|---|---|
| idle | 0.444 | 241 | 0.939% | 0.500% | 98.228% | 0.9281 |
| low (8 Mbps) | 0.464 | 242 | 0.944% | 0.516% | 97.864% | 0.9637 |
| mid (155.61 Mbps) | 0.506 | 255 | 0.991% | 0.593% | 97.317% | 0.9788 |
| high (862 Mbps) | 0.611 | 255 | 1.083% | 0.699% | 96.676% | 0.9818 |
| upload (341 Mbps) | 0.575 | 255 | 1.066% | 0.655% | 97.035% | 0.9790 |
| done | 0.611 | 255 | 1.083% | 0.699% | 96.676% | 0.9818 |
| extended (4200, auto) | 0.668 | 255 | 1.215% | 0.767% | 96.534% | 0.9802 |

几何（同一个 `measure_reference` 尺子；环带另给能量积分口径）：

| 状态 | 环内 Δ (uu)¹ | 环外 Δ (uu)¹ | 环内 Δ (uu)² | 环外 Δ (uu)² | 环宽 Δ (uu)² | sweep Δ (°) | 数字 bbox 最大 Δ |
|---|---|---|---|---|---|---|---|
| idle | +0.50 | +0.15 | +0.295 | +0.104 | −0.191 | n/a | 1.00 px |
| low | +0.50 | +0.15 | +0.295 | +0.104 | −0.191 | +0.000 | 1.00 px |
| mid | +0.25 | +0.15 | +0.143 | +0.082 | −0.061 | +0.000 | 0.00 px |
| high | +0.00 | +0.15 | −0.004 | +0.060 | +0.064 | +0.000 | 1.00 px |
| upload | +0.25 | +0.15 | +0.144 | +0.090 | −0.054 | +0.050 | 1.00 px |
| done | +0.00 | +0.15 | −0.004 | +0.060 | +0.064 | +0.000 | 1.00 px |
| extended | +0.00 | +0.15 | −0.004 | +0.050 | +0.054 | +0.000 | 1.00 px |

¹ 阈值口径（`luma > 8` 的第一/最后一个半径）。它把抗锯齿裙边算进半径，而参考图与我们的
渲染的抗锯齿核不同，所以看起来差 0.5 uu——是尺子的分辨力，不是几何。
² 能量积分口径（`w = ∫L dr / L_peak`、`c = ∫rL dr / ∫L dr`），对任何归一化的抗锯齿核都精确：
**环带几何实际对到 0.3 uu 以内，点亮状态下对到 0.01 uu**。

分区 mean|Δ|（越靠左越干净；分区按 `docs/visual-targets.md` 的锚点几何划，允许重叠）：

| 状态 | 环带本体 118.5…141.5 | 指针区 r<85 | 刻度环 97±12 | 文字块 y∈[205,275] | 其余 |
|---|---|---|---|---|---|
| idle | 0.868 | 0.261 | 0.312 | 1.048 | 0.209 |
| low | 0.837 | 0.285 | 0.396 | 1.128 | 0.226 |
| mid | 0.758 | 0.240 | 0.735 | 1.132 | 0.281 |
| high | 0.661 | 0.390 | 1.141 | 1.345 | 0.343 |
| upload | 0.821 | 0.342 | 0.871 | 1.241 | 0.314 |
| done | 0.661 | 0.390 | 1.141 | 1.345 | 0.343 |
| extended | 0.655 | 0.416 | 1.406 | 1.423 | 0.358 |

**结论**：指针区、环带本体、背景已进 1 级色差（≤0.87）；几何全部对齐到 1 px 级
（环带 ≤0.3 uu、sweep ≤0.05°、数字 ink bbox ≤1.00 px）。仍有差的是**文字**（文字块 1.0–1.42、
刻度环 0.31–1.41）——差在字形边缘的抗锯齿过渡带，不是位置错位（bbox 已证明 ≤1 px）。
`high` 与 `done` 的渲染缓冲**逐字节相同**（与参考图 `local-done.png` = `local-high.png` 一致）。

## 4. 为什么不能逐像素完全相同（5 条原因）

1. **字体光栅器不同。** 参考图是 Chromium 用 FreeType 系栅格器 + 整数 hinting 出的字形；
   Slint 的软件渲染走 parley/fontdue 自己的一套栅格化。同一个 `Gauge Mono 36 uu` 的字形，
   两家在边缘像素上的覆盖率分布不同——这就是差异热力图（`report/pair-*.png` 右下角）
   上只剩"字形轮廓一圈红"、而内部全黑的直接原因。
2. **`dominant-baseline: central` vs `Slint` 的行盒居中。** SVG 把**em 盒**中心对到锚点
   `y=226`，数字字形只占其上半部，所以参考图的 ink 中心实测是 `221.38`（比锚点高 4.6 uu）。
   Slint 的 `Text` 在固定高度盒里居中同样是 em 盒居中，但"em 盒中心 → ink 中心"的换算
   取决于字体的 ascent/descent 比例，两家不必相同；刻度文字还额外需要
   `label-dy = -1.375` 的标定常量，标定后的残余是 0.13 uu（见 `report/summary.md` 的刻度列）。
3. **参考图自身的读数散布就有 0.5 uu。** 同一段环带在 `-60°` / `+60°` 两个方向量出来是
   118.30 / 118.55（`ref-metrics/*.json` 的 `ring_radius_uu` 可查），内半径在这一张图里
   就能差 0.25 uu，跨状态最多 0.5 uu。任何实现都不可能比"目标的测量噪声"更准，
   所以 ±0.4 uu 是硬下限而不是保守值。
4. **本机字体 advance 比参考宽 1.25 uu。** 单位行 `"Mbps"` 的推进宽度在本机的
   Montserrat 上比参考图宽 1.25 uu（`ui/speed-gauge.slint` 的 `unit-row-dx = -1.25` 就是
   为此设的标定常量）。横向累计到右边界后，单位行 ink 的右沿只能对到 ~1 px 级，
   文字块的分区色差因此天然高于纯几何区。
5. **抗锯齿算法不同。** 参考是 Skia/Chromium 的分析式覆盖率 + gamma 校正，
   Slint 软件渲染是另一套覆盖率规则。结果是边缘像素相差 1–3 个灰阶：
   这正是"完全一致 ≥96.5%、`>8` 只有 ~1%、但 `max|Δ|` 能到 255"的来源
   （`max|Δ|` 出现在字形/亮弧边缘的单个抗锯齿像素上，不是区域性的错）。

**因此验收线定在**：几何 ≤1 px、区域色差 ≤1 级、`>32` 像素占比 <1%、墨迹 IoU ≥0.92。

## 5. 浅色主题基线（`render-light/`）

- 7 个状态（idle / low / mid / high / upload / done / extended），口径同参考图：
  286 逻辑 px × DPR 4 = 1144×1144，只换 `theme = light`（背景 `#f2f2f2`）。
- **浅色没有官方参考截图，因此完全没有做过像素校准**——它只是可视化基线 + 可复现性断言
  （同输入两次运行字节一致，`tests/visual.rs::render_is_deterministic` 的暗色版同理）。
- 唯一做过的**解析**目标校验是指针渐变：沿指针轴 `d ∈ [2,84] uu` 采样，
  与 `255 × 0.6 × d/85` 的线性期望相比，**max|Δ| = 1.56/255**（阈值 4/255），
  即在浅色底上指针渐变仍然是解析正确的，只是没和截图对拍过。
- 换句话说：浅色的**几何与解析目标**已有保障，浅色的**配色/对比度**未经参考图验证。

## 6. 复跑纪律

- `report/` 下的 PNG 与 JSON **幂等**：不写时间戳、不用随机数，
  同一输入两次运行的字节完全一致（`pair-*.png` / `stats.json` / `summary.md` / `invariance.md`）。
- `report/scales/` 里的 7 张图是尺寸/DPR 矩阵的原始证据，
  由 `examples/invariance.rs` 渲染、由 `tools/invariance.py` 量测成 `invariance.md`。
- 参考图目录 `C:\oss\Project\speedtest` 只读，`comparison/ref-metrics/**` 与
  `comparison/render/` 里既有的状态图都不在本套工具的写路径上。
