# Speedtest Gauge — 移植规格（TypeScript + SVG → Rust + Slint）

- 源项目: `C:\oss\Project\speedtest`（只读基线）
- 目标: `C:\oss\Project\Rust\speedtest-rs\slint`
- 本文档所有常量与算法均逐条标注出处 `来源: 文件:行`。凡是"来源"只给行号的，均指源项目内的相对路径。
- 单位约定: SVG 用户单位 (uu)，`viewBox 0 0 286 286`；内部数据量纲 kbps，显示量纲 Mbps。
- 伪码风格为 Rust；`f64` 表示 JS number 的等价物（本组件所有数值运算都在 f64 域完成，没有 f32 中间量）。

---

## 0. 渲染口径（所有章节的前提）

| 项 | 值 | 来源 |
|---|---|---|
| 画布 | 单个 `<svg>`，`viewBox="0 0 286 286"`，`width=286 height=286`，`role="img"`，`aria-label="Speedtest gauge"`，class `gauge-svg` | `src/gauge/gauge.ts:135-142` |
| CSS 尺寸 | 由容器 `#gauge-stage` 决定：`width: min(60vh, 440px); aspect-ratio: 1/1`，svg 为 `width:100%; height:100%` | `src/styles.css:58-73` |
| 缩放 | 纯等比缩放；viewBox→CSS 的倍数 = `stage_css_px / 286`。没有 preserveAspectRatio 覆盖，也没有 transform 缩放 | `src/styles.css:58-73`, `src/gauge/gauge.ts:135-141` |
| 每帧写入 | 3 处：sweep `clipPath` 内 `<path d>`、指针 `<g transform>`、数字 `<text>` 文本 | `src/gauge/gauge.ts:435-460` |
| 圆弧描边 | `fill:none; stroke-width:23; stroke-linecap:butt` | `src/gauge/gauge.ts:172-180` |
| 元素 z 序（DOM 顺序） | defs → 轨道弧 path → ringGroup(g, clipPath) → needle(g) → labelGroup(g) → number(text) → unitGroup(g) | `src/gauge/gauge.ts:145-240` |

Rust/Slint 侧对应：`viewbox-width: 286px; viewbox-height: 286px;`（Slint 的 `Path`/`Flickable` 无关），整台仪表用一个 `Path`/`Rectangle` 容器 + 文本元素在 286×286 逻辑坐标里布点，容器宽度由外部决定（等价于 `#gauge-stage`）。

---

## 1. 几何常量表

### 1.1 GEO 全字段

| 字段 | 值 | 含义 | 来源 |
|---|---|---|---|
| `GEO.radius` | 130 | 圆弧中心半径（描边中线） | `src/gauge/tokens.ts:11` |
| `GEO.strokeWidth` | 23 | 圆弧描边宽度 | `src/gauge/tokens.ts:13` |
| `GEO.size` | 286 | 容器边长 = radius * 2.2 | `src/gauge/tokens.ts:15` |
| `GEO.center` | 143 | centerX = centerY = size/2 | `src/gauge/tokens.ts:17` |
| `GEO.startAngle` | -135 | 起始角（0° 朝上，顺时针为正） | `src/gauge/tokens.ts:19` |
| `GEO.endAngle` | 135 | 结束角 | `src/gauge/tokens.ts:20` |
| `GEO.labelRadius` | 97 | 刻度文字半径 = 130 - 33 | `src/gauge/tokens.ts:22` |
| `GEO.labelFontSize` | 14 | 刻度字号 | `src/gauge/tokens.ts:24` |
| `GEO.labelFontWeight` | 700 | 刻度字重（bold） | `src/gauge/tokens.ts:25` |
| `GEO.needleBase` | 20 | 指针根部宽 | `src/gauge/tokens.ts:27` |
| `GEO.needleTip` | 10 | 指针尖端宽 | `src/gauge/tokens.ts:28` |
| `GEO.needleLength` | 85 | 指针长度（自圆心向上） | `src/gauge/tokens.ts:29` |
| `GEO.numberSize` | 36 | 数字字号（text-4xl → 36px） | `src/gauge/tokens.ts:31` |
| `GEO.numberWeight` | 500 | 数字字重（font-medium） | `src/gauge/tokens.ts:32` |
| `GEO.unitSize` | 14 | 单位行字号（text-sm） | `src/gauge/tokens.ts:34` |
| `GEO.unitGap` | 4 | 图标与 "Mbps" 的水平间距 | `src/gauge/tokens.ts:35` |
| `GEO.unitIconSize` | 16 | 图标名义边长 | `src/gauge/tokens.ts:36` |
| `GEO.bottomInset` | 16 | 文字块底边距（bottom-4） | `src/gauge/tokens.ts:38` |
| `GEO.numberLineHeight` | 40 | 数字行高（2.5rem） | `src/gauge/tokens.ts:40` |
| `GEO.unitLineHeight` | 20 | 单位行行高（text-sm 20px） | `src/gauge/tokens.ts:41` |

Rust:
```rust
pub struct Geo {
    pub radius: f64, pub stroke_width: f64, pub size: f64, pub center: f64,
    pub start_angle: f64, pub end_angle: f64, pub label_radius: f64,
    pub label_font_size: f64, pub label_font_weight: u16,
    pub needle_base: f64, pub needle_tip: f64, pub needle_length: f64,
    pub number_size: f64, pub number_weight: u16,
    pub unit_size: f64, pub unit_gap: f64, pub unit_icon_size: f64,
    pub bottom_inset: f64, pub number_line_height: f64, pub unit_line_height: f64,
}
```

### 1.2 TEXT_BLOCK 推导（每个数字的算式）

来源: `src/gauge/tokens.ts:45-50`

```
bottom        = size - bottomInset                              = 286 - 16          = 270
unitCenter    = bottom - unitLineHeight / 2                     = 270 - 20/2        = 260
numberCenter  = unitCenter - unitLineHeight/2 - unitGap - numberLineHeight/2
                                                                = 260 - 10 - 4 - 20 = 226
```

对应 CSS 盒模型：一个 `absolute bottom-4 left-0 right-0 text-center` 的块，自下而上堆叠 `[数字行 40][gap 4][单位行 20]`，块底 = 270；两条基线中心分别 260 / 226。注册为锚点：

```rust
pub const TEXT_BLOCK_BOTTOM: f64 = 270.0;
pub const TEXT_BLOCK_UNIT_CENTER: f64 = 260.0;
pub const TEXT_BLOCK_NUMBER_CENTER: f64 = 226.0;
```

### 1.3 派生几何（非 GEO 字段，但由 GEO 唯一确定）

| 项 | 公式 | 值 |
|---|---|---|
| 轨道内外径 | `radius ± strokeWidth/2` | 118.5 … 141.5（环宽 23.0） |
| sweep 裁剪半径 | `radius + strokeWidth/2` | 141.5 |
| 指针根部 y | `center` | 143 |
| 指针尖端 y | `center - needleLength` | 58 |
| 指针根部 x 跨度 | `center ± needleBase/2` | 133 … 153 |
| 指针尖端 x 跨度 | `center ± needleTip/2` | 138 … 148 |
| 刻度角步长（9 档） | `(endAngle-startAngle)/(n-1) = 270/8` | 33.75° |
| 圆环段角宽（240 段） | `270/240` | 1.125° |

---

## 2. 极坐标与路径

来源: `src/gauge/geometry.ts`（全文 39 行）

### 2.1 polar

来源: `src/gauge/geometry.ts:11-14`

```
polar(cx, cy, r, angle_deg):
    rad = (angle_deg - 90) * PI / 180
    return { x: cx + r * cos(rad), y: cy + r * sin(rad) }
```

语义校验（SVG y 轴向下）：
- `angle=0` → rad=-90° → `(cx, cy-r)`：正上方。
- `angle=90` → rad=0 → `(cx+r, cy)`：正右方（顺时针）。
- `angle=-135` → rad=-225° → `(143-91.9239, 143+91.9239) = (51.076, 234.924)`：左下。
- `angle=135` → `(234.924, 234.924)`：右下。

### 2.2 r3（三位小数取整）

来源: `src/gauge/geometry.ts:8`

```
r3(v):
    return to_string(Math.round(v * 1000) / 1000)
```

必须逐字复刻的两个 JS 语义：
1. `Math.round(x)` 的定义是 `floor(x + 0.5)`（**向 +∞ 取整**），不是"四舍五入远离零"。
   - `Math.round(-2.5) === -2`；Rust 的 `f64::round(-2.5) === -3.0`。**不能直接用 `round()`**。
   - 等价实现: `let s = (x * 1000.0 + 0.5).floor(); s / 1000.0`
2. `Number.prototype.toString()` 输出最短往返表示，整数不带 `.0`，`-0` 打印为 `"0"`。
   - `51.076` → `"51.076"`；`143` → `"143"`；`234.924` → `"234.924"`。
   - Rust `format!("{}", 143.0_f64)` 得到 `"143"`（一致）；但 `format!("{}", -0.0_f64)` 得到 `"-0"`（不一致），需要在 `v == 0.0` 时显式返回 `"0"`。

```rust
fn r3(v: f64) -> String {
    let q = (v * 1000.0 + 0.5).floor() / 1000.0;
    if q == 0.0 { return "0".to_string(); }
    format!("{}", q)              // f64 Display = 最短往返表示
}
```

### 2.3 arcPath（描边弧）

来源: `src/gauge/geometry.ts:17-22`

```
arcPath(cx, cy, r, a0, a1):
    p0 = polar(cx, cy, r, a0)
    p1 = polar(cx, cy, r, a1)
    large = |a1 - a0| > 180 ? 1 : 0
    return "M {r3(p0.x)} {r3(p0.y)} A {r3(r)} {r3(r)} 0 {large} 1 {r3(p1.x)} {r3(p1.y)}"
```

- 分隔符是**单个空格**，没有逗号；`A` 之后顺序为 `rx ry x-axis-rotation large-arc-flag sweep-flag x y`。
- x 轴旋转角恒为 `0`；sweep 恒为 `1`（顺时针）。
- large-arc 判定用**未取整**的 `|a1-a0|`，严格 `> 180`（`==180` 走 0）。
- 轨道弧黄金串（必须逐字节相等，来源 README §4.4 + 上式）：
  `M 51.076 234.924 A 130 130 0 1 1 234.924 234.924`

