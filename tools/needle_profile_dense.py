"""指针轴 0.5uu 密集扫描：接缝到底有多深、在哪个相位。

`tools/probe_colors.py` 只沿轴采 25 个点（步长 3uu），间距恰好接近一个切片宽度
（85/32 = 2.66uu），**会漏掉最坏相位**：接缝是"共享边落在像素中间"的覆盖率效应，
只有在边界附近连续采样才看得到。本脚本按 0.5uu 步长（1144px 图上 = 2px）扫过整条
指针轴，因此能稳定抓到 `c ≈ 0.5` 的那个像素。

两种对照口径：

  * `--ref local-high.png`：端到端（我们的渲染 vs 浏览器参考图），含参考图自身的噪声；
  * `--target dark|light`：解析目标（`needle_stops_u` 的线性 stop 插值），只含我们的
    结构性误差 —— 接缝的唯一干净口径。

用法:

    python tools/needle_profile_dense.py                     # 默认 high 状态：render vs ref
    python tools/needle_profile_dense.py --profile            # 附逐点剖面（贴报告用）
    python tools/needle_profile_dense.py --theme light        # needle-light.png vs 解析目标
    python tools/needle_profile_dense.py --sweep              # 扫 comparison/evidence/*/ 出对照表

`--sweep` 读每个 tag 目录里的 `high.png` / `needle-dark.png` / `needle-light.png`，
以及可选的 `median.txt`（单帧中位耗时，单位 ms，由 `cargo run --example shots` 的
stdout 抄进来）。
"""

from __future__ import annotations

import argparse
import math
import os
import sys
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
REF_DIR = Path(
    os.environ.get("SPEEDTEST_REPORT_DIR", str(ROOT.parents[2] / "speedtest" / "report"))
)
RENDER = ROOT / "comparison" / "render"
SWEEP = ROOT / "comparison" / "evidence"

UU = 286.0
C = 143.0
# high 状态的指针角（= ref-metrics/local-high.json 的 sweep_end_deg = 116.40；
# 渲染侧表的真值是 116.37，两者差 0.03° ⇒ 在 r=84 处横向偏移 0.04uu，可忽略）。
ANGLE = 116.37
NEEDLE_LEN = 85.0
TIP_ALPHA = 0.6

# 判定阈值（hard gate）
GATE = 4.0


# ---------------------------------------------------------------------------
# 采样
# ---------------------------------------------------------------------------

def load(path: Path) -> Image.Image:
    return Image.open(path).convert("RGB")


def luma(p) -> float:
    return 0.2126 * float(p[0]) + 0.7152 * float(p[1]) + 0.0722 * float(p[2])


def axis_point(r: float, angle: float = ANGLE) -> tuple[float, float]:
    rad = math.radians(angle - 90.0)
    return C + r * math.cos(rad), C + r * math.sin(rad)


def sample(img: Image.Image, r: float, angle: float = ANGLE) -> float:
    """沿指针轴取距离圆心 r 处的 luma（最近像素，与渲染器的像素网格一致）。"""
    s = img.width / UU
    x, y = axis_point(r, angle)
    px = min(max(int(round(x * s)), 0), img.width - 1)
    py = min(max(int(round(y * s)), 0), img.height - 1)
    return luma(img.getpixel((px, py)))


def samples(img: Image.Image, r0: float, r1: float, step: float) -> list[tuple[float, float]]:
    n = int(round((r1 - r0) / step)) + 1
    return [(r0 + i * step, sample(img, r0 + i * step)) for i in range(n)]


# ---------------------------------------------------------------------------
# 解析目标
# ---------------------------------------------------------------------------

def target_dark(r: float) -> float:
    """dark：白 needle over 黑 page ⇒ luma = 255·0.6·(r/85)。"""
    return 255.0 * TIP_ALPHA * (r / NEEDLE_LEN)


