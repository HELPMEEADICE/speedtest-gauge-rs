# report/ 参考物料清单与复刻口径

- 源项目: `C:\oss\Project\speedtest`（只读）
- 本文所有像素尺寸均由 `python -c "from PIL import Image; Image.open(p).size"` 实测（Pillow 12 / Python 3.14）。
- 本文回答三件事：**每个 png 是什么状态**、**它由哪个脚本以什么参数产出**、**1144×1144 截图该怎样复刻**。

---

## 0. 一句话结论（先给口径）

| 问题 | 答案 |
|---|---|
| `report/local-*.png` 的像素尺寸 | 1144 × 1144（RGB，实测） |
| 其中仪表的 **CSS 尺寸** | **572 × 572 CSS px**（脚本把 `#gauge-stage` 钉成 `width: 572px`，`aspect-ratio:1/1` 推出高 572） |
| **DPR** | **2**（`deviceScaleFactor: 2`） |
| **viewBox → 像素缩放倍数** | viewBox 边长 286 uu → 572 CSS px ⇒ **2.0 CSS px / uu**；乘 DPR ⇒ **4.0 设备像素 / uu**（1144 / 286 = 4.0） |
| **URL 参数组合** | `?bare=1&static=<Mbps>&stage=<stage>`，扩展刻度再加 `&scale=auto`；无 `theme`（= dark）、无 `mode`（= multi） |
| 截图裁剪 | 就是 `#gauge-stage` 的 `boundingBox()` 原样，**不加 padding**（区别于 `shots/*.png` 的 `pad=8`） |
| 因此复刻口径 | 视口 ≥ 572×572 CSS、DPR=2、`bare=1`、`static`/`stage` 冻结、stage 元素 572×572、按元素边界截图 ⇒ 得到 1144×1144；图上任一像素 `p` 对应 `p / 4` 个用户单位 |

---

## 1. report/ 全部 png 总表（20 个文件）