### 2.4 sectorPath（扇形，用作 sweep 裁剪）

来源: `src/gauge/geometry.ts:25-35`

```
sectorPath(cx, cy, r, a0, a1):
    p0 = polar(cx, cy, r, a0)
    p1 = polar(cx, cy, r, a1)
    large = |a1 - a0| > 180 ? 1 : 0
    return ["M {r3(cx)} {r3(cy)}",
            "L {r3(p0.x)} {r3(p0.y)}",
            "A {r3(r)} {r3(r)} 0 {large} 1 {r3(p1.x)} {r3(p1.y)}",
            "Z"].join(" ")
```

- 顶点顺序：圆心 → a0 点 → 顺时针弧 → a1 点 → `Z`（回到圆心）。**子路径不自交**，因为全程 `|a1-a0| <= 270`。
- large-arc 与 arcPath 同一判定。

### 2.5 clamp

来源: `src/gauge/geometry.ts:37-39`

```
clamp(v, lo, hi) = v < lo ? lo : (v > hi ? hi : v)
```
（NaN 会原样穿透：`NaN < lo` 与 `NaN > hi` 均为 false → 返回 NaN。Rust 需保持同一行为，不要用 `f64::clamp`，后者对 NaN 返回 NaN 但 debug 下会 panic 于 lo>hi；这里 lo<hi 恒成立，可安全使用 `.clamp()`，但**必须保留 NaN 直通**。）

---

## 3. 圆环角向渐变构造

来源: `src/gauge/gauge.ts:357-379`（renderVariant），`src/gauge/gauge.ts:63-74`（sampleStops），`src/gauge/gauge.ts:50-61`（hexToRgb / lerpHex），`src/gauge/gauge.ts:128`（ringSegments 默认值）

### 3.1 参数

| 参数 | 值 | 来源 |
|---|---|---|
| `ringSegments` (n) | 240（`options.ringSegments ?? 240`） | `src/gauge/gauge.ts:128` |
| `overlap` | 0.28° | `src/gauge/gauge.ts:363` |
| `span` | `endAngle - startAngle` = 270 | `src/gauge/gauge.ts:362` |

### 3.2 每段的角度公式

```
for i in 0..n:
    a0 = startAngle + span * i / n
       = -135 + 1.125 * i
    a1 = min(endAngle, startAngle + span * (i + 1) / n + overlap)
       = min(135, -134.72 + 1.125 * (i + 1))        // -134.72 = startAngle + overlap = -135 + 0.28
    color_t = (i + 0.5) / n            // 段中心采样，i+0.5 而非 i
    stroke  = sampleStops(stops[variant], color_t)
    path    = arcPath(143, 143, 130, a0, a1)
    attrs   = { fill: none, stroke, stroke-width: 23, stroke-linecap: butt }
```

要点：
- `a1` 的 `min(135, ...)` 截断只在最后一段生效：`i=239` 时裸值为 `-134.72 + 270 = 135.28`，被截到 135；因此**末段角宽 = 1.125°（无 overlap 余量）**，其余段角宽 1.405°。
- `a1` 的 `+overlap` 让相邻段互相重叠 0.28°，从而掩盖 butt 端帽在半透明抗锯齿下露出的发丝状接缝（`src/gauge/gauge.ts:363` 注释原文 "hides hairline seams between butt-capped segments"）。若不做 overlap，240 条独立描边弧之间会出现 0.28° 级的可见暗线/亮点。
- 采样点 `t=(i+0.5)/n` 与 stop 表的 `t` 不同步，所以相邻段的颜色差是"连续渐变在 240 点上的采样"，不是"stop 表直接取第 i 项"。
- 首段 `t = 0.5/240 = 0.0020833`，末段 `t = 239.5/240 = 0.9979167`；弧的两个端点色永远不是表首/表尾色，但视觉上一致，因为表本身来自连续贴图采样。

### 3.3 sampleStops 分段线性插值

来源: `src/gauge/gauge.ts:63-74`、`src/gauge/gauge.ts:50-61`

```
sampleStops(stops, t) -> String:
    x = clamp(t, 0, 1)
    for i in 0 .. stops.len()-2:
        a = stops[i]; b = stops[i+1]
        if x >= a.t && x <= b.t:
            sp = b.t - a.t
            k  = (sp == 0) ? 0 : (x - a.t) / sp
            return lerpHex(a.hex, b.hex, k)
    return stops[stops.len()-1].hex        // 兜底：直接返回 hex 字符串
```

```
hexToRgb("#rrggbb"):
    v = parse_int(hex[1..], radix 16)
    return [(v >> 16) & 255, (v >> 8) & 255, v & 255]

lerpHex(a, b, k):
    ra = hexToRgb(a); rb = hexToRgb(b)
    return "rgb(" + Math.round(ra.r + (rb.r-ra.r)*k) + ","
                  + Math.round(ra.g + (rb.g-ra.g)*k) + ","
                  + Math.round(ra.b + (rb.b-ra.b)*k) + ")"
```

必须复刻的细节：
1. **逐通道 `Math.round` 后转字符串**，得到的是 8-bit 量化色；不是 `f64` 色。Rust 侧请直接产出 `(u8,u8,u8)`，再拼 `"rgb(r,g,b)"`（无空格、小写）。
2. 区间判定是 `x >= a.t && x <= b.t`，**首命中即返回**；相邻区间在端点 `t` 处都命中，先命中的是左边那一对 —— 因为 `k` 恰为 1 或 0，两边结果等价，不影响数值。
3. 兜底分支返回的是**原始 hex 字符串**而不是 `rgb()`，所以 `stroke` 属性类型在两种形态间切换。Rust 侧统一成 `(u8,u8,u8)` 即可，无需保留字符串形态差异（渲染等价）。
4. `Math.round` 同 §2.2 的 `floor(x+0.5)` 语义；对非负通道值两者一致，可直接用 `(v + 0.5).floor() as u8`，但注意 `255.5` → 256 的溢出（不会出现：`lerp` 的两端都在 0..255，最坏 `254 + (255-254)*1 = 255`，量化后 ≤255；为稳妥用 `min(255)`）。

### 3.4 变体选择

来源: `src/gauge/gauge.ts:344-348`

```
currentVariant():
    upload = (stage == 'upload')
    if connectionMode == 'single': return upload ? 'ul-single' : 'dl-single'
    return upload ? 'ul-multi' : 'dl-multi'
```
即 `done` / `download` / `ping` / `idle` 都是 download 系变体；只有 `upload` 切到 upload 系。

### 3.5 Rust 侧数据结构建议

不要在运行时每帧重建字符串。推荐**构建期生成 + 运行期只改可见集合**：

```rust
// 4 个变体 × 240 段，构建期一次性算出，编译进二进制
#[derive(Clone, Copy)]
pub struct RingSeg {
    pub a0: f32,          // 度
    pub a1: f32,          // 度（已含 overlap / 末段截断）
    pub rgb: (u8, u8, u8),
}
pub static RING: [[RingSeg; 240]; 4] = /* 由 tools 生成的 .rs，或 include! 一个生成文件 */;
```

Slint 侧两种落地方式，按可行性排序：

1. **推荐：不发 clip，改为按角度裁剪段表。** 因为 sweep 裁剪的几何等价形式是"只画 `a0 < arcAngle` 的那些段，并把跨越边界的段截到 `a1_eff = min(a1, arcAngle)`"（sector 半径 141.5 正好等于环外径，裁剪边界是覆盖整个环宽的径向直线，与截断 arc 完全等价）。于是每帧只需算 `visible = RING[variant].iter().take_while(|s| s.a0 < arc).count()` 并对边界段生成一条被截断的 `commands`。这避免了 Slint 没有 `clip-path: path()` 的限制。
2. 若坚持用 clip：Slint 支持 `clip: true` 的矩形/圆角裁剪，不支持任意路径裁剪，因此不可行；只能用方式 1。

每个段的 `commands` 建议构建期预生成为 `SharedString`（4×240=960 条），运行期只做 `visible` 计数 + 一条边界段的重新生成；也可以只预生成 240 条弧路径的"起点/终点"数值，用 `Path { commands }` 每帧拼一条边界串。

---

## 4. 扫掠裁剪（sweep clip）

来源: `src/gauge/gauge.ts:435-454`（paint）

```
paint(angle):
    arcAngle = clamp(angle, -135, 135)
    sweep    = arcAngle - (-135)                  // 0 .. 270
    if sweep < 0.05:
        sweepPath.d = ""
        ringGroup.display = "none"
    else:
        if ringGroup.display == "none": ringGroup.display = ""
        sweepPath.d = sectorPath(143, 143, 130 + 23/2 /* = 141.5 */, -135, arcAngle)

    needle.transform = "rotate({angle.toFixed(3)} 143 143)"   // 注意：用未 clamp 到 ±135 的 angle
    if !unitLaidOut: layoutUnit()
    if |lastPaintAngle - arcAngle| >= 0.02:
        lastPaintAngle = arcAngle
        updateLabels(arcAngle)
```

规则提炼（移植必须逐条对上）：
1. **裁剪半径 = 130 + 23/2 = 141.5**，不是 130，也不是 141.5±。这是"环外缘"。
2. **阈值 0.05°**：`sweep < 0.05` 时不仅清空 `d`，还把整个 ringGroup `display:none`（避免退化扇形）。`sweep == 0.05` 走绘制分支。
3. ringGroup 的 `display` 是"按需恢复"：仅在当前值为 `"none"` 时写 `""`（等价于一个布尔 `ring_visible`）。
4. **角度 clamp 分两层**：
   - `arcAngle = clamp(angle, -135, 135)` 只用于裁剪与刻度点亮；
   - 指针 `rotate` 用的是 `step()` 返回的 `angle`，其范围是 `clamp(angle+wobble, -137, 137)`（见 §10.6），所以指针可以超冲 2° 而裁剪不会越界。
