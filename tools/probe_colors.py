"""Colour + needle-profile probe: reference vs our render.

The geometry already matches (see tools/verify_states.py); this checks the things
geometry cannot see: theme token colours and the needle's alpha ramp.

Usage: python tools/probe_colors.py
"""

from __future__ import annotations

import math
import os
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
REF = Path(
    os.environ.get("SPEEDTEST_REPORT_DIR", str(ROOT.parents[2] / "speedtest" / "report"))
)
RENDER = ROOT / "comparison" / "render"
UU = 286.0
C = 143.0


def load(path: Path) -> Image.Image:
    return Image.open(path).convert("RGB")


def at(img: Image.Image, x_uu: float, y_uu: float) -> tuple[int, int, int]:
    s = img.width / UU
    return img.getpixel((int(round(x_uu * s)), int(round(y_uu * s))))[:3]


def brightest(img: Image.Image, box_uu) -> tuple[int, int, int]:
    s = img.width / UU
    x0, y0, x1, y1 = box_uu
    best, best_l = (0, 0, 0), -1.0
    for py in range(int(y0 * s), int(y1 * s)):
        for px in range(int(x0 * s), int(x1 * s)):
            p = img.getpixel((px, py))[:3]
            l = 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
            if l > best_l:
                best, best_l = p, l
    return best


def polar(x_uu: float, y_uu: float) -> tuple[float, float]:
    return x_uu, y_uu


def ring_colour(img: Image.Image, angle: float, radius: float = 130.0) -> tuple[int, int, int]:
    rad = math.radians(angle - 90.0)
    return at(img, C + radius * math.cos(rad), C + radius * math.sin(rad))


def needle_profile(img: Image.Image, angle: float, r0: float = 12.0, r1: float = 84.0, n: int = 25):
    """Luma along the needle's centre line, from the hub outwards."""
    rad = math.radians(angle - 90.0)
    out = []
    for i in range(n):
        r = r0 + (r1 - r0) * i / (n - 1)
        out.append(at(img, C + r * math.cos(rad), C + r * math.sin(rad)))
    return out


def luma(p) -> float:
    return 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]


def main() -> int:
    ref, ren = load(REF / "local-high.png"), load(RENDER / "high.png")

    print("== theme tokens (R,G,B) ==")
    probes = {
        "track @-60°": lambda im: ring_colour(im, -60.0),
        "track @+60° (unlit part)": lambda im: ring_colour(im, 60.0),
        "ring @-100° (lit)": lambda im: ring_colour(im, -100.0),
        "ring @0° (lit)": lambda im: ring_colour(im, 0.0),
        "ring @100° (lit)": lambda im: ring_colour(im, 100.0),
        "number glyph (brightest)": lambda im: brightest(im, (106.0, 207.0, 180.0, 236.0)),
        "label 100 (lit)": lambda im: brightest(im, (128.0, 38.0, 158.0, 52.0)),
        "label 1000 (unlit)": lambda im: brightest(im, (192.0, 204.0, 230.0, 217.0)),
        "unit Mbps (brightest)": lambda im: brightest(im, (140.0, 252.0, 175.0, 268.0)),
        "unit arrow (brightest)": lambda im: brightest(im, (112.0, 250.0, 130.0, 267.0)),
    }
    for name, fn in probes.items():
        a, b = fn(ref), fn(ren)
        d = max(abs(x - y) for x, y in zip(a, b))
        print(f"  {name:28s} ref={a}  render={b}  maxΔ={d}")

    print("\n== needle alpha ramp along the axis (luma 0..255) ==")
    pr = needle_profile(ref, 116.37)
    pn = needle_profile(ren, 116.37)
    diffs = []
    for i, (a, b) in enumerate(zip(pr, pn)):
        r = 12.0 + (84.0 - 12.0) * i / (len(pr) - 1)
        d = luma(b) - luma(a)
        diffs.append(abs(d))
        print(f"  r={r:5.1f}  ref={luma(a):6.1f}  render={luma(b):6.1f}  Δ={d:+6.1f}")
    print(f"  mean|Δ|={sum(diffs)/len(diffs):.2f}  max|Δ|={max(diffs):.2f}")

    print("\n== needle banding: adjacent-sample luma steps in the render ==")
    steps = [luma(pn[i + 1]) - luma(pn[i]) for i in range(len(pn) - 1)]
    print("  steps:", " ".join(f"{s:+.1f}" for s in steps))
    return 0


if __name__ == "__main__":
    sys.exit(main())
