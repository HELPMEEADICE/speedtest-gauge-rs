"""Independent A/B verification: reference screenshot vs our Slint render.

Deliberately built on top of `measure_reference.py` (the same instrument that
produced `comparison/ref-metrics/*.json`), so the reference and the render are
measured by *identical* code.  Nothing here trusts numbers reported by the
implementer.

Usage:
    python tools/verify_states.py                # all states that have a reference
    python tools/verify_states.py high idle      # subset
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
import measure_reference as mr  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
REF = Path(
    os.environ.get("SPEEDTEST_REPORT_DIR", str(ROOT.parents[2] / "speedtest" / "report"))
)
RENDER = ROOT / "comparison" / "render"
REF_METRICS = ROOT / "comparison" / "ref-metrics"

# (reference png, our png, human label)
STATES = [
    ("local-idle", "idle", "Idle"),
    ("local-low", "low", "Download 8 Mbps"),
    ("local-mid", "mid", "Download 155.61 Mbps"),
    ("local-high", "high", "Download 862 Mbps"),
    ("local-upload", "upload", "Upload 341 Mbps"),
    ("local-done", "done", "Done"),
    ("local-extended", "extended", "Extended 4200 Mbps (auto)"),
]

NUMBER_BOX = (100.0, 205.0, 190.0, 248.0)
UNIT_BOX = (100.0, 248.0, 190.0, 275.0)
LABELS = [0, 5, 10, 50, 100, 250, 500, 750, 1000]
LABEL_ANGLES = [-135.0 + i * 33.75 for i in range(9)]


def load(path: Path) -> Image.Image:
    return Image.open(path).convert("RGB")


def ring_pair(img: Image.Image) -> tuple[float, float]:
    a = mr.ring_scan(img, -60.0)
    b = mr.ring_scan(img, 60.0)
    inner = min(x for x in (a[0] if a else 999, b[0] if b else 999))
    outer = max(x for x in (a[1] if a else 0, b[1] if b else 0))
    return round(inner, 2), round(outer, 2)


def label_centres(img: Image.Image) -> dict[str, list[float]]:
    out = {}
    for value, angle in zip(LABELS, LABEL_ANGLES):
        import math

        rad = math.radians(angle - 90)
        lx = mr.CENTER + 97.0 * math.cos(rad)
        ly = mr.CENTER + 97.0 * math.sin(rad)
        box = (lx - 18.0, ly - 10.0, lx + 18.0, ly + 10.0)
        ink = mr.ink_bbox(img, lambda p: mr.luma(p) > 60, box)
        if ink:
            out[str(value)] = [round(v, 2) for v in ink["centre_uu"]]
    return out


def pixel_stats(ref: Image.Image, ren: Image.Image) -> dict:
    a = np.asarray(ref, dtype=np.int16)
    b = np.asarray(ren, dtype=np.int16)
    if a.shape != b.shape:
        return {"error": f"size mismatch {a.shape} vs {b.shape}"}
    d = np.abs(a - b)
    per_px = d.max(axis=2)
    total = per_px.size
    return {
        "mean_abs": round(float(d.mean()), 3),
        "max_abs": int(d.max()),
        "pct_gt_8": round(float((per_px > 8).sum() * 100.0 / total), 3),
        "pct_gt_32": round(float((per_px > 32).sum() * 100.0 / total), 3),
        "pct_identical": round(float((per_px == 0).sum() * 100.0 / total), 3),
        "ink_ref": int((np.asarray(ref).max(axis=2) > 60).sum()),
        "ink_render": int((np.asarray(ren).max(axis=2) > 60).sum()),
    }


def main(argv: list[str]) -> int:
    wanted = set(argv[1:])
    rows = []
    for ref_name, ren_name, label in STATES:
        if wanted and ref_name.replace("local-", "") not in wanted and ren_name not in wanted:
            continue
        ref_path, ren_path = REF / f"{ref_name}.png", RENDER / f"{ren_name}.png"
        if not ren_path.exists():
            print(f"MISSING render {ren_path}")
            continue
        ref, ren = load(ref_path), load(ren_path)
        row = {
            "state": label,
            "ring_ref": ring_pair(ref),
            "ring_render": ring_pair(ren),
            "sweep_ref": mr.sweep_end_angle(ref),
            "sweep_render": mr.sweep_end_angle(ren),
            "number_ref": [round(v, 2) for v in mr.ink_bbox(ref, mr.bright, NUMBER_BOX)["bbox_uu"]],
            "number_render": [round(v, 2) for v in mr.ink_bbox(ren, mr.bright, NUMBER_BOX)["bbox_uu"]],
            "unit_ref": [round(v, 2) for v in mr.ink_bbox(ref, lambda p: mr.luma(p) > 60, UNIT_BOX)["bbox_uu"]],
            "unit_render": [round(v, 2) for v in mr.ink_bbox(ren, lambda p: mr.luma(p) > 60, UNIT_BOX)["bbox_uu"]],
            "labels_ref": label_centres(ref),
            "labels_render": label_centres(ren),
            "pixels": pixel_stats(ref, ren),
        }
        rows.append(row)

    print(json.dumps(rows, indent=1, ensure_ascii=False))

    # compact verdict table
    print("\n=== verdict ===")
    for row in rows:
        lab_diff = max(
            (max(abs(a - b) for a, b in zip(row["labels_ref"][k], row["labels_render"][k]))
             for k in row["labels_ref"] if k in row["labels_render"]),
            default=-1,
        )
        sw = "n/a" if row["sweep_ref"] is None or row["sweep_render"] is None else f"{row['sweep_render'] - row['sweep_ref']:+.2f}"
        print(
            f"{row['state']:30s} ring {row['ring_render']} vs {row['ring_ref']}  "
            f"sweepΔ {sw}  num_bboxΔ {max(abs(a-b) for a,b in zip(row['number_ref'], row['number_render'])):.2f}  "
            f"unit_bboxΔ {max(abs(a-b) for a,b in zip(row['unit_ref'], row['unit_render'])):.2f}  "
            f"labelΔ {lab_diff:.2f}  mean|Δ| {row['pixels'].get('mean_abs')}  "
            f">8: {row['pixels'].get('pct_gt_8')}%  ident {row['pixels'].get('pct_identical')}%"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