5. `rotate(...)` 角度格式化为 `toFixed(3)` —— 固定三位小数（`-135.000`、`0.000`、`116.370`）。Rust: `format!("{:.3}", angle)`；注意 `-0.0f64` 会输出 `-0.000`，JS 的 `(-0).toFixed(3)` 也是 `"0.000"`，需要 `if angle == 0.0 { "0.000" }` 特判。
6. 刻度点亮只在 `arcAngle` 变化 ≥ 0.02° 时重算（节流），但**解锁状态本身是幂等的**，移植时也可以每帧全量计算，只要判定规则一致。
7. Slint 落地：用 §3.5 的"按角度裁剪段表"替代 clip；`ring_visible` 用 `if sweep < 0.05 { /* 不画 */ }` 直接控制。

---

## 5. 指针

来源: `src/gauge/gauge.ts:186-194`（多边形）、`145-169`（渐变 defs）、`src/gauge/tokens.ts:112-130`（两个主题的 stop）

### 5.1 多边形（4 顶点，用 GEO 表达）

```
P0 = (center - needleBase/2, center)               = (133, 143)
P1 = (center + needleBase/2, center)               = (153, 143)
P2 = (center + needleTip/2,  center - needleLength) = (148, 58)
P3 = (center - needleTip/2,  center - needleLength) = (138, 58)
d = "M {P0.x}, {P1.y}" ...
```

源字符串（逐字符，注意 `, ` 逗号+空格分隔）：

```
M 133, 143 L 153, 143 L 148, 58 L 138, 58 Z
```

实际代码拼接为：`M ${143-10}, ${143} L ${143+10}, ${143} L ${143+5}, ${58} L ${143-5}, ${58} Z`，`fill = url(#needle-<uid>)`。
（源码里数字直接由模板字符串插入，得到 `M 133, 143 L 153, 143 L 148, 58 L 138, 58 Z`。）

### 5.2 旋转圆心

`rotate(angle.toFixed(3) 143 143)`：绕 `(GEO.center, GEO.center)` 旋转，与多边形自身坐标系一致（多边形已在绝对坐标里定义，没有额外 translate）。

### 5.3 objbBox 纵向 linearGradient

来源: `src/gauge/gauge.ts:151-168`

```
<linearGradient x1="0" y1="0" x2="0" y2="1" gradientUnits="objectBoundingBox">
    <stop offset="{offset*100}%" stop-color="{color}" stop-opacity="{opacity}"/> ... 
</linearGradient>
```

`gradientUnits=objectBoundingBox` + `y1=0,y2=1` ⇒ 渐变沿指针**自身包围盒**由上到下：offset 0 = 尖端 y=58，offset 1 = 根部 y=143。`offset` 属性写成百分比字符串（`0%` / `58%` / `100%`）。

暗色主题（`THEME.dark.needle`，`src/gauge/tokens.ts:112-116`）：

| offset | color | opacity |
|---|---|---|
| 0 | `#ffffff` | 0.6 |
| 1 | `#ffffff` | 0 |

浅色主题（`THEME.light.needle`，`src/gauge/tokens.ts:125-129`）：

| offset | color | opacity |
|---|---|---|
| 0 | `#ffffff` | 1 |
| 0.58 | `#A4A4A4` | 1 |
| 1 | `#222222` | 1 |

Slint 落地：`Path` 的 `fill` 只接受纯色/渐变画刷，Slint 的 `@linear-gradient(angle, stops)` 是**元素包围盒**相对、支持多 stop 与 alpha：
```slint
background: @linear-gradient(180deg, #ffffff99 0%, #ffffff00 100%);   // 暗色
background: @linear-gradient(180deg, #ffffffff 0%, #a4a4a4ff 58%, #222222ff 100%); // 浅色
```
（Slint 的 `@linear-gradient` 角度以元素为单位、0deg = 向下；对齐 objbBox 时把渐变施加在指针自身的 Path 元素包围盒上即可，且路径包围盒 = 133..153 × 58..143。）

---

## 6. 刻度文字

来源: `src/gauge/gauge.ts:381-403`（buildLabels）、`418-425`（updateLabels）、`src/gauge/tokens.ts:21-25`

### 6.1 位置公式

```
for index, value in labels.enumerate():
    angle = labelAngleAt(index, labels.len())          // 见 §9.2
    rad   = (angle - 90) * PI / 180
    x     = center + labelRadius * cos(rad)            // = 143 + 97*cos(rad)
    y     = center + labelRadius * sin(rad)            // = 143 + 97*sin(rad)
```

即 `polar(143, 143, 97, angle)`。标准 9 档的实测锚点：`0 → (74.4, 211.6)`、`100 → (143, 46)`、`1000 → (211.6, 211.6)`（`143 ∓ 97*cos45° = 143 ∓ 68.589`；`143 - 97 = 46`）。

### 6.2 文本属性

| 属性 | 值 |
|---|---|
| `text-anchor` | `middle` |
| `dominant-baseline` | `middle` |
| `font-size` | `14`（`GEO.labelFontSize`） |
| `font-weight` | `700`（`GEO.labelFontWeight`） |
| `font-family` | `Montserrat, 'Segoe UI', system-ui, sans-serif` |
| `fill` | 熄灭 = `THEME.labelOff`；点亮 = `THEME.labelOn` |
| 文本 | `String(value)`（整数，无小数、无千分位），例：`0 5 10 50 100 250 500 750 1000` |

语义：`text-anchor=middle` 让锚点成为**水平中心**；`dominant-baseline=middle`（Chromium 实现等价于 `central` 的近似）让锚点成为**字面框垂直中心**。二者合起来 ⇒ `(x,y)` 是"文字视觉中心"。Slint 的 `Text` 默认以元素包围盒对齐，所以要按"中心对齐"摆放：容器尺寸由 `width/height` 或 `alignment: center` 控制，**不要**用基线偏移。

### 6.3 点亮判定

来源: `src/gauge/gauge.ts:418-425`

```
on = (value == 0) ? (angle > node.angle) : (angle >= node.angle)
```

- `angle` 是 `paint()` 里 clamp 到 `[-135,135]` 的 `arcAngle`。
- `value == 0` 用**严格大于**：静止在 -135° 时 `0` 刻度**不亮**（这是唯一一处 `>`）。
- 其余刻度用 `>=`：指针正好落在刻度角上时该刻度亮。
- 只在 `on != node.on` 时写 `fill`（去抖）。

### 6.4 点亮/熄灭颜色 + p3 → sRGB 回退表

来源: `src/gauge/tokens.ts:101-131`（THEME）、`56-64`（SRGB_FALLBACK）、`src/gauge/gauge.ts:43-48`（`color()`）

`color(v) = SUPPORTS_P3 ? v : (SRGB_FALLBACK[v] ?? v)`。

`SRGB_FALLBACK` **全部 7 条**（原样，注意键里是三位小数、无多余空格）：

| 键（display-p3 原值） | sRGB 回退 |
|---|---|
| `color(display-p3 1 1 1)` | `#ffffff` |
| `color(display-p3 0 0 0)` | `#000000` |
| `color(display-p3 0.399 0.399 0.399)` | `#666666` |
| `color(display-p3 0.635 0.635 0.635)` | `#a3a3a3` |
| `color(display-p3 0.886 0.886 0.886)` | `#e2e2e2` |
| `color(display-p3 0.000 0.773 0.749)` | `#00c9c0` |
| `color(display-p3 0.671 0.380 0.910)` | `#b75cf0` |

主题取值表：

| token | dark | light |
|---|---|---|
| `page` | `#000000` | `#f2f2f2` |
| `track` | `#26273b`（本身是 hex，不经回退） | `color(display-p3 0.886 0.886 0.886)` → `#e2e2e2` |
| `labelOn` | `color(display-p3 1 1 1)` → `#ffffff` | `color(display-p3 0 0 0)` → `#000000` |
| `labelOff` | `color(display-p3 0.399 0.399 0.399)` → `#666666` | `color(display-p3 0.635 0.635 0.635)` → `#a3a3a3` |
| `number` | `#ffffff` | `#000000` |
| `unit` | `#666666` | `#a3a3a3` |

来源: `src/gauge/tokens.ts:101-131`。

强调色（`ACCENT`，`src/gauge/tokens.ts:78-83`）：

| key | `p3` 原文 | `hex` 回退 | 用途 |
|---|---|---|---|
| `download` | `color(display-p3 0.000 0.773 0.749)` | `#00c9c0` | multi + download/其它所有非 upload 阶段 |
| `upload` | `color(display-p3 0.671 0.380 0.910)` | `#b75cf0` | multi + upload |
| `downloadSingle` | `#e56b4f` | `#e56b4f` | single + 非 upload |
| `uploadSingle` | `#7cdb2b` | `#7cdb2b` | single + upload |

**移植决策（必须记录）**：Slint 的 `#rrggbb` 走 sRGB。所以 Rust 侧**恒定使用 hex 回退列**，即等价于 `SUPPORTS_P3 == false` 的运行路径。这意味着在宽色域屏上与原站的观感会有轻微差别（原站在支持 p3 的浏览器里用 p3 值）。若要完全等价，可在 Slint 里用 `Color::from_argb_f32` 之外的路径不可行 —— 保持 sRGB 是唯一稳定选择。

---

## 7. 底部文字块

来源: `src/gauge/gauge.ts:201-240`（构造）、`427-432`（layoutUnit）

### 7.1 数字

```
<text x="143" y="226"                  // (GEO.center, TEXT_BLOCK.numberCenter)
      text-anchor="middle" dominant-baseline="central"
      font-size="36" font-weight="500"
      font-family="'Gauge Mono', ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
      fill="{color(theme.number)}" class="gauge-number">0.00</text>
```

- 初始文本硬编码 `"0.00"`。
- `.gauge-number` 追加 `font-variant-numeric: tabular-nums; font-feature-settings: 'tnum' 1;`（`src/styles.css:75-78`）——**等宽数字**是像素级对齐的必要条件，Slint 侧需选等宽字体或显式启用 tnum。

### 7.2 单位行（图标 + 文字，作为 `<g class="gauge-unit">`）

结构（`src/gauge/gauge.ts:216-240`）：

```
<g class="gauge-unit" transform="translate(143, 260)">        // (center, TEXT_BLOCK.unitCenter)
    <g transform="translate(0, -8)">                          // 固定上移 8
        <path d="M6.6 0.6h2.8v6.2h3.1L8 12.4 3.5 6.8h3.1z" fill="{accent}"/>                 // download
        <path d="M6.6 15.4h2.8V9.2h3.1L8 3.6 3.5 9.2h3.1z" fill="{accent}" display="none"/>  // upload
    </g>
    <text x="20" y="0"                                        // x = unitIconSize + unitGap = 16 + 4
          dominant-baseline="central" font-size="14"
          font-family="Montserrat, 'Segoe UI', system-ui, sans-serif"
          fill="{color(theme.unit)}">Mbps</text>
</g>
```