| # | 文件名 | 像素尺寸 | 视觉状态 / 内容 | 产出脚本 | 关键参数 |
|---|---|---|---|---|---|
| 1 | `local-idle.png` | 1144×1144 | 本地仪表 **idle**：指针 −135°，圆环整段隐藏（sweep=0 < 0.05），数字 `0.00`，无刻度点亮 | `tools/artifacts.mjs:61-76` | `?bare=1&static=0&stage=idle`，572px，DPR2 |
| 2 | `local-low.png` | 1144×1144 | 本地 **低速下载 8 Mbps**：指针 −81.0°，数字 `8.00` | 同上 | `?bare=1&static=8&stage=download` |
| 3 | `local-mid.png` | 1144×1144 | 本地 **中速 155.61 Mbps**（对齐原站结果页数字串）：指针 12.5123°，数字 `155.61` | 同上 | `?bare=1&static=155.61&stage=download` |
| 4 | `local-high.png` | 1144×1144 | 本地 **下载峰值 862 Mbps**：指针 116.37°，数字 `862.00`，`0..750` 共 8 个刻度点亮 | 同上 | `?bare=1&static=862&stage=download` |
| 5 | `local-upload.png` | 1144×1144 | 本地 **上传 341 Mbps**：指针 46.035°，数字 `341.00`，图标切上传箭头，变体 `ul-multi`，accent = p3(0.671,0.38,0.91) | 同上 | `?bare=1&static=341&stage=upload` |
| 6 | `local-done.png` | 1144×1144 | 本地 **完成态**：与 `local-high.png` **逐字节相同**（sha256 前缀同为 `2977611231b9`）——`done` 与 `download` 的变体/强调色/指针完全一致 | 同上 | `?bare=1&static=862&stage=done` |
| 7 | `local-extended.png` | 1144×1144 | 本地 **扩展刻度 4200 Mbps**：切到 10 档 `[0,10,50,100,250,500,1000,2500,5000,10000]`，指针 95.4°（不 clamp），数字 `4,200.00` | 同上 | `?bare=1&static=4200&stage=download&scale=auto` |
| 8 | `overview-states.png` | 1196×832 | 6 态缩略拼版（idle / 8 / 155.61 / 862 / upload 341 / done） | `tools/compose.py:70-81` | tile=380, pad=14, cols=3, 顶部 30px 标题条 |
| 9 | `compare-ring.png` | 1440×420 | 原站贴图采样色条 vs 本地渲染采样色条（各 28 格） | `tools/compare.py:87-99` 产出 `shots/compare-ring.png`，再由 `tools/compose.py:68` 拷到 report/ | SHEET 720×210 → NEAREST ×2 |
| 10 | `compare-numbers.png` | 734×472 | 三行数字对比：① 原站结果页 gauge-mono 48px ② 本地 SVG `<text>`（36uu==48css px）③ 本地 HTML 同引擎对照 | `tools/compose.py:31-65` | 背景 (6,6,6)，行高 18+img+14，整体 NEAREST ×2 |
| 11 | `number-ref-48px.png` | 102×50 | 原站结果页数字墨迹框裁剪（gauge-mono 48 CSS px，DPR 1） | `tools/number-diff.mjs:42-45` | viewport 1440×1000, DPR **1**，clip = 文本 Range 矩形 |
| 12 | `number-local-48px.png` | 102×50 | 本地 SVG 数字墨迹框裁剪（把 stage 缩放到 36 uu == 48 CSS px） | `tools/number-diff.mjs:66-69` | viewport 900×900, DPR **1**，stage = 286*48/36 = **381.333px** |
| 13 | `number-html.png` | 102×48 | 同一字符串的纯 HTML `<div>` 孪生（同字体/48px/500/#fff on #000，`line-height:1`） | `tools/number-polarity.mjs:30-42` | viewport 900×900, DPR **1**，clip = div 矩形（无 padding） |
| 14 | `reference-number-dl.png` | 228×124 | 原站结果页下载数字 `155.61` 局部（gauge-mono 48px） | `tools/artifacts.mjs:37-44` | DPR2，pad **6** ⇒ (102+12)×2=228, (50+12)×2=124 |
| 15 | `reference-number-ul.png` | 228×124 | 原站结果页上传数字 `113.57` 局部 | 同上 | 同上 |
| 16 | `reference-live-go-state.png` | 2880×2000 | 原站**测速前**首页（GO 圆环态，含真实排版） | `tools/research.mjs:84` 产出 `research/shots/ref-idle-viewport.png`，手工拷入 report/ | viewport 1440×1000，DPR **2**，整视口截图 |
| 17 | `reference-live-dark.png` | 2880×2000 | 原站切暗色主题后的首页（纯黑底） | `tools/probe_dark.mjs:83` 产出 `research/shots/ref-dark-viewport.png` | 同上 |
| 18 | `reference-live-result-page.png` | 2880×2000 | 原站公开结果页 `/result/19869366452` | `tools/probe_result.mjs:56` 产出 `research/shots/ref-result-viewport.png` | 同上 |
| 19 | `reference-gradient-assets.png` | 2880×720 | 原站自带的 4 张 720×720 仪表渐变贴图并排（dl-multi / ul-multi / dl-single / ul-single） | `tools/sample_gradient.py:54-57` 产出 `research/shots/ref-gradient-assets.png` | 背景 (20,20,20)，4×720 = 2880 |
| 20 | `ui-full.png` | 2560×2000 | 本地**整页**截图（含控制面板与 readout），1280×1000 CSS @DPR2；四角实测 (0,0,0) | **当前 tools/ 下无任何脚本写出该文件名**（grep `ui-full` 无命中）⇒ 早先版本工具链的历史产物 | 视口 1280×1000，DPR 2 |

> #16–#19 的字节数与 `research/shots/` 下对应文件完全一致（131514 / 146178 / 459811 / 463071），判定为手工拷贝；`report/compare-ring.png` 与 `shots/compare-ring.png` 同为 7742 字节。
> 注：#19 的 4 张贴图源自 speedtest.net 的构建产物，**不随仓库分发**，README §7 明确其不在 WTFPL 覆盖范围内。

---

## 2. `local-*.png` 的产出规格（逐字来自 `tools/artifacts.mjs`）

### 2.1 浏览器上下文

```js
// artifacts.mjs:48
const ctx = await browser.newContext({ viewport: { width: 1200, height: 1000 }, deviceScaleFactor: 2 });
```
- 视口 **1200×1000 CSS**，`deviceScaleFactor: 2`。
- 整页 DPR 2 ⇒ 设备像素 = CSS × 2。

### 2.2 仪表尺寸被强制钉死

