# 字体资产度量报告（speedtest-rs / Slint 移植）

> 全部数值为**实测**：woff2 用 fontTools 4.67.0 + brotli 1.2.1 转 ttf，度量用 Pillow 12.2.0 / FreeType。
> 复现脚本：`slint/tools/fontwork/fontwork.py`（`python slint\tools\fontwork\fontwork.py`）。
> 参考基准：原项目 `src/gauge/tokens.ts` 的 `GEO`（viewBox `0 0 286 286`、`numberSize: 36`、
> `numberWeight: 500`、`labelFontSize: 14`、`labelFontWeight: 700`）与 4× 截图（286 × 4 = **1144 px**）。

---

## 0. 结论速览

| 项 | 值 |
| --- | --- |
| 转换路线 | **a 成功**：`pip install --user brotli` 1.2.1 + `fontTools` 设 `flavor=None` 保存 |
| Gauge Mono | `slint\.local-fonts\gaugemono.ttf`，8 360 B（woff2 4 240 B），upm 2048，Regular 400 |
| Montserrat | `slint\assets\fonts\montserrat.ttf`，176 404 B，upm 1000，**Bold 700 静态实例** |
| 缺失字形 | 无（两款字体覆盖全部需要的字符） |
| 中央数字 Slint font-size | `36 × (画布宽px / 286)`；对齐 1144 px 参考图即 **144 px** |

---

## 1. 转换过程与来源

| 文件 | 源 | 大小 | 说明 |
| --- | --- | --- | --- |
| `slint\.local-fonts\gaugemono.ttf` | `C:\oss\Project\speedtest\public\fonts\gaugemono.woff2` | 8 360 B | 专有，仅本地校准 |
| `slint\assets\fonts\montserrat.ttf` | `C:\oss\Project\speedtest\public\fonts\montserrat.woff2` | 176 404 B | OFL 1.1，可再分发 |

- `research\gaugemono-regular-webfont-DwJfUwLI.woff2` 与 `public\fonts\gaugemono.woff2`
  **SHA256 完全相同**（`1A00AA81…00B8E04`），是同一份资产，只取其一。
- 转换方式：`TTFont(src)` → `f.flavor = None` → `f.save(dst)`，woff2 的 glyf 变换由 fontTools 反解，
  **不是**仅 brotli 解压，轮廓、hint（`fpgm`/`prep`/`cvt `）、`FFTM` 表均保留。

### 1.1 Montserrat 的重要发现：源文件是可变字体，默认实例是 Thin

源 woff2 带 `fvar` 表：单轴 `wght 100–900`，**默认值 100**，9 个命名实例（Thin…Black）。
直接转出的 ttf 是 `usWeightClass = 100 / "Montserrat Thin"`，而页面用 `font-weight: 700`
（`tokens.ts:25 labelFontWeight: 700`）——浏览器在可变字体上取 wght=700 实例渲染，
朴素转换会**得到 Thin，笔画与推进都按 Thin 计算**（推进值差异见 §3.1，数字普遍窄 4%–16%）。

因此落盘的 `montserrat.ttf` 不是原样转换，而是
`fontTools.varLib.instancer.instantiateVariableFont(f, {"wght": 700}, updateFontNames=True)`
生成的**静态 Bold 实例**（`fvar`/`gvar` 已移除，`usWeightClass=700`，`head.macStyle` 置 bold 位，
`OS/2.fsSelection` 置 bold 位，name 归一为 family `Montserrat` / subfamily `Bold`）。

需要其它字重时重跑同一段即可，例如标题用的 600：

```python
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
f = TTFont(r"…\montserrat-var.ttf"); instantiateVariableFont(f, {"wght": 600}, updateFontNames=True); f.save("montserrat-600.ttf")
```

**保持现状**：`montserrat.ttf` = wght 700 静态实例，与 `font-weight:700` 的刻度数字 1:1 对应。

---

## 2. Gauge Mono（专有资产，仅本地校准）

打开自检：`TTFont(path)` → family **Gauge Mono Regular**（nameID1）/ 排版族名（nameID16）**Gauge Mono**，
FreeType `getname()` → `('Gauge Mono', 'Regular')`，`unitsPerEm = 2048`。