必须逐字保留的常量：
- 两个图标 `d` 原样（一个 16×16 视角的箭头朝下、一个朝上；都是 `0..16` 框内的 path，`h/v` 相对指令）。
- 图标容器 `translate(0, -8)`：图标 path 的 y 范围是 0.6…12.4（down）/ 3.6…15.4（up），中心约 6.5/9.5；上移 8 让图标视觉中心对齐文字中心线（`y=0` 且 `dominant-baseline=central`）。
- 文字 `x = unitIconSize + unitGap = 20`，`y = 0`。
- 单位行整体 `translate(center, 260)`。

### 7.3 layoutUnit：用 bbox 重新水平居中，以及为什么需要

```
layoutUnit():
    bbox = unitGroup.getBBox()                       // 元素自身坐标系（不含自身 transform）
    x    = center - (bbox.x + bbox.width / 2)         // = 143 - (bbox.x + bbox.width/2)
    unitGroup.transform = "translate({x.toFixed(2)}, 260)"
    unitLaidOut = true
```

- `getBBox()` 返回的是**子内容在 `unitGroup` 自身用户坐标系里**的包围盒，不包含 `unitGroup` 自己的 `transform`。因此 `bbox` 形如 `x≈0`、`width ≈ 20 + textWidth`。
- 朴素写法 `translate(143, 260)` 会把**图标左边缘**放到 x=143，整行看起来右偏。用 bbox 反算得到 `x = 143 - (0 + w/2)`，即让"图标+间隙+文字"这一整行的**几何中心**落在 143，与上方数字（`text-anchor=middle` 于 143）严格同轴。
- 必须重算的时机：
  1. `paint()` 里首次（`!unitLaidOut` → `layoutUnit()`），来源 `src/gauge/gauge.ts:449`；
  2. `document.fonts.ready` 之后（`src/gauge/gauge.ts:247-249`）——Web 字体到达会改变文字宽度，`w` 变则 `x` 变。
- 输出格式 `toFixed(2)`（两位小数）。Rust 需实现一次"文本宽度测量"：`Mbps` @14px Montserrat。若 Slint 侧无法测得稳定宽度，可以把测量值固化为常量表（14px / weight 400 / Montserrat 的 `"Mbps"` 前进宽度），并在文档里记录该常量的来源为实测。
  已知事实：`Mbps` 在 14px Montserrat 下的前进宽度是移植平台相关的，**没有任何源文件把它写成常量**；原始实现是运行时测量。这是移植中唯一无法从源码直接取到的数值，必须由 Rust 侧在目标平台实测一次后固化。

### 7.4 图标脉冲动画（易漏）

来源: `src/styles.css:80-94`

```css
.gauge-unit path { animation: unit-pulse 2s ease-in-out infinite;
                   transform-box: fill-box; transform-origin: center; }
@keyframes unit-pulse { 0%,100% { opacity: .5 } 50% { opacity: 1 } }
```

即单位行图标（**两个 path，含隐藏的那个**）持续 2 s 呼吸，opacity 0.5 → 1 → 0.5，`ease-in-out`。Slint 用 `animate opacity { duration: 2s; easing: ease-in-out; iteration-count: infinite; }` 无法直接表达 yoyo，需要 state 翻转或用两个 keyframe（Slint 支持 `iteration-count`/`direction: alternating` 的 Animation）。

---

## 8. 阶段与状态机

来源: `src/gauge/gauge.ts:29-32`（类型）、`283-302`（setStage）、`112-122`（内部状态）、`312-335`（其它 setter）、`344-355`（variant/accent）

### 8.1 类型

```rust
pub enum Stage { Idle, Ping, Download, Upload, Done }
pub enum ConnectionMode { Multi, Single }
pub enum ScaleMode { Fixed, Auto }
pub enum GradientVariant { DlMulti, UlMulti, DlSingle, UlSingle }
```
`Stage` 有 5 个取值（`idle|ping|download|upload|done`），其中 `ping` **不改变**任何视觉判定（不是 upload ⇒ download 系变体 + download 强调色），它只影响 `main.ts` 的按钮高亮。

### 8.2 setStage(stage) 的完整副作用（顺序敏感）

```
setStage(stage):
    if self.stage == stage: return          // ① 幂等早退，连图标都不碰
    self.stage = stage
    renderVariant(currentVariant())         // ② 重建 240 段圆环（内部还有 lastVariant 缓存）

    uploading = (stage == Upload)
    iconDownload.display = uploading ? "none" : ""
    iconUpload.display   = uploading ? ""     : "none"

    accent = currentAccent()               // ③ 按新 stage/mode 重算
    iconDownload.fill = color(accent.p3)   // ④ 两个图标都写成同一个 accent
    iconUpload.fill   = color(accent.p3)

    if stage == Upload || stage == Idle:   // ⑤ 只有这两个阶段归零
        motion.reset()                     // 500ms easeInOut 回 -135（见 §10.4）
        updateNumber(0)                    // 数字立刻变 "0.00"
```

- ① 早退⇒ 重复设置同一 stage **不会**重新切图标/重置指针。构造时 `stage` 初值就是 `idle`，所以 `setStage('idle')` 在初始态是空操作（这也解释了构造期需要自己写死初始 fill/display）。
- ⑤ `download` / `ping` / `done` **不**重置指针、**不**清零数字（`done` 保留最终读数，这是刻意为之：`src/gauge/gauge.ts:296-297` 注释）。
- `updateNumber(0)` 写入 `formatSpeed(0) = "0.00"`。

### 8.3 currentVariant / currentAccent 决策表

| stage | mode | variant | accent |
|---|---|---|---|
| upload | multi | `ul-multi` | `upload` = p3(0.671,0.380,0.910) / `#b75cf0` |
| upload | single | `ul-single` | `uploadSingle` = `#7cdb2b` |
| 其它任意（idle/ping/download/done） | multi | `dl-multi` | `download` = p3(0.000,0.773,0.749) / `#00c9c0` |
| 其它任意 | single | `dl-single` | `downloadSingle` = `#e56b4f` |

### 8.4 其余 API 精确语义

```
setSpeed(kbps):
    safe      = (is_finite(kbps) && kbps > 0) ? kbps : 0.0     // 负数/0/NaN/Inf → 0
    self.speed_kbps = safe
    mbps      = kbpsToMbps(safe)                               // = parseFloat((safe/1000).toFixed(2))，已量化到 2 位小数
    if scale_mode == Auto:
        applyLabels(if mbps > 1000 { LABELS_EXTENDED } else { LABELS_STANDARD })
    motion.setTarget(angleForValue(self.labels, mbps))          // spring 模式：只改 target，不改 angle
    updateNumber(safe)                                         // 入参是 kbps，不是 mbps
```

```
snapTo(kbps):                    // 冻结到精确值，无动画
    self.speed_kbps = (is_finite(kbps) && kbps > 0) ? kbps : 0.0
    mbps = kbpsToMbps(self.speed_kbps)
    if scale_mode == Auto: applyLabels(mbps > 1000 ? EXTENDED : STANDARD)
    angle = angleForValue(self.labels, mbps)
    motion.setTarget(angle, snap=true)        // angle=target，velocity=0，tween=null，wobble_amp=0
    self.display_angle = angle
    paint(angle)                              // 立刻重绘（不等下一帧）
    updateNumber(self.speed_kbps)
```

```
setConnectionMode(mode):
    if self.connection_mode == mode: return
    self.connection_mode = mode
    renderVariant(currentVariant())
    accent = currentAccent()
    iconDownload.fill = iconUpload.fill = color(accent.p3)     // 不改 display、不动指针

setScaleMode(mode):
    if self.scale_mode == mode: return
    self.scale_mode = mode
    applyLabels(if mode == Auto && self.speed_kbps / 1000 > 1000 { EXTENDED } else { STANDARD })

reset():
    self.speed_kbps = 0
    motion.reset()                 // 500ms easeInOut 回 -135
    updateNumber(0)                // 不 paint、不改 stage、不改 labels
```

`applyLabels(next)`：`if next === self.labels { return }`（**引用同一性**比较，不是值比较）；否则 `labels = next; buildLabels()`，`buildLabels` 重建全部 `<text>`、把每个 `node.on=false`、并把 `lastPaintAngle = NaN`（强制下一次 `paint` 全量刷新点亮状态）。Rust 侧用枚举 `ScaleKind::Standard | ScaleKind::Extended` 做"同一性"判断即可。

**auto 刻度切换条件**：`mbps > LABELS_STANDARD[last]`，即 `mbps > 1000`（严格大于）。两处出现：`setSpeed`（`gauge.ts:263`）与 `setScaleMode`（`gauge.ts:325`，写成 `speed_kbps/1000 > 1000`，注意这里比较的是**未量化的 kbps/1000**，与 `setSpeed` 的 `kbpsToMbps` 量化版本在边界上可能有 1e-3 的差异：`kbps=1000001` 时 `kbpsToMbps = 1000.0`（toFixed(2) 后 parseFloat）⇒ `setSpeed` 判 `1000.0 > 1000` 为假 → 用 STANDARD；`setScaleMode('auto')` 判 `1000.001 > 1000` 为真 → 用 EXTENDED。**这是一处真实存在的不一致，移植时请复刻同一行为**：`setSpeed` 用量化后的 Mbps，`setScaleMode` 用未量化的 `kbps/1000`。）

---

## 9. 刻度映射

来源: `src/gauge/scale.ts`（全文 85 行）

### 9.1 刻度表

```rust
pub const LABELS_STANDARD: [f64; 9] = [0.0, 5.0, 10.0, 50.0, 100.0, 250.0, 500.0, 750.0, 1000.0];      // scale.ts:26
pub const LABELS_EXTENDED: [f64; 10] = [0.0, 10.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0]; // scale.ts:34
```
9 档 ⇒ 步长 `270/8 = 33.75°`；10 档 ⇒ `270/9 = 30°`。

标准档的角度表（黄金值）：`0→-135, 5→-101.25, 10→-67.5, 50→-33.75, 100→0, 250→33.75, 500→67.5, 750→101.25, 1000→135`。