def target_light(r: float) -> float:
    """light：三个不透明 stop 沿 u = 1 − r/85 分段线性（灰度，luma = 通道值）。"""
    stops = [(0.0, 255.0), (0.58, 164.0), (1.0, 34.0)]  # u, gray
    u = 1.0 - r / NEEDLE_LEN
    u = min(max(u, 0.0), 1.0)
    for (u0, c0), (u1, c1) in zip(stops, stops[1:]):
        if u0 <= u <= u1:
            k = 0.0 if u1 == u0 else (u - u0) / (u1 - u0)
            return c0 + (c1 - c0) * k
    return stops[-1][1]


TARGETS = {"dark": target_dark, "light": target_light}


# ---------------------------------------------------------------------------
# 统计
# ---------------------------------------------------------------------------

def dense_stats(render: Image.Image, other: Image.Image | None, target) -> dict:
    """沿轴 0.5uu 扫描，返回 max|Δ| / mean|Δ| / 最坏点 / >4、>8 的样本数。"""
    step = 0.5
    r0, r1 = 2.0, 84.0
    rows = []
    for r, _ in samples(render, r0, r1, step):
        got = sample(render, r)
        want = sample(other, r) if other is not None else target(r)
        rows.append((r, want, got, got - want))
    diffs = [abs(d) for *_, d in rows]
    worst = max(rows, key=lambda t: abs(t[3]))
    return {
        "n": len(rows),
        "max": max(diffs),
        "mean": sum(diffs) / len(diffs),
        "worst_r": worst[0],
        "worst_d": worst[3],
        "gt4": sum(1 for d in diffs if d > GATE),
        "gt8": sum(1 for d in diffs if d > 8.0),
        "rows": rows,
    }


def full_stats(ref: Image.Image, ren: Image.Image) -> dict:
    """与 tools/verify_states.py::pixel_stats 同口径的全图统计。"""
    a = np.asarray(ref, dtype=np.int16)
    b = np.asarray(ren, dtype=np.int16)
    if a.shape != b.shape:
        return {"error": f"size mismatch {a.shape} vs {b.shape}"}
    d = np.abs(a - b)
    per_px = d.max(axis=2)
    total = per_px.size
    return {
        "mean": float(d.mean()),
        "max": int(d.max()),
        "gt8": float((per_px > 8).sum() * 100.0 / total),
        "ident": float((per_px == 0).sum() * 100.0 / total),
    }


# ---------------------------------------------------------------------------
# 输出
# ---------------------------------------------------------------------------

def print_profile(ref_name: str, ref_img: Image.Image, ren: Image.Image, target) -> None:
    print(f"\n== 逐点剖面（{ref_name} vs 我们，步长 0.5uu，角度 {ANGLE}°）==")
    print(f"{'r':>6} {'期望':>8} {'我们':>8} {'Δ':>8}")
    for r, want, got, d in dense_stats(ren, ref_img, target)["rows"]:
        flag = "  <<<" if abs(d) > GATE else ""
        if abs(d) > GATE or abs(d) > 2.0:
            print(f"{r:6.1f} {want:8.1f} {got:8.1f} {d:+8.1f}{flag}")
    print("  （只列 |Δ| > 2 的点）")