| 字段 | 值 | 折算 em |
| --- | --- | --- |
| family / subfamily | `Gauge Mono Regular` / `Regular` | — |
| typographic family (16/17) | `Gauge Mono` / `Regular` | — |
| PostScript name | `GaugeMono-Regular` | — |
| unitsPerEm | 2048 | 1.0 |
| hhea ascender / descender / lineGap | 1568 / −552 / 0 | 0.765625 / −0.269531 / 0 |
| OS/2 sTypoAscender / Descender / LineGap | 1536 / −512 / 0 | 0.75 / −0.25 / 0 |
| OS/2 usWinAscent / usWinDescent | 1568 / 552 | 0.765625 / 0.269531 |
| OS/2 sCapHeight / sxHeight | 750 / 500 | 0.366211 / 0.244141 ⚠ |
| head yMin / yMax | −177 / 1568 | −0.086426 / 0.765625 |
| usWeightClass | 400（Regular，唯一字重） | — |
| usWidthClass | 5（Medium 宽度） | — |
| fsType | 4（Preview & Print 嵌入许可） | — |
| numGlyphs / cmap 覆盖 | 52 / 49 | — |
| `post.isFixedPitch` | 0（**名义非等宽**，但数字与标点实际同宽） | — |

⚠ `sCapHeight = 750` 与本字体数字的实际墨迹**不一致**：数字墨迹纵跨 y −31…1567
（高 1598 u = **0.780273 em**），即数字是「上顶 ascender、下略微出基线」的显示字形，
比 sCapHeight 高出约 2.1 倍。排版上不要用 sCapHeight 估数字高度，用 0.7803 em。

### 2.1 字宽（`hmtx` 精确值 + PIL @ size 36）

`getlength()` 在 36 px 下返回 FreeType 26.6 定点舍入后的整数值；`hmtx` 列为权威 em 基准，
Rust 侧无字体时用 `单位/2048` 估算。

| 字符/串 | hmtx advance (u) | 精确 em | PIL `getlength` @36 | px/36 |
| --- | --- | --- | --- | --- |
| `0`–`9`（每个） | 798 | **0.389648** | 14.0 | 0.388889 |
| `.` | 368 | **0.179688** | 6.0 | 0.166667 |
| `,` | 368 | **0.179688** | 6.0 | 0.166667 |
| `862.00` | 5×798 + 368 = 4358 | **2.127930** | 76.0 | 2.111111 |
| `4,200.00` | 6×798 + 2×368 = 5524 | **2.697266** | 96.0 | 2.666667 |
| `999.99` | 4358 | 2.127930 | 76.0 | 2.111111 |
| `1144` | 4×798 = 3192 | 1.558594 | 56.0 | 1.555556 |

**数字 0–9 全部等宽（798 u）**：`set(getlength(d) for d in "0123456789")` 只有一个元素 `{14.0}` ✓
（因此读数刷新时数字不会横向抖动）。

### 2.2 墨迹框 `getbbox` @36

| 串 | bbox (left, top, right, bottom) | 宽 × 高 px |
| --- | --- | --- |
| `862.00` | (0, 1, 76, 28) | 76 × 27 |
| `4,200.00` | (0, 1, 96, 31) | 96 × 30（逗号下探） |
| `0` | (0, 1, 14, 28) | 14 × 27 |
| `.` | (0, 26, 6, 28) | 6 × 2 |
| `,` | (0, 26, 6, 31) | 6 × 5 |

数字墨迹高 27 px / 36 px = 0.75（与 0.7803 em 对齐，差在定点舍入）。

### 2.3 字形覆盖

需要 `0123456789.,` → **缺失 0 个**，全部命中。
（轮廓互不相同，`0` 2 个轮廓、`8` 3 个轮廓、`1` 1 个轮廓，是真实字形而非占位方块。）

---

## 3. Montserrat（OFL 1.1，可再分发）

打开自检：`TTFont(path)` → family **Montserrat** / subfamily **Bold**，
FreeType `getname()` → `('Montserrat', 'Bold')`，`unitsPerEm = 1000`，`usWeightClass = 700`。

| 字段 | 值 | 折算 em |
| --- | --- | --- |
| family / subfamily | `Montserrat` / `Bold` | — |
| unitsPerEm | 1000 | 1.0 |
| hhea ascender / descender / lineGap | 968 / −251 / 0 | 0.968 / −0.251 / 0 |
| OS/2 sTypoAscender / Descender | 968 / −251 | 0.968 / −0.251 |
| OS/2 usWinAscent / usWinDescent | 1109 / 270 | 1.109 / 0.27 |
| OS/2 sCapHeight / sxHeight | 700 / 517 | 0.7 / 0.517 |
| head yMin / yMax | −266 / 1076 | −0.266 / 1.076 |
| OS/2 fsSelection / head.macStyle | 160（0xA0：bold + USE_TYPO_METRICS）/ 1（bold） | — |
| usWeightClass | **700（Bold，已实例化）** | — |
| fsType | 0（Installable，OFL） | — |
| numGlyphs / cmap 覆盖 | 1909 / 969 | — |