### 9.2 labelAngleAt

```
labelAngleAt(index, count):
    if count < 2: return -135.0                       // startAngle
    return index / (count - 1) * (135 - (-135)) + (-135)
```

### 9.3 angleForValue（值 → 角）

```
angleForValue(labels, mbps):
    n = labels.len()
    if n == 0: return -135.0                                   // startAngle
    if !is_finite(mbps) || mbps <= labels[0]: return labelAngleAt(0, n)
    idx = labels.iter().position(|x| *x >= mbps)               // findIndex 语义：第一个 >= mbps 的下标
    if idx.is_none(): return labelAngleAt(n-1, n)              // 超上限 → 夹在末档角
    idx = idx.unwrap()
    if mbps == labels[idx]: return labelAngleAt(idx, n)        // 命中刻度 → 精确角
    i0  = if idx == 0 { 0 } else { idx - 1 }
    lo  = labels[i0]; hi = labels[idx]
    span = hi - lo
    t = if span == 0 { 0.0 } else { (mbps - lo) / span }
    return labelAngleAt(i0, n) + t * (labelAngleAt(idx, n) - labelAngleAt(i0, n))
```

要点：
- `findIndex(x => x >= v)` 是**第一个** `>=` 的下标；表内严格递增，所以等价于"上界档位"。
- `mbps <= labels[0]` 分支在 `idx` 之前判，保证 `idx=0` 且 `mbps < labels[0]` 时也返回端角（否则 `t` 会为负）。`labels[0]` 恒为 0，因此 `mbps <= 0` 全走此分支。
- `idx == -1`（超过最大刻度）：返回 `labelAngleAt(n-1,n)` = **端角 135（clamp，不越界）**。
- `span == 0` 保护：表内不会有重复值，属防御性写法，保留即可。
- 注意 `i0 == idx`（当 `idx == 0`）时 `t` 应为 0；这里 `idx=0` 分支已被 `mbps <= labels[0]` 拦截，实际不可达。
- 非均匀刻度的本质：**刻度等角分布，档内线性插值**。所以 `0→100` 与 `250→1000` 各占 135°，`0..5` 这一小段也占 33.75°。

### 9.4 valueForAngle（角 → 值，反函数，逐行等价）

```
valueForAngle(labels, angle):
    n = labels.len()
    if n < 2: return 0.0
    clamped = min(135.0, max(-135.0, angle))
    step    = (135.0 - (-135.0)) / (n - 1)                      // 33.75 (n=9) / 30.0 (n=10)
    idx     = min(n - 2, max(0, floor((clamped - (-135.0)) / step)))
    t       = (clamped - labelAngleAt(idx, n)) / step
    return labels[idx] + t * (labels[idx + 1] - labels[idx])
```

- 它假设档位等角（由 `labelAngleAt` 保证），所以用 `step` 反解，而不是再查一次表。
- `clamped == 135` 时 `(270)/33.75 = 8 = n-1`，被 `min(n-2)` 夹到 7，`t = (135-101.25)/33.75 = 1.0` ⇒ 返回 `labels[8] = 1000`。正确。
- 黄金值：`valueForAngle(STANDARD, 116.37) = 862.0`（`idx=7`，`t=(116.37-101.25)/33.75=0.448`，`750+0.448*250=862`）。
- 该函数在源码里**只有导出、无内部调用点**（`scale.ts:59-68`），属于对外 API / 测试用反函数。移植时保留为纯函数即可。

### 9.5 formatSpeed（数字显示）

来源: `src/gauge/scale.ts:70-81`

```
MBPS_FORMAT = Intl.NumberFormat("en-US", { style: "decimal",
                                           minimumFractionDigits: 2,
                                           maximumFractionDigits: 2 })
formatSpeed(kbps):
    if !is_finite(kbps) || kbps < 0: return "—"        // U+2014 EM DASH
    if kbps == 0:                    return MBPS_FORMAT.format(0)     // "0.00"
    return MBPS_FORMAT.format(kbps / 1000)
```

必须复刻的三点：
1. **恒定两位小数**并带 `en-US` 千分位逗号：`862000 → "862.00"`、`4200000 → "4,200.00"`、`12345 → "12.35"`。
2. `0` 走单独分支返回 `"0.00"`（`-0.0` 也命中 `kbps == 0`，得 `"0.00"`）。
3. 非有限（NaN/±Inf）或**负数** → `"—"`（U+2014）。注意 `setSpeed` 已把负数折成 0，所以这条只在直接调用 `formatSpeed` 时可见。
4. 舍入模式：`Intl.NumberFormat` 默认 `roundingMode: "halfExpand"`（0.5 远离零进位）；Rust 的 `format!("{:.2}")` 是"对精确二进制值做 half-to-even"。差异只在十进制展开恰好落在 `...5` 上时出现。若要逐字节对齐，建议实现一个 "按精确十进制展开 → halfExpand → 截到 2 位 → 插千分位" 的格式化器（可用 `format!("{:.20}", v)` 取近似精确展开后手工舍入），或直接接受该差异并在对拍时用容差。

```rust
pub fn format_speed(kbps: f64) -> String {
    if !kbps.is_finite() || kbps < 0.0 { return "\u{2014}".to_string(); }
    if kbps == 0.0 { return "0.00".to_string(); }
    group_2dp(kbps / 1000.0)      // 见上：halfExpand + "," 千分位
}
```

### 9.6 kbpsToMbps（语义差异）

来源: `src/gauge/scale.ts:83-85`

```
kbpsToMbps(kbps) = parseFloat((kbps / 1000).toFixed(2))
```

- `toFixed(2)`：先量化到 2 位小数**字符串**（舍入规则同 `toFixed` 规范：对精确值做"近似值 n 使 n/10^2 - x 最小，并取较大的 n"⇒ 相当于 half-up，且受二进制表示影响），再 `parseFloat` 丢掉尾零 ⇒ **返回 number，不是字符串**。
- 与 `formatSpeed` 的关键差异：
  | 输入 kbps | `formatSpeed` | `kbpsToMbps` |
  |---|---|---|
  | 862000 | `"862.00"`（保留两位 + 无千分位问题） | `862.0`（数值） |
  | 4200000 | `"4,200.00"` | `4200.0` |
  | 12345 | `"12.35"` | `12.35` |
  | 12344 | `"12.34"` | `12.34` |
  | 1000001 | `"1,000.00"`（1e6/1000=1000.001 → 2 位 → "1000.00"） | `1000.0` |
- 用途：`kbpsToMbps` 只喂 `angleForValue` 和 `mbps > 1000` 判定，**不用于显示**。所以那 0.001 Mbps 级别的量化在指针角度上是 ~0.000135° 级别，肉眼不可见，但它决定了 §8.4 里 `setSpeed`/`setScaleMode` 的边界分歧。
- Rust 等价：`fn kbps_to_mbps(kbps: f64) -> f64 { round_2dp_half_up(kbps / 1000.0) }`，其中 `round_2dp_half_up` 必须与 `toFixed(2)`+`parseFloat` 等价（即：量化到 2 位后仍保留 f64，不做字符串往返）。

---

## 10. 动画引擎

来源: `src/gauge/animator.ts`（全文 231 行）

### 10.1 cubicBezier 求值器（完整算法）

来源: `src/gauge/animator.ts:26-65`

```
cubicBezier(x1, y1, x2, y2) -> Fn(f64)->f64:
    cx = 3*x1
    bx = 3*(x2 - x1) - cx
    ax = 1 - cx - bx
    cy = 3*y1
    by = 3*(y2 - y1) - cy
    ay = 1 - cy - by

    sampleX(t)  = ((ax*t + bx)*t + cx)*t
    sampleY(t)  = ((ay*t + by)*t + cy)*t
    sampleDX(t) = (3*ax*t + 2*bx)*t + cx

    solveX(x):
        t = x
        for i in 0..8:                       // 牛顿迭代，最多 8 次
            err = sampleX(t) - x
            if |err| < 1e-6: return t
            d = sampleDX(t)
            if |d| < 1e-6: break             // 导数退化 → 转二分
            t -= err / d
        lo = 0.0; hi = 1.0; t = x
        for i in 0..24:                      // 二分兜底，最多 24 次
            v = sampleX(t)
            if |v - x| < 1e-6: break
            if v < x { lo = t } else { hi = t }
            t = (lo + hi) / 2
        return t

    return |x|:
        if x <= 0: return 0
        if x >= 1: return 1
        return sampleY(solveX(x))
```

细节：牛顿循环结束后**不重置 `lo/hi` 的初值来源**——`lo=0, hi=1, t=x`（重新以 x 为起点做二分，而不是用牛顿的结果继续）。二分是"定点迭代"，最多 24 步，`1/2^24 ≈ 6e-8` 精度，加上 `1e-6` 提前退出。

### 10.2 两条具名缓动

来源: `src/gauge/animator.ts:68-69`

```rust
EASE_OUT    = cubicBezier(0.0, 0.0, 0.58, 1.0);   // ≈ CSS cubic-bezier(0,0,0.58,1)
EASE_IN_OUT = cubicBezier(0.42, 0.0, 0.58, 1.0);  // ≈ CSS cubic-bezier(0.42,0,0.58,1)
```

### 10.3 spring 参数与积分方程

来源: `src/gauge/animator.ts:85-92`（配置）、`171-183`（积分）

```rust
DEFAULT_NEEDLE_CONFIG = NeedleConfig {
    mode: Spring,            // 默认 spring
    omega: 21.4,             // rad/s
    zeta: 0.92,              // 阻尼比
    wobble_hz: 6.5,
    wobble_max: 1.1,         // 度
    wobble_gain: 0.0032,     // 度 / (度/秒)
};
```

```
// 每帧：
k = omega*omega                       // 21.4^2 = 457.96
c = 2*zeta*omega                      // 2*0.92*21.4 = 39.376
steps = max(1, ceil(dt / (1/480)))    // 子步长上限 1/480 s
h = dt / steps
for _ in 0..steps:
    a = k*(target - angle) - c*velocity
    velocity += a * h                 // 半隐式欧拉：先更新速度
    angle    += velocity * h
```