```js
// artifacts.mjs:65-67
document.getElementById('gauge-stage').style.width = '572px';
await page.waitForTimeout(320);
```
- `#gauge-stage` 的 CSS 声明是 `width: min(60vh, 440px); aspect-ratio: 1/1`（`src/styles.css:58-64`）；视口 1000 高 ⇒ `60vh = 600px`，`min(600,440) = 440px`，即**默认** 440×440。
- 脚本把 `width` 覆盖成 **572px**，`aspect-ratio: 1/1` ⇒ 高同样 **572px**。
- 注释原文（`artifacts.mjs:51`）说明了动机：*"sized so 1 svg unit == 2 css px (a 572 px stage), for crisp comparisons"* —— 572 / 286 = 2，正好整数倍。

### 2.3 截图裁剪

```js
// artifacts.mjs:69-74
const el  = await page.$('#gauge-stage');
const box = await el.boundingBox();
await page.screenshot({ path: ..., clip: { x: box.x, y: box.y, width: box.width, height: box.height } });
```
- `clip` = 元素外框**原样**，`pad = 0`。
- ⇒ 输出 = 572 × 572 CSS × DPR 2 = **1144 × 1144**（实测一致）。
- 由于 `.gauge-svg { overflow: visible }`（`src/styles.css:70`），超出 286 用户单位框的墨迹（刻度文字外沿、单位图标）也在裁剪区内，不会被切。

### 2.4 URL 参数 → 文件映射（`?static=<v>&stage=<s>` 冻结）

来源: `tools/artifacts.mjs:52-60`

| 文件 | `static` | `stage` | 其它 | 指针角 | 数字 |
|---|---|---|---|---|---|
| `local-idle.png` | `0` | `idle` | — | −135.000° | `0.00` |
| `local-low.png` | `8` | `download` | — | −81.000° | `8.00` |
| `local-mid.png` | `155.61` | `download` | — | 12.512° | `155.61` |
| `local-high.png` | `862` | `download` | — | 116.370° | `862.00` |
| `local-upload.png` | `341` | `upload` | — | 46.035° | `341.00` |
| `local-done.png` | `862` | `done` | — | 116.370° | `862.00` |
| `local-extended.png` | `4200` | `download` | `scale=auto` | 95.400°（10 档） | `4,200.00` |

全部带 `bare=1`（`artifacts.mjs:62` 的模板 `http://localhost:5178/?bare=1&${q}`）。完整 URL 示例：

```
http://localhost:5178/?bare=1&static=155.61&stage=download
http://localhost:5178/?bare=1&static=4200&stage=download&scale=auto
```

角度值由 §9 的映射算出并已与 README §4.4 的 116.37° 核对：
- `8`：`labelAngleAt` 档内插值 `t=(8−5)/5=0.6` ⇒ `−101.25 + 0.6×33.75 = −81.0`
- `155.61`：`t=(155.61−100)/150=0.3707333` ⇒ `0 + t×33.75 = 12.51225`
- `341`：`t=(341−250)/250=0.364` ⇒ `33.75 + t×33.75 = 46.035`
- `862`：`t=(862−750)/250=0.448` ⇒ `101.25 + t×33.75 = 116.37`
- `4200`（扩展 10 档，步长 30°）：`t=(4200−2500)/2500=0.68` ⇒ `75 + 0.68×30 = 95.4`

### 2.5 注意：`shots/*.png` 是**另一套**口径（912×912）

`tools/shoot.mjs:22,34-41`（8 个状态图，落 `shots/`，不进 `report/`）：

```js
viewport { width: 1440, height: 1000 }, deviceScaleFactor: 2
const el = await page.$('#gauge-stage');  const box = await el.boundingBox();
const pad = 8;
clip: { x: box.x - pad, y: box.y - pad, width: box.width + 16, height: box.height + 16 }
```
- 视口 1000 高 ⇒ `min(60vh,440) = 440` CSS ⇒ 舞台 **440×440**（未覆盖 width）。
- 裁剪 = 440 + 8×2 = **456 CSS** ⇒ ×2 = **912 设备像素**（实测 `shots/*.png` 全部 912×912）。
- 换算：1 用户单位 = `456/286 = 1.594` … 准确说是 **440/286 = 1.5385 CSS px = 3.0770 设备 px**（`compare.py` 里 `CSS=440, PAD=8, s=W/(CSS+2*PAD)=912/456=2`，`units-per-px = 440*2/286`）。
- `shots/` 的 8 个状态与 `report/local-*` **不同**：`shoot.mjs` 用的是 `mid static=150`（而 report 用 `155.61`），且多出 `gigabit static=1000`、`extended static=4200&scale=auto`，少了 `done`/`high` 之外的差异见下：