大写 `M` 实测墨迹 `(83, 0, 872, 700)` → 大写字高恰为 700 u = 0.7 em ✓（与 sCapHeight 一致）。

### 3.1 字宽 @ size 14（刻度数字 `font-size:14`、`font-weight:700`）

`hmtx` 列取自**落盘的 Bold 静态实例**（权威线性值）。`getlength @14` 是 FreeType **带 hinting**
的整数网格结果（该字体含 `fpgm`/`prep` 字节码，14 ppem 下每个字形推进被网格拟合），
因此 `px/14` 与精确 em 有 ±0.1 em 的量化偏差——Rust 侧无字体估算请用 `单位/1000` 那一列。

| 串/字符 | hmtx advance (u) | 精确 em | PIL `getlength` @14 | px/14（hinted） |
| --- | --- | --- | --- | --- |
| `100` | 392+679+679 = 1750 | **1.750** | 26.0 | 1.857143 |
| `1000` | 1750+679 = 2429 | **2.429** | 36.0 | 2.571429 |
| `Mbps` | 955+690+690+531 = 2866 | **2.866** | 41.0 | 2.928571 |
| `0` | 679 | 0.679 | 10.0 | 0.714286 |
| `1` | 392 | 0.392 | 6.0 | 0.428571 |
| `2` | 590 | 0.590 | 9.0 | 0.642857 |
| `3` | 592 | 0.592 | 9.0 | 0.642857 |
| `4` | 689 | 0.689 | 11.0 | 0.785714 |
| `5` | 595 | 0.595 | 9.0 | 0.642857 |
| `6` | 637 | 0.637 | 10.0 | 0.714286 |
| `7` | 620 | 0.620 | 10.0 | 0.714286 |
| `8` | 660 | 0.660 | 10.0 | 0.714286 |
| `9` | 637 | 0.637 | 10.0 | 0.714286 |
| `.` / `,` | 262 | 0.262 | 4.0 / 4.0 | 0.285714 |

参考：同一字体在**可变源默认轴（Thin 100）**下 `0`=652、`1`=342、`M`=956，
实例化到 700 后变为 679 / 392 / 955 —— 笔画加粗让数字推进普遍变宽 4%–16%，
这也是「直接转换会得到 Thin」必须被纠正的量化证据。

`getbbox("1000") @14 = (0, 4, 36, 14)` → 36 × 10 px；`getbbox("Mbps") @14 = (0, 3, 41, 17)` → 41 × 14 px
（`p` 下探到 y=17）。
@36 px：`"1000"` = 87.0 px、`"Mbps"` = 104.0 px。
数字**不是等宽**（392…689 u 混排）——刻度标签是「100 / 1000」这类左对齐短串，没问题；
但若要在 Slint 里做等宽数字滚动，需要逐个数字定宽。

### 3.2 字形覆盖

需要 `0123456789Mbps` → **缺失 0 个**，全部命中。

---

## 4. Slint 中央数字的 font-size 结论（与 4× 截图逐像素对齐）

### 4.1 换算链

原 SVG：`viewBox="0 0 286 286"`，中央数字 `font-size = 36`（**SVG 用户单位 uu**）。
参考截图宽 **1144 px**，即 `scale = 1144 / 286 = 4.0` 精确整数倍。

```
36 uu × 4 = 144 px（em 尺寸）
```

- SVG 的 `font-size` 与 Slint 的 `font-size` 语义相同 —— 都是**em 尺寸**（1 em = font-size），
  所以换算是纯线性缩放，不需要按 ascender/capHeight 做任何修正。
- **若 Slint 画布就是 1144 px（与参考图 1:1 对比）→ `font-size: 144px`。**
- 若 Slint 按 uu 设计尺寸铺满 286 逻辑 px → `font-size: 36px`。
- 通用公式：`font_size_px = 36 × (画布宽px ÷ 286) = 0.125874 × 画布宽px`。
- 缩放后其它量同步 ×4：数字 advance `14 px@36 → 56 px@144`；
  `"862.00"` 宽 `76.0 px@36 → 304 px@144`（精确 306.4 px，见下）。

### 4.2 横向：可用 advance 校验

`"862.00"` 精确 advance = 4358/2048 em = 2.12793 em：
- @ 36 uu → **76.6 uu**（`text-anchor: middle`，x=143 → 覆盖 104.7…181.3 uu，未溢出 286 视口 ✓）
- @ 144 px → **306.4 px**
`"4,200.00"` = 5524/2048 = 2.69727 em → 97.1 uu / 388.5 px。
Slint 侧用 `horizontal-alignment: center` + 定宽容器居中即可，两者都按 advance 居中，语义一致。