- 子步长规则：`1/480 s` 是**上限**（即 60 Hz 帧 dt=0.0167 s ⇒ steps=8，h=0.00208；120 Hz ⇒ steps=4；dt=0.05 s ⇒ steps=24）。
- 阶跃响应 ≈ 200 ms 收敛（`omega=21.4`），与 tween 的 `needleTracking:0.2` 体感对齐；`zeta=0.92` 略欠阻尼 ⇒ 极小过冲。
- 半隐式欧拉（symplectic Euler）而不是 RK4：必须逐字复刻，否则轨迹不一致。

### 10.4 tween 模式、重定向规则、springResetTween 特殊路径

来源: `src/gauge/animator.ts:117-147`、`154-183`

```
setTarget(angle, opts={snap:false, duration:None, ease:None}):
    self.target = clamp(angle, -135, 135)
    if opts.snap:
        self.angle = self.target; self.velocity = 0
        self.tween = None; self.wobble_amp = 0
        return
    if self.config.mode == Tween:
        self.tween = Tween { from: self.angle,     // ← 从"当前角"重开，不是从上一个 tween 的 from
                             to:   self.target,
                             elapsed: 0,
                             duration: opts.duration ?? TIMING.needleTracking /* 0.2 */,
                             ease: opts.ease ?? EASE_OUT }
```

**重定向规则**：每次 `setTarget` 都把 `elapsed` 归零、`from` 设为**当前** `angle`、`duration` 重置为完整 200 ms（不从剩余时间续算），因此连续高频样本会产生"永远追不上"的追赶曲线（这是原站的真实行为）。

```
reset(duration = TIMING.needleReturn /* 0.5 */):
    self.target = -135
    self.tween = Tween { from: self.angle, to: -135, elapsed: 0, duration, ease: EASE_IN_OUT }
    if self.config.mode == Spring:
        self.spring_reset_tween = self.tween        // ← 额外挂到"弹簧模式也要遵守的显式时长"槽
```

`step()` 的分支优先级（**关键**）：

```
step(dt) -> f64:
    if mode == Tween or spring_reset_tween.is_some():
        tw = spring_reset_tween.or(tween)
        if let Some(tw) = tw:
            tw.elapsed += dt
            raw  = if tw.duration <= 0 { 1.0 } else { min(1.0, tw.elapsed / tw.duration) }
            next = tw.from + (tw.to - tw.from) * (tw.ease)(raw)
            self.velocity = (next - self.angle) / max(dt, 1e-4)      // 速度由差分反推
            self.angle    = next
            if raw >= 1.0:
                if tw is spring_reset_tween { spring_reset_tween = None; self.velocity = 0 }
                self.tween = None
        // 注意：tw 为 None 时什么也不做，velocity 保留旧值
    else:
        ... spring 积分（见 §10.3）
```

三个必须复刻的怪癖：
1. **`springResetTween` 不被 `setTarget` 清空**。reset 之后再调 `setTarget`（spring 模式、非 snap）时，`springResetTween` 仍在，所以 `step` 会继续跑**归位 tween**，直到它跑满并置空，然后才开始向新 target 的弹簧。`setTarget(snap=true)` 同样不清它。
2. 归位 tween 结束时把 `velocity` 显式置 0（避免弹簧带着残余速度冲出去）。
3. tween 模式且无 active tween 时，`velocity` 保持上一次的值 ⇒ 微颤振幅 `ampTarget` 也保持（会缓慢衰减到该值，见 §10.5）。

### 10.5 isSettled

来源: `src/gauge/animator.ts:149-151`

```
is_settled = |angle - target| < 0.02 && |velocity| < 0.5
```
（角度容差 0.02°，速度容差 0.5 °/s）

### 10.6 微颤（wobble）

来源: `src/gauge/animator.ts:185-193`

```
slew       = |velocity|                                   // 度/秒
amp_target = min(wobble_max /*1.1*/, slew * wobble_gain /*0.0032*/)
wobble_amp += (amp_target - wobble_amp) * min(1.0, dt / 0.09)     // 90 ms 一阶低通
wobble_phase += dt * wobble_hz /*6.5*/ * 2*PI                     // 相位累计，不回卷
wobble      = sin(wobble_phase) * wobble_amp
return clamp(angle + wobble, -135 - 2, 135 + 2)                   // ⇒ [-137, 137]
```

- **`clamp(-137, 137)` 就在这里**：`step()` 的返回值（也就是 `paint()` 的入参与指针 `rotate` 的角度）被夹在 `[startAngle-2, endAngle+2]`。这就是任务里"paint 返回值"所指的夹持 —— `paint()` 本身返回 `void`，夹持在 `step()` 里。
- `wobblePhase` 单调累加，**从不清零**（包括 `reset`/`snap`）；`snapTo` 只把 `wobbleAmp` 归零。
- `wobbleMax = 1.1` 是"硬上限"，`slew*gain` 达到 1.1 需要 `slew ≥ 343.75 °/s`；一次 500 ms 的 270° 归位速度峰值 ≈ 848 °/s ⇒ 会打满。
- 关掉微颤：`wobble_max = 0` ⇒ `amp_target = 0` ⇒ `wobble_amp` 指数衰减到 0。

### 10.7 Clock（共享 RAF 时钟）

来源: `src/gauge/animator.ts:198-231`

```
tick(now):
    dt = min(0.05, (now - last) / 1000)          // ← dt 上限 0.05 s（后台标签页回来不会爆冲）
    last = now
    for fn in subs: fn(dt)
    handle = requestAnimationFrame(tick)
```

- `dt` 上限 **0.05 s**；下限无（首次 tick 的 dt 约为 0）。
- `add(fn)` 返回 unsubscribe；`subs` 为空时自动 `stop()`。
- Slint 落地：用 `Timer { interval: 16ms; running: true; }` 或者 `slint::Timer` + `slint::invoke_from_event_loop`；**必须自己做 dt 上限 0.05**，因为 Slint 的 timer 不保证间隔（尤其 `Timer` 的最小粒度与合成器刷新率有关）。更稳的做法：以 `std::time::Instant` 记录上一帧，得到真实 dt 后 `min(0.05)`。

---

## 11. 数据层

来源: `src/gauge/pipeline.ts`（全文 178 行）

### 11.1 接口

```rust
pub struct SpeedSample { pub kbps: f64, pub stage: Stage }
pub trait SpeedSource { fn name(&self) -> &str; fn start(&mut self, cb: SampleListener); fn stop(&mut self); }
```

### 11.2 DemoSource

来源: `pipeline.ts:51-127`

**默认值（源码实测，与 README 一致）**：
| 参数 | 默认 | 源码 |
|---|---|---|
| `downloadPeak` | **862** Mbps | `pipeline.ts:61` |
| `uploadPeak` | **341** Mbps | `pipeline.ts:62` |
| `holdMs` | **4200** ms | `pipeline.ts:63` |
| `seed` | **20260101** | `pipeline.ts:64` |
| `tickMs` | 100 ms（硬编码） | `pipeline.ts:76` |
| `rampMs` | 1500 ms（硬编码） | `pipeline.ts:77` |
| ping 前置等待 | 1200 ms（硬编码） | `pipeline.ts:117` |
| download→upload 间隔 | 620 ms（硬编码） | `pipeline.ts:115` |

> `main.ts` 只透传 `seed`（`main.ts:43`: `seed: num('seed', 20260101)`），**不传** `downloadPeak`/`uploadPeak`/`holdMs`，所以运行时生效值就是上表默认值。README §1 的 `?autoplay=1&seed=7` 与 §4.6 完全一致。

**随机数：mulberry32**（`pipeline.ts:24-33`，逐字复刻）

```
mulberry32(seed):
    a = seed as u32 (>>> 0)
    return ||:
        a = (a + 0x6d2b79f5) mod 2^32
        t = a
        t = imul(t XOR (t >>> 15), t OR 1)              // Math.imul = 有符号 32 位乘法（wrapping）
        t = t XOR (t + imul(t XOR (t >>> 7), t OR 61))  // 注意：加法是 JS number 加法后 XOR ⇒ 有符号
        return ((t XOR (t >>> 14)) mod 2^32) as f64 / 4294967296.0
```
Rust 等价（全用 `u32` + `wrapping_*`，`>>>` 是逻辑右移，`Math.imul` 是 `wrapping_mul` 后按 `i32` 解释再转回 `u32`）：
```rust
fn mulberry32(seed: u32) -> impl FnMut() -> f64 {
    let mut a = seed;
    move || {
        a = a.wrapping_add(0x6d2b79f5);
        let mut t = a;
        t = ((t ^ (t >> 15)) as i32).wrapping_mul((t | 1) as i32) as u32;
        let s = ((t ^ (t >> 7)) as i32).wrapping_mul((t | 61) as i32) as u32;
        t ^= t.wrapping_add(s);
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }
}
```

**时间线（wall clock，ms）**

```
t=0        phase('ping');  emit('ping', 0)
t=1200     runMeasuredPhase('download', 862, 4200)
             phase('download')
             loop 每 100ms 采样，直到 t_local >= 1500+4200 = 5700
t=6900     phase('upload')
t=7520     runMeasuredPhase('upload', 341, 4200)
t=13220    finish(): phase('done'); emit('done', dl*1000); onDone()
```

**每 tick 的样本公式**（`pipeline.ts:87-93`）：

```
t       = now - started                            // ms
rampT   = min(1, t / 1500)
envelope= if rampT < 1 { 1 - (1 - rampT)^3 } else { 1 }        // ease-out cubic 包络
drift   = sin(t/1000 * 1.7) * 0.012 + sin(t/1000 * 4.3) * 0.006
noise   = (rnd() - 0.5) * 0.03                                 // ±1.5%
mbps    = peak * envelope * (1 + drift + noise)
emit(stage, max(0, mbps * 1000))                               // ← kbps
```

- `drift` 永不为 0 ⇒ 平台期始终有 ±1.8% 的整体漂移，加上 ±1.5% 噪声 ⇒ 峰值样本会略高于 `peak`（最大约 `862 * 1.03 ≈ 888 Mbps`）。
- `emit` 里 `Math.max(0, kbps)` 兜底。
- **`finish()` 发出的最终读数用的是 `dl * 1000`（下载峰值），不是上传峰值**（`pipeline.ts:102`）。这是源码真实行为，移植时若不想继承这个"看起来像 bug"的行为，必须显式记为 deviation 而不是默默改掉。
- `stop()` 置 `stopped = true` 并 `clearTimeout`；所有回调开头都检查 `stopped`。