| shoot.mjs 的 id | query |
|---|---|
| `idle` | `static=0&stage=idle` |
| `low` | `static=8&stage=download` |
| `mid` | `static=150&stage=download` ← **150，不是 155.61** |
| `high` | `static=862&stage=download` |
| `upload` | `static=341&stage=upload` |
| `done` | `static=862&stage=done` |
| `gigabit` | `static=1000&stage=download`（report 里没有对应文件；`compare.py` 的环色/几何/指针校验就基于这一张） |
| `extended` | `static=4200&stage=download&scale=auto` |

⇒ **要复刻 `report/local-*.png` 就用 572px + 无 pad；要复刻 `shots/*.png` 或用 `compare.py` 做像素校验就用 440px + pad 8。** 两套不要混用。

---

## 3. 拼版规则

### 3.1 `overview-states.png`（1196×832）— `tools/compose.py:70-81`

```python
ids   = ["local-idle","local-low","local-mid","local-high","local-upload","local-done"]
tile, pad, cols = 380, 14, 3
rows  = ceil(6/3) = 2
canvas = RGB( cols*(tile+pad)+pad , rows*(tile+pad)+pad+30 ) = RGB(3*394+14, 2*394+14+30) = (1196, 832)
背景 (5,5,5)
标题：d.text((pad, 9), "local gauge - idle / 8 / 155.61 / 862 / upload 341 / done", fill=(160,160,160))
每格：report/local-*.png → convert("RGB") → resize((380,380), LANCZOS)
      → paste 到 ( pad + (i%3)*394 , 30 + pad + (i//3)*394 )
```
- 顺序 = idle, low(8), mid(155.61), high(862), upload(341), done(862)；`extended` **不在**总览里。
- 顶部 30px 是标题条；所有缩略图从 1144 缩到 380（LANCZOS），只用于人眼看状态，不可用于像素级比对。

### 3.2 `compare-ring.png`（1440×420）— `tools/compare.py:87-99`

```python
SHEET = RGB(720, 210) 背景 (0,0,0)
左标题 (10,6)  "reference: production gauge-gradient-dl-multi-web.png sampled along the ring"  fill(150,150,150)
右标题 (370,6) "local: shots/gigabit.png sampled the same way"                                  fill(150,150,150)
for i, (t, want, got, dE) in enumerate(28 个采样点):
    x0 = 10 + i*12.6
    左色块 rectangle([x0, 40, x0+12, 130], fill = 原站 hex)          ← 左栏起点 x=10
    右色块 rectangle([370+i*12.6, 40, 382+i*12.6, 130], fill = 本地 hex)  ← 右栏起点 x=370
脚注 (10,150)  "max dE {max:.0f} / mean dE {mean:.1f} over {n} interior samples"
最后 SHEET.resize((1440,420), NEAREST)  →  shots/compare-ring.png
```
- 色块尺寸 12×90 CSS（×2 后 24×180）；28 格 × 12.6 = 352.8，正好铺满 720 宽的一半。
- 采样点 = 原站贴图弧中点采样（`research/gradient-samples.json` 的 `arc[0..27]`），本地侧在 `shots/gigabit.png` 上按同一角度 `px_at(c, c, 130*upp, deg)` 取色。
- **首尾两点（t=0 / t=1）被排除**在 ΔE 统计外（`compare.py:50-52`），因为扇区边界天然有抗锯齿。README §4.1 的 `interior samples=26 / max ΔE 1.4 / mean ΔE 0.2` 就是 26 个内点的统计。
- `report/compare-ring.png` = `shots/compare-ring.png` 的拷贝。

### 3.3 `compare-numbers.png`（734×472）— `tools/compose.py:31-65`