### 4.3 纵向：需要 0.1269 em 的补偿（实测）

SVG 用 `numberCenter = 226 uu` 作为**行盒中心**；Slint 的 `vertical-alignment: center` 也是按
字体行盒（ascent + descent）居中，但数字墨迹并不与行盒同中心：

| 量 | 计算 | @144 px |
| --- | --- | --- |
| ascent（hhea 1568/2048） | 0.765625 em | 110.25 px |
| descent（hhea 552/2048） | 0.269531 em | 38.81 px |
| 行盒高 | 1.035156 em | 149.06 px |
| 行盒中心（相对基线） | −0.248047 em | −35.72 px |
| 数字墨迹（yMax 1567 / yMin −31） | 上 0.765137 / 下 0.015137 em | +110.18 / −2.18 px |
| 墨迹中心（相对基线） | −0.375000 em | −54.00 px |

墨迹中心比行盒中心**高 0.126953 em ≈ 18.28 px @144**。
所以：把 Slint 文本元素的盒子中心放在 `numberCenter` 上时，数字会偏高，
需**整体下移 `0.127 × font-size`**（@144 px ≈ 18.3 px，@36 px ≈ 4.6 px）才是像素级对齐。

实测交叉验证（PIL，`getmetrics()` 给行盒、`getbbox("862.00")` 给墨迹）：

| size | ascent+descent | 行盒中心 | 墨迹中心 | 差（墨迹−行盒） |
| --- | --- | --- | --- | --- |
| 36 px | 28 + 10 = 38 | 19.0 | 14.5 | **−4.50 px = −0.125000 em** |
| 144 px | 111 + 39 = 150 | 75.0 | 56.5 | **−18.50 px = −0.128472 em** |

与理论值 0.126953 em 一致（差异来自 FreeType 整像素舍入）。
同一轮实测还确认：`getlength("862.00") @144 px = 306 px`、`getbbox = (0, 0, 306, 113)`，
与 4.1 的 4× 映射（36 uu → 144 px，76.6 uu → 306.4 px）吻合。

> 保守替代：不依赖行盒，直接把文本块顶边放在
> `baseline_y − 0.765625 × font-size`，其中 `baseline_y = numberCenter_y + 0.375 × font-size`。
> 两者等价，前者更好写（一个 `y` 偏移即可）。

### 4.4 字重

`tokens.ts` 声明中央数字 `font-weight: 500`，但 Gauge Mono **只有 400 一个字重**（`usWeightClass=400`，
52 个字形）。CSS 字重匹配规则下 500 会回退到唯一的 400 面且**不做合成加粗**（合成只在 ≥600 且无粗体面时触发），
所以网页上看到的笔画就是 400 的原样。Slint 侧用 Regular、**不要**用 Bold/合成加粗。

---

## 5. 分发边界（硬约束）

| 资产 | 可分发 | 依据 |
| --- | --- | --- |
| `slint\.local-fonts\gaugemono.ttf` | **否** | Ookla 专有仪表字体。仅允许本地校准渲染；已写入 `name` 表 nameID 0/10 = `LOCAL CALIBRATION ONLY - PROPRIETARY - DO NOT REDISTRIBUTE`，`fsType=4`（Preview & Print）。`.gitignore` 排除整个 `.local-fonts/`，随包发布前必须确认该目录不在产物中。 |
| `slint\assets\fonts\montserrat.ttf` | 是 | SIL Open Font License 1.1，全文见同目录 `OFL.txt`；版权 `Copyright 2011 The Montserrat Project Authors`。必须随附 `OFL.txt`。 |

公开构建（无 `.local-fonts/`）时中央数字会缺字体：回退到 `ui` 里声明的等宽族
（Consolas / ui-monospace），度量缺口见下表，仅影响观感不影响布局，因为 Slint 侧应按
**advance 比例**预留宽度而不是按字体家族。

| 回退字体（实测） | 数字 advance em | 与 Gauge Mono 0.389648 的偏差 |
| --- | --- | --- |
| Consolas @36 px | **0.555556 em**（20.0 px，10 个数字全同宽） | **+42.6%**（明显更宽，中央数字会撑大） |

> 结论：**公开发布版不要试图用系统等宽字体伪装 Gauge Mono**，宽度差异 42.6% 会破坏中央数字的对齐锚点；
> 应在无该字体时接受布局按 `Gauge Mono` 的 advance 比例（0.389648 em/数字）预留并保持居中。