### 11.3 ManualSource

来源: `pipeline.ts:130-145`

```
start(cb)          -> 保存 cb
stop()             -> cb = None
push(kbps, stage)  -> cb?.({kbps, stage})       // stage 默认 'download'，不做任何 clamp
```

### 11.4 WebSocketSource

来源: `pipeline.ts:152-178`

```
start(cb):
    stop()                                  // 先关旧连接
    socket = WebSocket(url)
    socket.on('message', ev):
        try:
            data = JSON.parse(String(ev.data))       // 期望 { kbps: number, stage?: Stage }
            if typeof data.kbps == 'number':
                cb({ kbps: data.kbps, stage: data.stage ?? 'download' })
        catch: 忽略脏帧
stop(): socket?.close(); socket = None
```
- 只校验 `kbps` 是数字；`stage` 任意字符串会被原样透传（**不校验枚举**，`setStage` 收到非法值只会走到"非 upload ⇒ download 系"分支）。
- 无重连、无心跳、无鉴权。

---

## 12. 交互层

来源: `src/main.ts`（全文 201 行）

### 12.1 URL 参数全表

初始化顺序：先读参数 → 写 `document.documentElement.dataset.theme` → `body.classList.toggle('is-bare')` → 构造 `Gauge`（`main.ts:11-20`）。

| 参数 | 取值 | 默认 | 语义 | 源码 |
|---|---|---|---|---|
| `theme` | `light` / 其它 | `dark` | `=== 'light'` 才 light；写 `html[data-theme]`，并作为 `GaugeOptions.theme` | `main.ts:11,16,20` |
| `mode` | `single` / 其它 | `multi` | 连接模式 | `main.ts:12` |
| `scale` | `auto` / 其它 | `fixed` | 刻度模式 | `main.ts:13` |
| `bare` | `1` / 其它 | 关 | `body.classList.toggle('is-bare', bare)`；`.is-bare` 隐藏 `.panel` 与 `.readout` | `main.ts:14,17`；`styles.css:184-187` |
| `static` | number（Mbps） | 无 | 存在即冻结：`setStage(stage)` → `snapTo(mbps*1000)` → 同步按钮/readout/slider | `main.ts:143-157` |
| `stage` | `idle\|ping\|download\|upload\|done` | `static>0 ? 'download' : 'idle'` | 与 `static` 配合 | `main.ts:146` |
| `autoplay` | `1` / 其它 | 关 | 无 `static` 时启动 `startDemo()` | `main.ts:158-159` |
| `seed` | number | `20260101` | 传给 `DemoSource` | `main.ts:43` |
| `value` | number | `862` | Download / Done 按钮与 `snapTo` 的目标 Mbps | `main.ts:90,94` |
| `uvalue` | number | `341` | Upload 按钮的目标 Mbps | `main.ts:92` |

`num(key, fallback)`：`Number(raw)`，`Number.isFinite` 为假则用 fallback（`main.ts:5-9`）。注意空串 `?seed=` → `Number('') = 0` → 有限 ⇒ **seed 变 0**。

`static` 内部分支：`stage = params.get('stage') ?? (mbps > 0 ? 'download' : 'idle')`；`snapTo(Number.isFinite(mbps) ? mbps*1000 : 0)`；并把 slider 与标签同步到该值。

### 12.2 window.speedGauge 方法表

来源: `main.ts:165-199`

| 成员 | 行为 |
|---|---|
| `setSpeed(kbps)` | `applySample(kbps)`：**不传 stage** ⇒ 若 stage 未变则不触发 `setStage`；然后 `gauge.setSpeed(kbps)` + `updateReadout(kbps)` |
| `setStage(stage)` | 记录 `lastStage` → `gauge.setStage` → 按钮高亮同步；**不动指针/数字**（除 upload/idle 的重置副作用） |
| `snapTo(kbps)` | 直接转 `gauge.snapTo`（会重绘，但**不更新 readout**） |
| `reset()` | `gauge.reset()`（500 ms 归位 + 数字 0.00；不改 stage） |
| `setConnectionMode(mode)` | 转 `gauge` |
| `setScaleMode(mode)` | 转 `gauge` |
| `playDemo()` | `startDemo()`：停旧源 → `lastStage='idle'` → `setStage('idle')` → `reset()` → 新建 `DemoSource({seed: ?seed ?? 20260101, onPhase, onDone})` → `start` |
| `stop()` | `stopSource()`：停源 + 按钮图标复位 |
| `manual` | `ManualSource` 实例（可 `push(kbps, stage)`） |
| `gauge` | `Gauge` 实例（完整 API） |

页面末端写 `document.body.dataset.ready = 'true'`（`main.ts:201`）—— 所有截图工具的同步点。

### 12.3 控件行为

来源: `main.ts:71-140`、`index.html:14-29`

| 控件 | id | 行为 |
|---|---|---|
| Run demo / Stop | `#play` | `dataset.playing` 翻转；文本 `Run demo` / `Stop`；点击时 playing==='true' → stop，否则 startDemo |
| Reset | `#reset` | 停源 → `lastStage='idle'` → `setStage('idle')` → `snapTo(0)` → 同步按钮 |
| 滑块 | `#speed` | `input` 事件：`mbps = Number(value)`；label = `mbps.toLocaleString('en-US') + " Mbps"`；停源；`lastStage = mbps===0 ? 'idle' : (lastStage==='upload' ? 'upload' : 'download')`；`setStage` 后 `setSpeed(mbps*1000)` + readout。范围 `0..10000 step 1` |
| Single | `#single` | `change` → `setConnectionMode(checked?'single':'multi')` |
| Extend scale | `#autoscale` | `change` → `setScaleMode(checked?'auto':'fixed')` |
| Idle/Download/Upload/Done | `[data-stage]` | 停源 → `setStage(stage)` → 再 `snapTo`：idle→0；download/done→`num('value',862)*1000`；upload→`num('uvalue',341)*1000` |

`updateReadout(kbps)`（`main.ts:63-69`）：
```
`${(kbps/1000).toLocaleString('en-US',{min/maxFractionDigits:2})} Mbps  ·  ${round(kbps).toLocaleString('en-US')} kbps`
```
分隔符是**两个空格 + U+00B7 + 两个空格**（`"  ·  "`）。

### 12.4 主题切换对 token 的影响

- 主题只在**构造期**决定：`Gauge.theme` 是 `readonly`（`gauge.ts:97`），运行时切不到。换主题 = 换 URL 参数重载页面。
- 主题影响：`THEME[theme]` 的 6 个颜色 + 指针渐变 stops（§5.3）；`html[data-theme]` 影响页面外壳的 6 个 CSS 变量：

| CSS 变量 | dark | light | 来源 |
|---|---|---|---|
| `--page` | `#000000` | `#f2f2f2` | `styles.css:16,26` |
| `--text` | `#ffffff` | `#111111` | `styles.css:17,27` |
| `--muted` | `#666666` | `#a3a3a3` | `styles.css:18,28` |
| `--panel` | `rgba(255,255,255,0.05)` | `rgba(0,0,0,0.04)` | `styles.css:19,29` |
| `--panel-line` | `rgba(255,255,255,0.09)` | `rgba(0,0,0,0.08)` | `styles.css:20,30` |
| `--accent` | `#00c9c0`（两套相同） | 同 | `styles.css:21` |
| `color-scheme` | `dark` | `light` | `styles.css:22,31` |

---

## 13. 样式

来源: `src/styles.css`（全文 187 行）

### 13.1 字体资源

| 文件 | 字节 | 许可 | 声明 |
|---|---|---|---|
| `public/fonts/gaugemono.woff2` | 4240 | **Ookla 资产，不随仓库许可分发**（README §7）；本地存在 | `@font-face { font-family: 'Gauge Mono'; src: url('/fonts/gaugemono.woff2') format('woff2'); font-weight: 400 500; font-display: block; }`（`styles.css:1-6`） |
| `public/fonts/montserrat.woff2` | 118624 | **SIL OFL 1.1**（`OFL.txt`，Copyright 2024 The Montserrat.Git Project Authors） | 同上，`woff2-variations` + `woff2` 双源，`font-weight: 100 900`（`styles.css:8-13`） |
| `public/fonts/OFL.txt` | 4400 | SIL OFL 1.1 全文 | — |

字体族栈：
- 数字：`'Gauge Mono', ui-monospace, SFMono-Regular, Menlo, Consolas, monospace`（`gauge.ts:209`）
- 刻度 + 单位行 + 页面：`Montserrat, 'Segoe UI', system-ui, sans-serif`（`gauge.ts:234,394`；`styles.css:47`）

### 13.2 页面外壳的排版常量

