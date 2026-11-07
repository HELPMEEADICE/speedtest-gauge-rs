# NOTICE

本文件是 `speed-gauge-slint`（Rust + Slint 移植）的来源与第三方资产声明。
许可正文见 [`LICENSE`](LICENSE)。

---

## 1. 原创作者与来源

| 项 | 内容 |
|---|---|
| 原始实现 | **speedtest-gauge-svg** —— <https://github.com/HELPMEEADICE/speedtest-gauge-svg> |
| 原创作者 | **HELPMEEADICE** |
| 原始技术栈 | TypeScript + SVG + CSS（浏览器内运行） |
| 原始许可 | **WTFPL v2** |

原始实现里的这些设计（286 uu 坐标系、±135° 环、240 段角向渐变、9/10 档非线性刻度、
tween/spring 双模型、速度驱动微颤、`idle→ping→download→upload→done` 阶段机）都是原作者的设计，
本移植只是把它们搬到 Rust + Slint 上；逐文件对照见 [`docs/module-map.md`](docs/module-map.md)。

## 2. 本移植

| 项 | 内容 |
|---|---|
| 包名 / crate | `speed-gauge-slint`（lib `speed_gauge_slint`），版本 `0.1.0`，`publish = false` |
| 许可 | **WTFPL v2**（沿用原项目，全文见 [`LICENSE`](LICENSE)） |
| 技术栈 | Rust 2021 + [Slint](https://slint.dev) `=1.18.1`（`default-features = false`）+ `tiny-skia 0.11.4`（可选） |
| 与原始实现的关系 | 数值层（几何/刻度/渐变/动画/数据管线）逐行或语义等价移植；视图层从 "SVG + CSS" 换成 "Slint 组件 + Rust 侧烘焙几何"，渲染手段的差异集中在 [`docs/known-deviations.md`](docs/known-deviations.md) |
| 无 WebView | 本包不引入 Chromium / WebView2 / CEF，也不引入 async runtime（依据见 [`docs/performance.md`](docs/performance.md) §7） |

原创作者没有再对本移植做背书，本移植的缺陷由本仓库负责。

## 3. 第三方字体

### 3.1 Montserrat —— 随包分发（SIL OFL 1.1）

| 项 | 值 |
|---|---|
| 文件 | [`assets/fonts/montserrat.ttf`](assets/fonts/montserrat.ttf)（176 404 B） |
| 字族 / 字重 | `Montserrat` / `Bold`（`usWeightClass = 700` 静态实例） |
| 许可 | **SIL Open Font License 1.1** —— 全文 [`assets/fonts/OFL.txt`](assets/fonts/OFL.txt) |
| 版权 | Copyright 2011 The Montserrat Project Authors（<https://github.com/JulietaUla/Montserrat>） |
| 完整说明 | [`assets/fonts/LICENSE-NOTICE.md`](assets/fonts/LICENSE-NOTICE.md) |

OFL 义务：分发时必须随附 `OFL.txt`；不得单独售卖字体本身；改名后的衍生字体不得使用保留字体名。
本包分发的是**未改名**的原始静态实例。

### 3.2 Gauge Mono —— 不随包分发（Ookla 专有）

`Gauge Mono` 是 **Ookla 的专有字体**，用于复刻中央数字的等宽效果。它**不在本仓库的可分发资产里**：

- 只有本机存在 `.local-fonts/gaugemono.ttf` 时才会被运行时注册（该目录被 `.gitignore` 排除），
  详见 [`.local-fonts/README.md`](.local-fonts/README.md)；
- 缺失时组件回退到系统等宽字体，中央数字与 `Mbps` 的墨迹宽度约 **+42.6%**
  （Consolas 对比实测，见 [`docs/font-metrics.md`](docs/font-metrics.md)）；
- **不得**把该字体放进发布包、安装器或任何可分发产物。

> 注：上游项目在本移植进行期间追加了 commit `9ed26d2 "assets: commit the gauge numeral font"`，
> 把同一份 `public/fonts/gaugemono.woff2` 提交进了**它自己的**仓库（WTFPL 覆盖其原创代码）。
> 这不改变 Ookla 对该字体的权利，因此**本移植仍然不随包分发它**，只保留上述"本机存在才注册"的机制。
> 我们移植所依据的 `src/gauge/*.ts`、`src/styles.css`、`index.html` 在该 commit 前后**逐字节未变**。

## 4. 第三方运行时

### 4.1 Slint —— GPLv3 / 商业双许可

本包依赖 [Slint](https://slint.dev) `1.18.1`。Slint 采用 **GPLv3 与商业许可的双许可**：

- 以 **GPLv3** 分发你的应用 ⇒ 需遵守 GPLv3 的开源义务；
- 闭源 / 商业分发 ⇒ 需要向 Slint 一方购买**商业许可**。

**该许可义务由使用本包的下游自行遵守**；本包的 WTFPL 只覆盖本包自己的代码，不能替下游解决
Slint 的许可问题。

### 4.2 tiny-skia

`tiny-skia 0.11.4`（feature `screenshots` / `needle-raster` 时启用）为 **BSD-3-Clause**，
随 Cargo 依赖分发，许可正文在 crates.io 包内。

## 5. 参考资产（不在本仓库）

`speedtest.net` 的名字、配色与仪表视觉属于 Ookla；本仓库只保存**度量结果**
（`comparison/ref-metrics/*.json`）与本地渲染图，不含原站截图资产。
`speedtest-gauge-svg` 原仓库里若有原站截图，其归属与再分发条件以原仓库为准。
