"""Ground-truth measurement of the reference screenshots (report/local-*.png).

The reference images are 1144x1144 = viewBox(286) x 4, i.e. 4 device pixels per
SVG user unit.  This script extracts the anchors the Rust port must reproduce so
the comparison step can be judged against numbers instead of eyeballs.

Usage:
    python tools/measure_reference.py <png> [--json]

Prints (in *user units*, i.e. pixels / 4):
    ring inner/outer radius, ring stroke width (radial scan at a few angles)
    number ink bbox + centre, unit-row ink bbox + centre
    label ink centres for the 9 standard ticks
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

from PIL import Image

UU = 286.0
CENTER = 143.0


def luma(px: tuple[int, int, int]) -> float:
    return 0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2]


def close(a: tuple[int, int, int], b: tuple[int, int, int], tol: int) -> bool:
    return all(abs(x - y) <= tol for x, y in zip(a, b))


def ring_scan(img: Image.Image, angle_deg: float) -> tuple[float, float] | None:
    """Radial scan for the track colour band; returns (inner, outer) radius in uu."""
    scale = img.width / UU
    cx, cy = img.width / 2, img.height / 2
    rad = math.radians(angle_deg - 90)
    hits = []
    r = 90.0
    while r <= 175.0:
        x = int(round(cx + r * scale * math.cos(rad)))
        y = int(round(cy + r * scale * math.sin(rad)))
        if 0 <= x < img.width and 0 <= y < img.height:
            px = img.getpixel((x, y))[:3]
            # track #26273b or any lit ring colour: just look for "not background"
            if luma(px) > 8.0:
                hits.append(r)
        r += 0.05
    if not hits:
        return None
    return hits[0], hits[-1]


def ink_bbox(img: Image.Image, predicate, box_uu: tuple[float, float, float, float]):
    """Ink bbox of the pixels matching `predicate` inside a user-unit box."""
    scale = img.width / UU
    x0, y0, x1, y1 = box_uu
    xs, ys = [], []
    for py in range(int(y0 * scale), int(y1 * scale)):
        for px in range(int(x0 * scale), int(x1 * scale)):
            if predicate(img.getpixel((px, py))[:3]):
                xs.append(px)
                ys.append(py)
    if not xs:
        return None
    return {
        "bbox_uu": [min(xs) / scale, min(ys) / scale, max(xs) / scale, max(ys) / scale],
        "centre_uu": [(min(xs) + max(xs) + 1) / 2 / scale, (min(ys) + max(ys) + 1) / 2 / scale],
        "size_uu": [(max(xs) - min(xs) + 1) / scale, (max(ys) - min(ys) + 1) / scale],
        "count": len(xs),
    }


def sweep_end_angle(img: Image.Image, mid_radius: float = 130.0) -> float | None:
    """Angle (deg, 0 = up, clockwise) where the lit ring stops being 'lit'.

    The track colour is #26273b (luma ~39) and every gradient colour is far
    brighter, so a luma threshold on the mid-radius circle isolates the sweep.
    """
    scale = img.width / UU
    cx, cy = img.width / 2, img.height / 2
    last_lit = None
    a = -135.0
    while a <= 135.0:
        rad = math.radians(a - 90)
        x = int(round(cx + mid_radius * scale * math.cos(rad)))
        y = int(round(cy + mid_radius * scale * math.sin(rad)))
        px = img.getpixel((x, y))[:3]
        if luma(px) > 60.0:
            last_lit = a
        a += 0.05
    return last_lit


def bright(px) -> bool:
    return luma(px) > 200


def greyish(px) -> bool:
    return 80 <= luma(px) <= 130


def main(argv: list[str]) -> int:
    path = Path(argv[1])
    img = Image.open(path).convert("RGB")
    out: dict[str, object] = {"file": path.name, "size": img.size, "scale": img.width / UU}

    rings = {}
    for angle in (-100.0, -60.0, 0.0, 60.0, 100.0):
        rings[f"{angle:+.0f}"] = ring_scan(img, angle)
    out["ring_radius_uu"] = rings
    out["sweep_end_deg"] = sweep_end_angle(img)

    out["number_ink"] = ink_bbox(img, bright, (100.0, 205.0, 190.0, 248.0))
    out["unit_row_ink"] = ink_bbox(img, lambda p: luma(p) > 60, (100.0, 248.0, 190.0, 275.0))

    labels = [0, 5, 10, 50, 100, 250, 500, 750, 1000]
    label_boxes = {}
    for i, value in enumerate(labels):
        angle = -135.0 + i * 33.75
        rad = math.radians(angle - 90)
        lx = CENTER + 97.0 * math.cos(rad)
        ly = CENTER + 97.0 * math.sin(rad)
        box = (lx - 18.0, ly - 10.0, lx + 18.0, ly + 10.0)
        label_boxes[str(value)] = ink_bbox(img, lambda p: luma(p) > 60, box)
    out["label_ink"] = label_boxes

    print(json.dumps(out, indent=2) if "--json" in argv else json.dumps(out, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