| 选择器 | 属性 | 值 | 来源 |
|---|---|---|---|
| `body` | `font-family` | `Montserrat, 'Segoe UI', system-ui, sans-serif` | `styles.css:47` |
| `body` | `font-size` | `15px` | `styles.css:48` |
| `body` | `background` / `color` | `var(--page)` / `var(--text)` | `styles.css:45-46` |
| `body` | 布局 | `flex column; align-items:center; justify-content:center; gap:8px; min-height:100vh; margin:0; -webkit-font-smoothing:antialiased` | `styles.css:43-56` |
| `#gauge-stage` | 尺寸 | `width: min(60vh, 440px); aspect-ratio: 1/1; display:grid; place-items:center; will-change: transform` | `styles.css:58-64` |
| `.gauge-svg` | — | `display:block; width:100%; height:100%; overflow:visible; shape-rendering:geometricPrecision; text-rendering:geometricPrecision` | `styles.css:66-73` |
| `.gauge-number` | — | `font-variant-numeric: tabular-nums; font-feature-settings:'tnum' 1` | `styles.css:75-78` |
| `.gauge-unit path` | — | `animation: unit-pulse 2s ease-in-out infinite; transform-box: fill-box; transform-origin: center` | `styles.css:80-84` |
| `@keyframes unit-pulse` | — | `0%,100% { opacity:.5 } 50% { opacity:1 }` | `styles.css:86-94` |
| `.panel` | — | `flex wrap; gap:10px; padding:14px 18px; border-radius:14px; background:var(--panel); border:1px solid var(--panel-line); backdrop-filter:blur(8px); max-width:min(92vw,720px)` | `styles.css:96-108` |
| `.panel button` | — | `font-size:13px; font-weight:600; color:var(--text); background:transparent; border:1px solid var(--panel-line); border-radius:999px; padding:8px 16px; transition: background/border-color/color .15s ease` | `styles.css:110-121` |
| `.panel button:hover` | `border-color` | `rgba(255,255,255,0.3)` | `styles.css:123-125` |
| `.panel button.is-active` | — | `background:var(--accent); border-color:var(--accent); color:#001210` | `styles.css:127-131` |
| `.panel button[data-playing='true']` | — | `background:#ff3366; border-color:#ff3366; color:#fff` | `styles.css:133-137` |
| `.field` | — | `gap:10px; font-size:12px; color:var(--muted); letter-spacing:0.02em` | `styles.css:139-146` |
| `.field input[type=range]` | `width` | `190px`；`accent-color: var(--accent)` | `styles.css:148-151` |
| `.field output` | — | `min-width:92px; tabular-nums; color:var(--text); font-weight:600; font-size:12px` | `styles.css:153-159` |
| `.switch` | — | `gap:6px; font-size:12px; color:var(--muted); user-select:none`；`input { accent-color: var(--accent) }` | `styles.css:161-173` |
| `.readout` | — | `tabular-nums; font-size:11px; letter-spacing:0.06em; color:var(--muted); text-transform:uppercase; min-height:16px` | `styles.css:175-182` |
| `body.is-bare .panel, body.is-bare .readout` | `display` | `none` | `styles.css:184-187` |

**字间距（letter-spacing）汇总**（任务要求）：只有两处 —— `.field` 为 `0.02em`（12px ⇒ 0.24px），`.readout` 为 `0.06em`（11px ⇒ 0.66px）。SVG 内的 `<text>`（数字 / 刻度 / Mbps）**没有** letter-spacing，数字靠 `tabular-nums` + `tnum` 特性等宽。

**页面底色**：`#000000`（dark）/ `#f2f2f2`（light）。

---

## 附录 A — 黄金测试向量（可直接写成单元测试）

```text
# 几何（以下字符串均经 r3 格式化，可直接断言相等）
polar(143,143,130,-135)           = (51.0761…, 234.9239…)   → r3 → "51.076" / "234.924"
polar(143,143,130,   0)           = (143.0, 13.0)            → 正上方（143, 13）
polar(143,143,130,  90)           = (273.0, 143.0)           → 正右方，顺时针为正
polar(143,143,130, 135)           = (234.9239…, 234.9239…)
polar(143,143, 97,-135)           = (74.4106…, 211.5893…)     → 刻度 "0" 锚点
polar(143,143, 97,   0)           = (143.0, 46.0)             → 刻度 "100" 锚点

arcPath(143,143,130,-135,135)     = "M 51.076 234.924 A 130 130 0 1 1 234.924 234.924"   ← 轨道弧（逐字节对齐 README §4.4）
arcPath(143,143,130,-135,-133.595)= "M 51.076 234.924 A 130 130 0 0 1 48.85 232.642"     ← 圆环第 0 段（含 overlap）
arcPath(143,143,130, 133.875,135) = "M 236.711 233.101 A 130 130 0 0 1 234.924 234.924" ← 圆环第 239 段（被 min 截断）
sectorPath(143,143,141.5,-135,  0) = "M 143 143 L 42.944 243.056 A 141.5 141.5 0 0 1 143 1.5 Z"
sectorPath(143,143,141.5,-135, 45) = "M 143 143 L 42.944 243.056 A 141.5 141.5 0 0 1 243.056 42.944 Z"
                                     ← |45-(-135)| = 180，large 仍为 0（严格 >180 才置 1）
sectorPath(143,143,141.5,-135,116.37) = "M 143 143 L 42.944 243.056 A 141.5 141.5 0 1 1 269.776 205.85 Z"
                                     ← 862 Mbps 的实际裁剪扇形（|116.37+135| = 251.37 > 180 ⇒ large=1）

# 刻度
labelAngleAt(0,9) = -135 ; labelAngleAt(4,9) = 0 ; labelAngleAt(8,9) = 135 ; labelAngleAt(7,9) = 101.25
labelAngleAt(0,10)= -135 ; labelAngleAt(9,10)= 135 (step 30)
angleForValue(STANDARD,   0) = -135
angleForValue(STANDARD,   8) = -81.0            # (8-5)/5=0.6 ; -101.25+0.6*33.75
angleForValue(STANDARD, 155.61) = 12.51225       # (155.61-100)/150 = 0.3707333… ; 0 + t*33.75
angleForValue(STANDARD, 341) = 46.035            # (341-250)/250 = 0.364 ; 33.75 + t*33.75
angleForValue(STANDARD, 862) = 116.37            # (862-750)/250 = 0.448 ; 101.25 + t*33.75
angleForValue(STANDARD,1000) = 135
angleForValue(STANDARD,4200) = 135               # idx=-1 → clamp
angleForValue(STANDARD,  f64::NAN) = -135
valueForAngle(STANDARD, 116.37) = 862.0
valueForAngle(STANDARD, -200)   = 0.0            # clamp 到 -135 ⇒ idx 0, t 0
valueForAngle(STANDARD,  200)   = 1000.0

# 数字
formatSpeed(0)        = "0.00"
formatSpeed(862000)   = "862.00"
formatSpeed(12345)    = "12.35"
formatSpeed(4200000)  = "4,200.00"
formatSpeed(-1)       = "—"
formatSpeed(f64::NAN) = "—"
kbpsToMbps(862000)    = 862.0
kbpsToMbps(12345)     = 12.35
kbpsToMbps(1000001)   = 1000.0

# 圆环
n=240, span=270, overlap=0.28
i=0   → a0=-135.0    , a1=-133.595 , t=0.00208333
i=1   → a0=-133.875  , a1=-132.47  , t=0.00625
i=239 → a0=133.875   , a1=135.0 (被 min 截断) , t=0.99791667
最后一档 stop 表颜色出现在 i=239 之外的段？否：t 最大 0.99792，落在 [0.9630,1.0000] 区间内 ⇒ #7aff77 → #7bfe71 之间插值。

# 微颤
wobble_amp 目标 = min(1.1, |velocity|*0.0032)
vel=1000 → 1.1 (打满) ; vel=100 → 0.32 ; vel=10 → 0.032
step() 返回 clamp(angle+wobble, -137, 137)

# 动画
cubicBezier(0,0,0.58,1)(0.5) ≈ 0.8  (精确值需按 §10.1 算法算，不要查表)
EASE_OUT(0)=0 ; EASE_OUT(1)=1 ; EASE_OUT(0.2) 用算法求值
spring: k=457.96, c=39.376, 子步长 h<=1/480
```

> 上表 `arcPath` 第 0 段与 `sectorPath` 的示例用于**格式**校验（分隔符/小数位/large 标志），`a1=-133.875` 是"无 overlap"的裸值；实际渲染用的是 `-133.595`。写单元测试时以实际公式为准。

## 附录 B — 源码 vs README 不一致清单（移植时以源码为准）

| # | README 说法 | 源码实际 | 影响 |
|---|---|---|---|
| B1 | §2.5 表：download single 终点 `#ffba6b` | `gradient-stops.ts:103`：`t=1.0000` 是 **`#ffb96b`**；`#ffba6b` 只覆盖 `t=0.5185…0.9630` | 弧末端的最后 0.36% 角宽（≈1°）颜色差 1/255 |
| B2 | §2.5 表：upload single 终点 `#cfff70` | `gradient-stops.ts:133`：`t=1.0000` 是 **`#ceff70`**；`#cfff70` 覆盖 `t=0.5185…0.9630` | 同上 |
| B3 | §1 未提及 `holdMs` | `pipeline.ts:63`：`holdMs` 默认 **4200 ms**（README 的时序描述没有这个常量） | 复刻演示时长需要它 |
| B4 | §3.4 / §2.3 未提及 `finish()` 的读数来源 | `pipeline.ts:102`：`done` 阶段的最终样本是 **下载峰值** `dl*1000`，即便刚跑完 upload | 演示结束时数字会"跳回" 862.00，看起来像 bug，但这是源码行为 |
| B5 | §4.4 表述"指针角度 @862 Mbps = 116.37°" | 一致（已核算） | — |
| B6 | §2.3 `containerSize = e*2.2` | 一致：`size` 在 tokens 里直接写成 286（`tokens.ts:15`），没有运行时算 130*2.2 | 无 |
| B7 | README 未提及 `ringSegments=240` 之外的可调项对末段的影响 | `gauge.ts:367` 的 `min` 让**末段**无 overlap（角宽 1.125° 而非 1.405°） | 复刻圆环时需要一致，否则末端会多出一条 0.28° 的重叠边 |
| B8 | README §2.5 说页面底色 = `color(display-p3 0 0 0)` | `styles.css` 里是 `#000000`（等价）；但**浅色**页面 `styles.css` 用 `#f2f2f2`，原站实测是 `color(display-p3 0.949 0.95 0.949)`（`research/dark-theme.json:25`），二者在 p3 屏上不等价 | 浅色主题的页面底色是"近似"，不是精确复刻 |
| B9 | README §4.5 写"DPR 1/2/3 下数字锚点恒为 (143,226)" | 单一实现在 SVG 用户单位里恒为 226，与 DPR 无关（结论正确，但这是 viewBox 的必然结果，不是额外特性） | 无 |
| B10 | README 未提 | `setSpeed` 用**量化后**的 mbps 判 `>1000`，`setScaleMode` 用**未量化**的 `kbps/1000` 判 ⇒ 边界值（如 kbps=1000001）两处结论可能相反（见 §8.4） | 只在 1000 Mbps ±0.001 的极窄区间可见 |
| B11 | README §3.2 说微颤"振幅 min(1.1°, |ω|×0.0032)" | 一致；但 README 未提 `wobblePhase` 从不重置、`snap` 只清振幅 | 复刻相位需注意 |
| B12 | README 未提 | `reset()` 后 `springResetTween` 不被 `setTarget` 清除（`animator.ts:141-144` vs `117-135`） | spring 模式下"归位途中收到新样本"的行为是"先把归位跑完再去新目标" |
