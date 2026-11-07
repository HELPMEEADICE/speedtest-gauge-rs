"""Measure *our* Slint renders and diff them against the reference metrics.

复用 `tools/measure_reference.py` 的度量函数（radial scan / ink bbox / sweep end），
所以两边口径**逐行相同**：同样的背景阈值、同样的扫描角度、同样的采样盒。

用法：
    python tools/measure_render.py                 # 量 comparison/render/*.png 并对照
    python tools/measure_render.py --json          # 额外输出机器可读结果
    python tools/measure_render.py --dump          # 打印每项的原始实测值

验收容差（与任务书一致）：
    环内/外半径 ±0.4 uu | sweep 终点角 ±0.2° | 数字/单位行 ink bbox 每边 ±1.0 uu
    9 个刻度 ink 中心每点 ±1.0 uu
"""

from __future__ import annotations

import json
import math
import os
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
import measure_reference as mr  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
RENDER = ROOT / "comparison" / "render"
REF = ROOT / "comparison" / "ref-metrics"
# 原始参考图（只读基线，用于"同一度量两图对比"的稳健校验）
REF_PNG = Path(
    os.environ.get("SPEEDTEST_REPORT_DIR", str(ROOT.parents[2] / "speedtest" / "report"))
)

CENTRE = 143.0
LABELS = [0, 5, 10, 50, 100, 250, 500, 750, 1000]

# 状态 → (参考 json, 参与对照的指标类)
# gigabit 没有参考图（原站没出这一张）：环半径/刻度位置与 high 相同，sweep 与数字按规格单独核对。
PLAN = {
    "idle": ("local-idle.png", ("ring", "sweep", "number", "unit", "label"), None),
    "low": ("local-low.png", ("ring", "sweep", "number", "unit", "label"), None),
    "mid": ("local-mid.png", ("ring", "sweep", "number", "unit", "label"), None),
    "high": ("local-high.png", ("ring", "sweep", "number", "unit", "label"), None),
    "upload": ("local-upload.png", ("ring", "sweep", "number", "unit", "label"), None),
    "extended": (
        "local-extended.png",
        ("ring", "sweep", "number", "unit", "label"),
        None,
    ),
    "done": ("local-high.png", ("ring", "sweep", "number", "unit", "label"), None),
    # 原站没有 gigabit(1000) 这张图：只比几何（与 high 同布局），sweep 与规格值 135.0 对。
    "gigabit": ("local-high.png", ("ring",), 135.0),
}

TOL_RING = 0.4
TOL_SWEEP = 0.2
TOL_BOX = 1.0
TOL_LABEL = 1.0

# 只看 ±60°：这两个角度最先打到环、不会先撞刻度字（visual-targets.md §1）
RING_ANGLES = ("-60", "+60")


def measure(png: Path) -> dict:
    """与 measure_reference.main() 完全同口径的度量，返回 dict 而不是打印。"""
    img = Image.open(png).convert("RGB")
    out: dict[str, object] = {"file": png.name, "size": img.size, "scale": img.width / mr.UU}
    out["ring_radius_uu"] = {
        f"{a:+.0f}": mr.ring_scan(img, a) for a in (-100.0, -60.0, 0.0, 60.0, 100.0)
    }
    out["sweep_end_deg"] = mr.sweep_end_angle(img)
    out["number_ink"] = mr.ink_bbox(img, mr.bright, (100.0, 205.0, 190.0, 248.0))
    out["unit_row_ink"] = mr.ink_bbox(
        img, lambda p: mr.luma(p) > 60, (100.0, 248.0, 190.0, 275.0)
    )
    label_ink = {}
    for i, value in enumerate(LABELS):
        angle = -135.0 + i * 33.75
        rad = __import__("math").radians(angle - 90)
        lx = CENTRE + 97.0 * __import__("math").cos(rad)
        ly = CENTRE + 97.0 * __import__("math").sin(rad)
        label_ink[str(value)] = mr.ink_bbox(
            img, lambda p: mr.luma(p) > 60, (lx - 18.0, ly - 10.0, lx + 18.0, ly + 10.0)
        )
    out["label_ink"] = label_ink
    return out


def edge_delta(a, b) -> float:
    """两个 bbox 的最大单边差（uu）；任一缺失返回 inf。"""
    if a is None or b is None:
        return float("inf")
    return max(abs(x - y) for x, y in zip(a["bbox_uu"], b["bbox_uu"]))