def run_single(args) -> int:
    theme = args.theme
    if theme == "light":
        ren_path = Path(args.render or (RENDER / "needle-light.png"))
        ref_path = Path(args.ref) if args.ref else None
    else:
        ren_path = Path(args.render or (RENDER / "high.png"))
        ref_path = Path(args.ref) if args.ref else (REF_DIR / "local-high.png")
    if not ren_path.exists():
        print(f"MISSING render {ren_path}")
        return 2
    ren = load(ren_path)
    target = TARGETS[theme]

    if ref_path is not None and ref_path.exists():
        ref_img = load(ref_path)
        st = dense_stats(ren, ref_img, target)
        print(f"== 指针轴密集扫描：{ren_path.name} vs {ref_path.name} ==")
    else:
        ref_img = None
        st = dense_stats(ren, None, target)
        print(f"== 指针轴密集扫描：{ren_path.name} vs 解析目标（{theme}）==")
    print(
        f"  采样 {st['n']} 点   max|Δ| {st['max']:.2f}/255   mean|Δ| {st['mean']:.2f}/255"
        f"   最坏 r={st['worst_r']:.1f}uu（Δ {st['worst_d']:+.2f}）"
    )
    print(f"  >{GATE:.0f}/255 的点 {st['gt4']} 个   >8/255 的点 {st['gt8']} 个"
          f"   ⇒ {'达标' if st['max'] <= GATE else '未达标'}")

    if ref_img is None:
        # 解析目标口径：额外给出行程中段的剖面（两主题各一条，贴报告用）
        print(f"\n== 逐点剖面（解析目标 {theme} vs 我们，抽样呈报）==")
        print(f"{'r':>6} {'期望':>8} {'我们':>8} {'Δ':>8}")
        for r in [4.0, 8.0, 12.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 76.0, 80.0, 84.0]:
            got = sample(ren, r)
            want = target(r)
            print(f"{r:6.1f} {want:8.1f} {got:8.1f} {got - want:+8.1f}")
        if args.profile:
            print(f"\n== 全点（{theme}）==")
            for r, want, got, d in st["rows"]:
                flag = "  <<<" if abs(d) > GATE else ""
                print(f"{r:6.1f} {want:8.1f} {got:8.1f} {d:+8.1f}{flag}")
    elif args.profile:
        print_profile(ref_path.name, ref_img, ren, target)
    return 0


def run_sweep() -> int:
    if not SWEEP.exists():
        print(f"MISSING sweep dir {SWEEP}")
        return 2
    ref_dark = load(REF_DIR / "local-high.png")
    hdr = (
        f"{'tag':>14} {'dense max':>10} {'dense mean':>11} {'analytic max':>13} "
        f"{'全图 mean':>10} {'>8%':>7} {'ident%':>8} {'中位ms':>8}"
    )
    print("== 切片参数对照表（指针轴 0.5uu 扫描）==")
    print(hdr)
    print("-" * len(hdr))
    rows = []
    for tag_dir in sorted(SWEEP.iterdir()):
        if not tag_dir.is_dir():
            continue
        high = tag_dir / "high.png"
        if not high.exists():
            continue
        ren = load(high)
        d_ref = dense_stats(ren, ref_dark, TARGETS["dark"])
        nd = tag_dir / "needle-dark.png"
        d_ana = dense_stats(load(nd), None, TARGETS["dark"]) if nd.exists() else None
        fs = full_stats(ref_dark, ren)
        med = "-"
        mf = tag_dir / "median.txt"
        if mf.exists():
            med = mf.read_text(encoding="utf-8").strip()
        rows.append((tag_dir.name, d_ref, d_ana, fs, med))
    for tag, d_ref, d_ana, fs, med in rows:
        ana = f"{d_ana['max']:.2f}" if d_ana else "-"
        print(
            f"{tag:>14} {d_ref['max']:10.2f} {d_ref['mean']:11.2f} {ana:>13} "
            f"{fs.get('mean', float('nan')):10.3f} {fs.get('gt8', float('nan')):7.3f} "
            f"{fs.get('ident', float('nan')):8.3f} {med:>8}"
        )
    print(f"\n  硬指标：dense max|Δ| ≤ {GATE:.0f}/255")
    for tag, d_ref, d_ana, _, _ in rows:
        ok = d_ref["max"] <= GATE
        print(
            f"  {tag:>14}  dense max {d_ref['max']:6.2f}  "
            f"解析 max {(d_ana['max'] if d_ana else float('nan')):6.2f}  "
            f"⇒ {'达标' if ok else '未达标'}"
            f"（最坏 r={d_ref['worst_r']:.1f}uu）"
        )
    return 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--render", default=None, help="我们的 PNG")
    ap.add_argument("--ref", default=None, help="参考 PNG")
    ap.add_argument("--theme", default="dark", choices=["dark", "light"])
    ap.add_argument("--profile", action="store_true", help="打印全点剖面")
    ap.add_argument("--sweep", action="store_true", help="扫 tools/probe_out/sweep/*")
    args = ap.parse_args(argv[1:])
    if args.sweep:
        return run_sweep()
    return run_single(args)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