```python
rows = [ ("reference  speedtest.net result page  gauge-mono 48px  (live crop, ink inverted)", "number-ref-48px.png"),
         ("local  SVG <text>  36 svg units == 48 css px  (this component)",                 "number-local-48px.png"),
         ("local  HTML <div>  same font / size / colour  (same-engine control)",            "number-html.png") ]
mask(f): 转 L；若左上角像素 > 128（浅底）则取反 → 统一成"黑底白墨"
W = max( max(textlength(label)) , max(img.width) + 40 )
H = sum(img.height) + 3*24 + 16                  = 50+50+48 + 72 + 16 = 236
canvas = RGB(W, H) 背景 (6,6,6)
y = 10
for label, img in rows:
    d.text((8, y), label, fill=(150,150,150));  y += 18
    paste(img, ((W - img.width)//2, y));        y += img.height + 14
resize((W*2, H*2), NEAREST)  →  734×472
```
- `W = 367`（由最长的那条标签文本宽度决定）⇒ 输出宽 734；`H = 236` ⇒ 输出高 472。
- 三行语义：**第 1 行是原站真图**、**第 2 行是本组件 SVG `<text>`**、**第 3 行是同引擎 HTML 对照**。README §4.3 说明第 1/2 行的笔画粗细差异来自"暗字亮底 vs 亮字暗底"的 sRGB 抗锯齿合成，而第 2/3 行逐像素同尺寸证明字形与排版完全一致。
- 前置依赖：必须先跑 `tools/number-diff.mjs`（#11/#12）与 `tools/number-polarity.mjs`（#13），否则 `compose.py` 会打印 `skipping compare-numbers.png` 并跳过这一段。

---

## 4. 复刻 1144 截图的完整配方（可直接照做）

```text
1) 构建/启动本地页面（源项目 npm run dev → http://localhost:5178/，或 Rust 侧等价服务）
2) 视口：width >= 572, height >= 572（原脚本 1200×1000；只要不小于 572×572 就不影响元素裁剪）
3) DPR：2
4) URL：?bare=1&static=<v>&stage=<s>[&scale=auto]
        v/s 取值见 §2.4 表；不带 theme（=dark）、不带 mode（=multi）
5) 等 body[data-ready="true"] 出现，再等 document.fonts.ready（Gauge Mono / Montserrat 必须已就绪，
   否则数字宽度与单位行居中都会偏）
6) 把 #gauge-stage 的 style.width 覆盖成 572px（高度由 aspect-ratio:1/1 自动 572px）
7) 等 ~320ms 让 layoutUnit() 重算一次（字体到达后会再触发一次 bbox 重新居中）
8) 截 #gauge-stage 的 boundingBox，clip 不加任何 padding
   ⇒ 1144×1144
```

坐标换算（写对拍脚本时用）：

```
设备像素 → 用户单位:  uu = px / 4.0
用户单位 → 设备像素:  px = uu * 4.0
CSS px  → 用户单位:  uu = css / 2.0
圆心 (143,143) uu  =  (572, 572) px（图正中）
环带半径 118.5..141.5 uu = 474..566 px
```

对照 `compare.py` 的口径（它吃的是 `shots/*.png`，**不是** `local-*.png`）：

```
CSS = 440, PAD = 8, VB = 286
s      = 图宽 / (CSS + 2*PAD) = 912/456 = 2.0
圆心   = PAD*s + (CSS*s)/2 = 16 + 440 = 456 px
像素/单位 = (CSS*s)/VB = 880/286 = 3.0769
```
若用 `local-*.png` 做同类校验，则应取 `s = 1144/572 = 2.0`、圆心 `= 572`、`像素/单位 = 572/286 = 4.0`、`PAD = 0`。

---

## 5. 与 Rust/Slint 对拍时的建议

1. **主口径用 1144×1144**：整数倍缩放（4 px/uu）没有亚像素模糊，最适合逐点断言；`shots/*.png` 的 3.077 px/uu 会引入 1/3 像素的抗锯齿差异，只适合"环宽/角度"这类容差比较。
2. 断言集建议直接取 `port-spec.md` 附录 A 的黄金向量 + 本文件 §2.4 的 7 个 `(static, stage) → 角度/数字`。
3. `local-done.png ≡ local-high.png` 可以当一条免费的回归断言：done 与 download 的渲染必须逐像素相同（含指针、圆环、数字），只有按钮高亮不同（而按钮在 `bare=1` 下被隐藏）。
4. `reference-live-*.png` 与 `reference-gradient-assets.png` 是**原站资产**，只能人工目检，不要放进 CI 的自动对拍（`reference-gradient-assets.png` 里的 4 张贴图不在仓库许可范围内）。
5. `ui-full.png` 无脚本可复现，建议在 CI 中直接排除，或按 §4 的口径用 1280×1000 CSS / DPR2 重新生成一份再纳入。