def centre_delta(a, b) -> float:
    if a is None or b is None:
        return float("inf")
    return max(abs(x - y) for x, y in zip(a["centre_uu"], b["centre_uu"]))


def compare(
    name: str, ours: dict, ref: dict, kinds: tuple[str, ...], sweep_expected: float | None
) -> dict:
    res: dict[str, object] = {"state": name, "ok": True}

    ring_rows = []
    for a in RING_ANGLES:
        o = ours["ring_radius_uu"].get(a)
        r = ref["ring_radius_uu"].get(a)
        if o and r:
            d_in, d_out = abs(o[0] - r[0]), abs(o[1] - r[1])
            ok = d_in <= TOL_RING and d_out <= TOL_RING
            ring_rows.append((a, o, r, d_in, d_out, ok))
            res["ok"] &= ok
    res["ring"] = ring_rows

    if sweep_expected is not None:
        o = ours["sweep_end_deg"]
        if o is None:
            res["sweep"] = (None, sweep_expected, float("inf"), False)
            res["ok"] = False
        else:
            d = abs(o - sweep_expected)
            res["sweep"] = (o, sweep_expected, d, d <= TOL_SWEEP)
            res["ok"] &= d <= TOL_SWEEP
    elif "sweep" in kinds:
        o, r = ours["sweep_end_deg"], ref["sweep_end_deg"]
        if o is None and r is None:
            res["sweep"] = (None, None, 0.0, True)
        elif o is None or r is None:
            res["sweep"] = (o, r, float("inf"), False)
            res["ok"] = False
        else:
            d = abs(o - r)
            res["sweep"] = (o, r, d, d <= TOL_SWEEP)
            res["ok"] &= d <= TOL_SWEEP
    else:
        res["sweep"] = (ours["sweep_end_deg"], None, None, True)

    if "number" in kinds:
        d = edge_delta(ours["number_ink"], ref["number_ink"])
        res["number"] = (ours["number_ink"], ref["number_ink"], d, d <= TOL_BOX)
        res["ok"] &= d <= TOL_BOX
    else:
        res["number"] = (ours["number_ink"], None, None, True)

    if "unit" in kinds:
        d = edge_delta(ours["unit_row_ink"], ref["unit_row_ink"])
        res["unit"] = (ours["unit_row_ink"], ref["unit_row_ink"], d, d <= TOL_BOX)
        res["ok"] &= d <= TOL_BOX
    else:
        res["unit"] = (ours["unit_row_ink"], None, None, True)

    if "label" in kinds:
        worst, worst_key = 0.0, ""
        for key in ref["label_ink"]:
            d = centre_delta(ours["label_ink"].get(key), ref["label_ink"].get(key))
            if d > worst:
                worst, worst_key = d, key
        res["label"] = (worst, worst_key, worst <= TOL_LABEL)
        res["ok"] &= worst <= TOL_LABEL
    else:
        res["label"] = (None, "", True)

    return res


def fmt(v, nd=2):
    return "—" if v is None else f"{v:.{nd}f}"


def edge_scan_50(img: Image.Image, angle_deg: float, mid_r: float = 130.0):
    """50% 交叉法量环的内/外半径：对"AA 覆盖曲线"不敏感，两图可直接对比。

    `measure_reference.ring_scan` 用 `luma > 8` 阈值——软件渲染器与 Chromium 的
    抗锯齿覆盖曲线不同，低覆盖像素的灰度不同，读数会差 0.3–0.5 uu。这个函数取
    "半径方向 luma 峰值的一半"作为边沿，能把两个渲染器的几何对齐比较。
    """
    s = img.width / mr.UU
    cx = cy = img.width / 2
    rad = math.radians(angle_deg - 90)
    samples = []
    r = mid_r * 0.6
    while r <= mid_r * 1.25:
        x = int(round(cx + r * s * math.cos(rad)))
        y = int(round(cy + r * s * math.sin(rad)))
        if 0 <= x < img.width and 0 <= y < img.height:
            samples.append((r, mr.luma(img.getpixel((x, y))[:3])))
        r += 0.02
    if not samples:
        return None
    peak = max(v for _, v in samples)
    half = peak * 0.5
    inner = next((r for r, v in samples if v >= half), None)
    outer = max((r for r, v in samples if v >= half), default=None)
    return (inner, outer)


