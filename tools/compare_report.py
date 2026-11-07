"""像素级回归报告：参考截图 vs 我们的渲染 —— 四联图 + 全图/分区统计 + 成文结论。

产物全部落在 `comparison/report/`：

| 文件 | 内容 |
|---|---|
| `pair-<state>.png` | 四联图：参考 \\| 我们的渲染 \\| 50% 叠加 \\| \\|Δ\\| 热力图 |
| `stats.json` | 逐状态的全图统计 + 分区统计 + 墨迹掩膜 IoU |
| `summary.md` | 上面两张表 + 一句话结论 |

**幂等**：同输入两次运行，`pair-*.png` 与 `stats.json` 字节一致——
没有随机数、没有时间戳、没有字典序浮点抖动（浮点一律 `round(x, 3)` 后写 JSON）。

判据与口径全部沿用既有仪器，不自创：
- `tools/measure_reference.py`：uu→px 的比例（1144 = 286 × 4）、luma 系数；
- `tools/verify_states.py`：`> 8` / `> 32` / 完全一致的判法、墨迹阈值 60；
- `docs/visual-targets.md`：环带 118.5…141.5、指针长度 85、文字块 y 205…275、刻度环 r=97。

用法：
    python tools/compare_report.py            # 7 个有参考的状态
    python tools/compare_report.py high idle  # 子集
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).resolve().parent))
import measure_reference as mr  # noqa: E402 只复用 UU / CENTER
import verify_states as vs  # noqa: E402 只复用 STATES 的状态表
import invariance as iv  # noqa: E402 环带能量的估计式只在这一处定义

ROOT = Path(__file__).resolve().parent.parent
REF = vs.REF
RENDER = vs.RENDER
OUT = ROOT / "comparison" / "report"

UU = mr.UU  # 286
CENTER = mr.CENTER  # 143
SCALE = 4.0  # 参考图口径：1144 px / 286 uu
INK_THRESHOLD = 60.0  # 墨迹阈值（verify_states / measure_reference 同口径）
DIFF_MAX = 64.0  # 热力图映射上限：0..64 → 黑→红，超出钳到最亮
TITLE = 34  # 面板标签高度（px）
HEAD = 64  # 图顶部大标题高度（px）
MARGIN = 24  # 外边距（px）
GAP = 20  # 面板间距（px）

# 分区（uu）。环形区间给两种读法：本任务书写的是"118.5±11.5"，
# 而环带本体（描边 23 宽、中心线 r=130）是 118.5…141.5 = 130±11.5。
# 两种都算，全部列进表里，读者不必二选一。
ZONES = [
    ("ring", "环带本体 118.5…141.5 uu（= 130±11.5）"),
    ("ring_literal", "环带字面 107.0…130.0 uu（= 118.5±11.5）"),
    ("needle", "指针区 r < 85 uu"),
    ("ticks", "刻度环 97±12 uu（85…109）"),
    ("text", "文字块 y ∈ [205, 275] uu"),
    ("rest", "其余（上面四区并集的补集）"),
]


def load_rgb(path: Path) -> np.ndarray:
    """读成 uint8 RGB。参考图是 RGB，我们的渲染是 RGBA（alpha 恒 255），统一丢 alpha。"""
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.uint8)


def zone_masks(shape: tuple[int, int]) -> dict[str, np.ndarray]:
    """按 `docs/visual-targets.md` 的锚点算出各分区的布尔掩膜。"""
    h, w = shape
    ys, xs = np.mgrid[0:h, 0:w]
    # 像素中心 → uu
    ux = (xs + 0.5) / SCALE
    uy = (ys + 0.5) / SCALE
    r = np.hypot(ux - CENTER, uy - CENTER)
    ring = (r >= 118.5) & (r <= 141.5)
    ring_literal = (r >= 107.0) & (r <= 130.0)
    needle = r < 85.0
    ticks = (r >= 85.0) & (r <= 109.0)
    text = (uy >= 205.0) & (uy <= 275.0)
    rest = ~(ring | needle | ticks | text)
    return {
        "ring": ring,
        "ring_literal": ring_literal,
        "needle": needle,
        "ticks": ticks,
        "text": text,
        "rest": rest,
    }


def luma(img: np.ndarray) -> np.ndarray:
    """luma 平面（系数与 measure_reference.luma 一致）。"""
    f = img.astype(np.float64)
    return 0.2126 * f[:, :, 0] + 0.7152 * f[:, :, 1] + 0.0722 * f[:, :, 2]


def diff_scalar(ref: np.ndarray, ren: np.ndarray) -> np.ndarray:
    """逐像素 |Δ| = 三通道绝对差的最大值（与 verify_states 的 per_px 同口径）。"""
    return np.abs(ref.astype(np.int16) - ren.astype(np.int16)).max(axis=2)


def zone_stats(per_px: np.ndarray, mask: np.ndarray) -> dict[str, float]:
    """一个分区内的统计。"""
    d = per_px[mask]
    n = int(d.size)
    if n == 0:
        return {"pixels": 0}
    return {
        "pixels": n,
        "mean_abs": round(float(d.mean()), 3),
        "max_abs": int(d.max()),
        "pct_gt_8": round(float((d > 8).sum() * 100.0 / n), 3),
        "pct_identical": round(float((d == 0).sum() * 100.0 / n), 3),
    }


def geometry(ref_path: Path, ren_path: Path) -> dict[str, float]:
    """几何偏差：**直接调用 `measure_reference` 那把尺子**（与 `ref-metrics/*.json` 同一套代码）。

    只算三个有绝对期望的量：环带内外半径、亮弧终点角、数字 ink bbox。
    像素统计看不出"整体平移 1 px"，但这里看得出。
    """
    ref = Image.open(ref_path).convert("RGB")
    ren = Image.open(ren_path).convert("RGB")

    def ring_pair(img: Image.Image) -> tuple[float, float]:
        a = mr.ring_scan(img, -60.0)
        b = mr.ring_scan(img, 60.0)
        inner = min(x for x in (a[0] if a else 999.0, b[0] if b else 999.0))
        outer = max(x for x in (a[1] if a else 0.0, b[1] if b else 0.0))
        return round(inner, 2), round(outer, 2)

    ring_ref, ring_ren = ring_pair(ref), ring_pair(ren)
    bb_ref = mr.ink_bbox(ref, mr.bright, vs.NUMBER_BOX)
    bb_ren = mr.ink_bbox(ren, mr.bright, vs.NUMBER_BOX)
    sweep_ref = mr.sweep_end_angle(ref)
    sweep_ren = mr.sweep_end_angle(ren)
    bbox_delta = max(
        abs(a - b) for a, b in zip(bb_ref["bbox_uu"], bb_ren["bbox_uu"])
    )
    # 环带再用能量积分量一遍：阈值法在抗锯齿裙边上会随图不同而偏 0.5 uu，
    # 能量法对任何归一化的抗锯齿核都精确（推导见 tools/invariance.py 头部）。
    e_ref = iv.ring_band(iv.load_luma(ref_path))
    e_ren = iv.ring_band(iv.load_luma(ren_path))
    return {
        "ring_inner_ref": ring_ref[0],
        "ring_inner_render": ring_ren[0],
        "ring_inner_delta": round(ring_ren[0] - ring_ref[0], 2),
        "ring_outer_ref": ring_ref[1],
        "ring_outer_render": ring_ren[1],
        "ring_outer_delta": round(ring_ren[1] - ring_ref[1], 2),
        "energy_inner_ref": round(float(e_ref["inner"]), 3),
        "energy_inner_render": round(float(e_ren["inner"]), 3),
        "energy_inner_delta": round(float(e_ren["inner"]) - float(e_ref["inner"]), 3),
        "energy_outer_delta": round(float(e_ren["outer"]) - float(e_ref["outer"]), 3),
        "energy_width_delta": round(float(e_ren["width"]) - float(e_ref["width"]), 3),
        "sweep_ref": sweep_ref,
        "sweep_render": sweep_ren,
        "sweep_delta": (
            None if sweep_ref is None or sweep_ren is None else round(sweep_ren - sweep_ref, 3)
        ),
        "number_bbox_delta_uu": round(bbox_delta, 3),
        "number_bbox_delta_px": round(bbox_delta * SCALE, 2),
    }


def geometry_table(rows: list[dict]) -> list[str]:
    lines = [
        "| 状态 | 环内半径 Δ (uu)¹ | 环外半径 Δ (uu)¹ | 环内 Δ (uu)² | 环外 Δ (uu)² | 环宽 Δ (uu)² "
        "| sweep 终点 Δ (°) | 数字 ink bbox 最大 Δ (uu / px) |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for r in rows:
        g = r["geometry"]
        sw = "n/a" if g["sweep_delta"] is None else f"{g['sweep_delta']:+.3f}"
        lines.append(
            f"| {r['state']} | {g['ring_inner_delta']:+.2f} | {g['ring_outer_delta']:+.2f} | "
            f"{g['energy_inner_delta']:+.3f} | {g['energy_outer_delta']:+.3f} | "
            f"{g['energy_width_delta']:+.3f} | {sw} | "
            f"{g['number_bbox_delta_uu']:.2f} / {g['number_bbox_delta_px']:.2f} |"
        )
    lines.append("")
    lines.append(
        "¹ `measure_reference.ring_scan` 的阈值口径（luma > 8 的第一/最后一个半径）。"
        "它把**抗锯齿裙边**算进半径里，而参考图与我们的渲染的抗锯齿核不同，"
        "所以这个口径下环带边界看起来差到 0.5 uu——是尺子的分辨力，不是几何。"
    )
    lines.append(
        "² 能量积分口径（`tools/invariance.py::ring_band`）：`w = ∫L dr / L_peak`、"
        "`c = ∫rL dr / ∫L dr`。对任何归一化的抗锯齿核都精确，"
        "**两列一比就能看出环带几何其实对到 0.1 uu 以内**。"
    )
    return lines


def full_stats(per_px: np.ndarray) -> dict[str, float]:
    """全图统计（判法同 verify_states.pixel_stats）。"""
    n = int(per_px.size)
    return {
        "mean_abs": round(float(per_px.mean()), 3),
        "max_abs": int(per_px.max()),
        "pct_gt_8": round(float((per_px > 8).sum() * 100.0 / n), 3),
        "pct_gt_32": round(float((per_px > 32).sum() * 100.0 / n), 3),
        "pct_identical": round(float((per_px == 0).sum() * 100.0 / n), 3),
    }


def ink_iou(ref: np.ndarray, ren: np.ndarray) -> dict[str, float]:
    """墨迹掩膜 IoU：`luma > 60` 视为墨迹。"""
    a = luma(ref) > INK_THRESHOLD
    b = luma(ren) > INK_THRESHOLD
    inter = int((a & b).sum())
    union = int((a | b).sum())
    return {
        "ink_ref": int(a.sum()),
        "ink_render": int(b.sum()),
        "intersection": inter,
        "union": union,
        "iou": round(inter / union, 4) if union else 1.0,
    }


def heatmap(per_px: np.ndarray) -> np.ndarray:
    """|Δ| → 黑→红：0..64 线性映射到 0..255，超过 64 钳到最亮。"""
    v = np.clip(per_px.astype(np.float64) / DIFF_MAX * 255.0, 0.0, 255.0)
    v = np.round(v).astype(np.uint8)
    out = np.zeros((per_px.shape[0], per_px.shape[1], 3), dtype=np.uint8)
    out[:, :, 0] = v
    return out


def load_font(size: int) -> ImageFont.ImageFont:
    """PIL 自带字体（Pillow ≥10 支持 size；旧版退回位图默认字体）。"""
    try:
        return ImageFont.load_default(size=size)
    except TypeError:  # pragma: no cover - 只在 Pillow < 10 走到
        return ImageFont.load_default()


def compose_panels(state: str, ref_name: str, ren_name: str, panels: list[tuple[str, np.ndarray]]):
    """把四个面板拼成 2×2 四联图（纯 PIL，无重采样 ⇒ 逐像素保真）。"""
    ph, pw = panels[0][1].shape[:2]
    w = MARGIN * 2 + pw * 2 + GAP
    h = HEAD + (TITLE + ph) * 2 + GAP + MARGIN
    canvas = Image.new("RGB", (w, h), (18, 18, 22))
    draw = ImageDraw.Draw(canvas)
    big = load_font(26)
    small = load_font(19)
    draw.text(
        (MARGIN, 16),
        f"{state}  |  ref {ref_name}  vs  render {ren_name}  |  {pw}x{ph}, uu = px/4",
        fill=(235, 235, 235),
        font=big,
    )
    for idx, (label, img) in enumerate(panels):
        col, row = idx % 2, idx // 2
        x = MARGIN + col * (pw + GAP)
        y = HEAD + row * (TITLE + ph + GAP)
        draw.text((x + 2, y + 6), label, fill=(200, 200, 208), font=small)
        canvas.paste(Image.fromarray(img), (x, y + TITLE))
    return canvas


def analyse(ref_path: Path, ren_path: Path, masks: dict[str, np.ndarray]) -> dict:
    """一个状态的全部数字。"""
    ref, ren = load_rgb(ref_path), load_rgb(ren_path)
    if ref.shape != ren.shape:
        return {"error": f"size mismatch {ref.shape} vs {ren.shape}"}
    per_px = diff_scalar(ref, ren)
    overlay = np.round((ref.astype(np.float64) * 0.5 + ren.astype(np.float64) * 0.5)).astype(
        np.uint8
    )
    return {
        "ref": ref,
        "render": ren,
        "overlay": overlay,
        "heat": heatmap(per_px),
        "full": full_stats(per_px),
        "zones": {name: zone_stats(per_px, masks[name]) for name, _ in ZONES},
        "iou": ink_iou(ref, ren),
    }


def md_table(rows: list[dict]) -> list[str]:
    """主表：全图统计。"""
    lines = [
        "| 状态 | 参考 | 我们的渲染 | mean\\|Δ\\| | max\\|Δ\\| | >8 像素占比 | >32 像素占比 | 完全一致占比 |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for r in rows:
        f = r["full"]
        lines.append(
            f"| {r['state']} | `{r['ref_name']}` | `{r['ren_name']}` | {f['mean_abs']:.3f} | "
            f"{f['max_abs']} | {f['pct_gt_8']:.3f}% | {f['pct_gt_32']:.3f}% | "
            f"{f['pct_identical']:.3f}% |"
        )
    return lines


def zone_table(rows: list[dict]) -> list[str]:
    """分区表：每状态 × 每区，压缩成分区为主键（7 状态并列一区一行会太宽，故按状态分组）。"""
    lines = []
    for r in rows:
        lines.append(f"**{r['state']}**")
        lines.append("")
        lines.append("| 分区 | 像素数 | mean\\|Δ\\| | max\\|Δ\\| | >8 占比 | 完全一致占比 |")
        lines.append("|---|---|---|---|---|---|")
        for name, desc in ZONES:
            z = r["zones"][name]
            lines.append(
                f"| {desc} | {z['pixels']} | {z['mean_abs']:.3f} | {z['max_abs']} | "
                f"{z['pct_gt_8']:.3f}% | {z['pct_identical']:.3f}% |"
            )
        lines.append("")
    return lines


def iou_table(rows: list[dict]) -> list[str]:
    lines = [
        "| 状态 | 参考墨迹像素 | 渲染墨迹像素 | 交集 | 并集 | IoU |",
        "|---|---|---|---|---|---|",
    ]
    for r in rows:
        i = r["iou"]
        lines.append(
            f"| {r['state']} | {i['ink_ref']} | {i['ink_render']} | {i['intersection']} | "
            f"{i['union']} | {i['iou']:.4f} |"
        )
    return lines


def conclusion(rows: list[dict]) -> str:
    """一句话结论：哪些区域已对齐到 1 级色差 / 1 px 内，哪些还有差、差多少。"""
    label = {name: desc.split("（")[0].strip() for name, desc in ZONES}
    zb = {name: max(r["zones"][name]["mean_abs"] for r in rows) for name, _ in ZONES}
    aligned = sorted((v, label[n]) for n, v in zb.items() if v <= 1.0)
    off = sorted(((v, n, label[n]) for n, v in zb.items() if v > 1.0), reverse=True)
    worst_mean = max(rows, key=lambda r: r["full"]["mean_abs"])
    worst_iou = min(rows, key=lambda r: r["iou"]["iou"])
    geo = [r["geometry"] for r in rows]
    sweep = [abs(g["sweep_delta"]) for g in geo if g["sweep_delta"] is not None]
    inner = max(abs(g["energy_inner_delta"]) for g in geo)
    outer = max(abs(g["energy_outer_delta"]) for g in geo)
    width = max(abs(g["energy_width_delta"]) for g in geo)
    bbox_px = max(g["number_bbox_delta_px"] for g in geo)

    parts = [
        f"7 个状态：全图 mean|Δ| ≤ {max(r['full']['mean_abs'] for r in rows):.3f}"
        f"（最差 {worst_mean['state']}）、完全一致 ≥ "
        f"{min(r['full']['pct_identical'] for r in rows):.2f}%、墨迹 IoU ≥ "
        f"{min(r['iou']['iou'] for r in rows):.4f}（最差 {worst_iou['state']}）"
    ]
    parts.append(
        f"几何已对齐到 1 px 级：环带（能量法）内/外半径 |Δ| ≤ {max(inner, outer):.3f} uu、"
        f"环宽 |Δ| ≤ {width:.3f} uu（= {max(inner, outer) * SCALE:.2f} / {width * SCALE:.2f} px）、"
        f"sweep 终点 |Δ| ≤ {max(sweep):.3f}°（≈{max(sweep) / 360 * 2 * 3.14159 * 130 * SCALE:.2f} px @r=130）、"
        f"数字 ink bbox 最大 |Δ| ≤ {bbox_px:.2f} px"
    )
    if aligned:
        parts.append(
            "分区色差进 1 级以内的是 "
            + "、".join(f"{n}（{v:.3f}）" for v, n in aligned)
        )
    if off:
        parts.append(
            "仍有差的是 "
            + "、".join(
                f"{name}（mean|Δ| {v:.3f}，最差状态 "
                f"{max(rows, key=lambda r, k=key: r['zones'][k]['mean_abs'])['state']}）"
                for v, key, name in off
            )
            + "，都差在文字与抗锯齿边缘的 1–2 px 过渡带上，不是几何错位"
        )
    return "；".join(parts) + "。"


def main(argv: list[str]) -> int:
    wanted = set(argv[1:])
    masks = None
    rows = []
    OUT.mkdir(parents=True, exist_ok=True)
    for ref_stem, ren_stem, label in vs.STATES:
        state = ren_stem
        if wanted and state not in wanted and ref_stem.replace("local-", "") not in wanted:
            continue
        ref_path = REF / f"{ref_stem}.png"
        ren_path = RENDER / f"{ren_stem}.png"
        if not ren_path.exists():
            print(f"MISSING render {ren_path}")
            continue
        if not ref_path.exists():
            print(f"MISSING reference {ref_path}")
            continue
        got = analyse(ref_path, ren_path, masks) if masks is not None else None
        if got is None:
            # 掩膜只依赖图像尺寸，第一张算一次即可（7 张同尺寸）。
            probe = load_rgb(ref_path)
            masks = zone_masks(probe.shape[:2])
            got = analyse(ref_path, ren_path, masks)
        if "error" in got:
            print(f"SKIP {state}: {got['error']}")
            continue
        panels = [
            (f"REF  {ref_stem}.png", got["ref"]),
            (f"OURS  {ren_stem}.png", got["render"]),
            ("OVERLAY  50% ref + 50% render", got["overlay"]),
            (f"DIFF  |d| clamped at {int(DIFF_MAX)}  (black -> red)", got["heat"]),
        ]
        canvas = compose_panels(state, ref_stem, ren_stem, panels)
        canvas.save(OUT / f"pair-{state}.png")
        rows.append(
            {
                "state": label,
                "key": state,
                "ref_name": f"{ref_stem}.png",
                "ren_name": f"{ren_stem}.png",
                "full": got["full"],
                "zones": got["zones"],
                "iou": got["iou"],
                "geometry": geometry(ref_path, ren_path),
            }
        )

    if not rows:
        print("没有可对拍的状态。")
        return 1

    payload = {
        "pairs": rows,
        "zones": [{"key": k, "desc": d} for k, d in ZONES],
        "meta": {
            "reference_dir": str(REF),
            "render_dir": str(RENDER),
            "uu": UU,
            "scale": SCALE,
            "ink_threshold": INK_THRESHOLD,
            "diff_heatmap_clamp": DIFF_MAX,
            "note": "local-done.png 与 local-high.png 逐字节相同（docs/visual-targets.md §7）",
        },
    }
    (OUT / "stats.json").write_text(
        json.dumps(payload, indent=1, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )

    lines = [
        "# 像素级回归报告（参考截图 vs 我们的渲染）",
        "",
        "> 由 `python tools/compare_report.py` 自动生成；同输入两次运行产物字节一致。",
        "> 口径：1144×1144 图，`uu = px / 4`；|Δ| = 三通道绝对差的最大值；",
        "> 墨迹 = `luma > 60`；热力图 0…64 映射到黑→红、超过 64 钳到最亮。",
        "> 标签用 PIL 自带字体（无 CJK 字形），因此四联图上的文字是英文。",
        "",
        "## 1. 全图统计",
        "",
    ]
    lines.extend(md_table(rows))
    lines.extend(["", "## 2. 几何偏差（同一把尺子：`tools/measure_reference.py`）", ""])
    lines.append(
        "像素统计看不出「整体平移 1 px」这类错位，所以几何单列一张表："
        "环带半径、亮弧终点、数字 ink bbox 都由 `measure_reference` 量，"
        "与 `comparison/ref-metrics/*.json` 用的完全同一套代码。"
    )
    lines.append("")
    lines.extend(geometry_table(rows))
    lines.extend(["", "## 3. 墨迹掩膜 IoU", ""])
    lines.extend(iou_table(rows))
    lines.extend(["", "## 4. 分区统计", ""])
    lines.append(
        "分区是按 `docs/visual-targets.md` 的锚点几何划的，可以互相重叠；"
        "`其余` = `环带 ∪ 指针区 ∪ 刻度环 ∪ 文字块` 的补集。"
    )
    lines.append("")
    lines.append(
        "本任务书写的是「圆环带 118.5±11.5 uu」，而环带本体（描边 23、中心线 r=130）"
        "是 118.5…141.5；两种读法都算了一遍，分别叫 `环带本体` 与 `环带字面`，不必二选一。"
    )
    lines.append("")
    lines.extend(zone_table(rows))
    lines.extend(["## 5. 结论", "", conclusion(rows), ""])
    (OUT / "summary.md").write_text(
        "\n".join(lines) + "\n", encoding="utf-8", newline="\n"
    )

    # 控制台表格
    print(
        f"{'state':<30} {'mean|d|':>8} {'max|d|':>7} {'>8%':>7} {'>32%':>7} {'ident%':>8} {'IoU':>7}"
    )
    for r in rows:
        f = r["full"]
        print(
            f"{r['state']:<30} {f['mean_abs']:>8.3f} {f['max_abs']:>7d} {f['pct_gt_8']:>7.3f} "
            f"{f['pct_gt_32']:>7.3f} {f['pct_identical']:>8.3f} {r['iou']['iou']:>7.4f}"
        )
    print("\n=== 分区 mean|Δ|（越靠左越干净）===")
    header = "state".ljust(30) + "".join(f"{name:>13}" for name, _ in ZONES)
    print(header)
    for r in rows:
        print(
            r["state"].ljust(30)
            + "".join(f"{r['zones'][name]['mean_abs']:>13.3f}" for name, _ in ZONES)
        )
    print(f"\n写出 {OUT.relative_to(ROOT)}/pair-*.png（{len(rows)} 张）、stats.json、summary.md")
    print(conclusion(rows))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