def report_ring50(rows: list[dict]) -> None:
    """同一度量跑两图（我们的 PNG vs 原始参考 PNG）——几何是否真的对齐。"""
    if not REF_PNG.is_dir():
        print(f"\n[ring50] 跳过：找不到原始参考图目录 {REF_PNG}")
        return
    print("\n环半径（50% 交叉法，同一度量跑两图；避免 luma>8 阈值的 AA 偏差）")
    print(f"{'状态':<10}{'角度':<8}{'参考 内/外':<22}{'我们 内/外':<22}{'Δ内':<8}{'Δ外':<8}")
    for r in rows:
        name = r["state"]
        ref_png = REF_PNG / (PLAN[name][0] if name != "gigabit" else "local-high.png")
        ours_png = RENDER / f"{name}.png"
        if not ref_png.is_file() or not ours_png.is_file():
            continue
        a_img = Image.open(ref_png).convert("RGB")
        b_img = Image.open(ours_png).convert("RGB")
        for ang in (-60, 60):
            a = edge_scan_50(a_img, ang)
            b = edge_scan_50(b_img, ang)
            if not (a and b):
                continue
            print(
                f"{name:<10}{ang:<8}{fmt(a[0])+'/'+fmt(a[1]):<22}"
                f"{fmt(b[0])+'/'+fmt(b[1]):<22}"
                f"{abs(a[0]-b[0]):<8.2f}{abs(a[1]-b[1]):<8.2f}"
            )


def main(argv: list[str]) -> int:
    rows = []
    raw = {}
    for name, (ref_png_name, kinds, sweep_expected) in PLAN.items():
        png = RENDER / f"{name}.png"
        if not png.is_file():
            print(f"[跳过] {png} 不存在")
            continue
        ours = measure(png)
        raw[name] = ours
        ref = json.loads(
            (REF / ref_png_name.replace(".png", ".json")).read_text(encoding="utf-8")
        )
        rows.append(compare(name, ours, ref, kinds, sweep_expected))

    print("=" * 118)
    print(
        f"{'状态':<10}{'环内/外 (uu)':<30}{'sweep 终点角 (°)':<30}"
        f"{'数字 bbox 最大Δ':<16}{'单位行 bbox 最大Δ':<18}{'刻度中心 最大Δ':<14}{'结论'}"
    )
    print("=" * 118)
    for r in rows:
        ring = " ".join(
            f"{a}:{fmt(o[0])}/{fmt(o[1])}(Δ{max(di,do):.2f})"
            for (a, o, _ref, di, do, _ok) in r["ring"]
        )
        sw_o, sw_r, sw_d, _ = r["sweep"]
        if sw_r is None:
            sweep = f"{fmt(sw_o)}/参考—"
        elif sw_o is None and sw_r is None:
            sweep = "无亮弧/无亮弧"
        else:
            sweep = f"{fmt(sw_r)}/{fmt(sw_o)}(Δ{fmt(sw_d)})"
        num = "—" if r["number"][2] is None else f"{r['number'][2]:.2f}"
        unit = "—" if r["unit"][2] is None else f"{r['unit'][2]:.2f}"
        lab = "—" if r["label"][0] is None else f"{r['label'][0]:.2f}({r['label'][1]})"
        print(
            f"{r['state']:<10}{ring:<30}{sweep:<30}{num:<16}{unit:<18}{lab:<14}"
            f"{'PASS' if r['ok'] else 'FAIL'}"
        )
    print("=" * 118)

    failed = [r["state"] for r in rows if not r["ok"]]
    print("全部达标" if not failed else f"未达标：{', '.join(failed)}")

    report_ring50(rows)

    if "--dump" in argv:
        print("\n逐项实测（我们）:")
        for name, m in raw.items():
            print(f"\n[{name}]")
            for a in RING_ANGLES:
                v = m["ring_radius_uu"].get(a)
                if v:
                    print(f"  ring {a}: inner {v[0]:.2f}  outer {v[1]:.2f}")
            if m["sweep_end_deg"] is not None:
                print(f"  sweep_end {m['sweep_end_deg']:.2f}")
            for key in ("number_ink", "unit_row_ink"):
                v = m[key]
                if v:
                    print(f"  {key}: bbox {[round(x, 2) for x in v['bbox_uu']]}")
            for k, v in m["label_ink"].items():
                if v:
                    print(f"  label {k:>4}: centre {[round(x, 2) for x in v['centre_uu']]}")

    if "--json" in argv:
        print(json.dumps([{k: v for k, v in r.items()} for r in rows], indent=2, default=str))

    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
