"""Verify the local font assets used for pixel-accurate calibration.

Prints family/upm/weight plus the advance widths the Rust side relies on.
Exit code is non-zero when a required glyph is missing.

Usage: python tools/verify_fonts.py
"""

import sys
from pathlib import Path

from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
GAUGE = ROOT / ".local-fonts" / "gaugemono.ttf"
MONTSERRAT = ROOT / "assets" / "fonts" / "montserrat.ttf"

GAUGE_CHARS = "0123456789.,"
MONTSERRAT_CHARS = "0123456789Mbps"


def report(path: Path, required: str) -> list[str]:
    if not path.exists():
        print(f"MISSING {path}")
        return list(required)
    font = TTFont(path)
    names = {r.nameID: r.toUnicode() for r in font["name"].names if r.platformID == 3}
    upm = font["head"].unitsPerEm
    cmap = font.getBestCmap()
    hmtx = font["hmtx"]
    print(
        f"{path.name}: family={names.get(1)!r} sub={names.get(2)!r} upm={upm} "
        f"glyphs={font['maxp'].numGlyphs} weight={font['OS/2'].usWeightClass}"
    )
    missing = [c for c in required if ord(c) not in cmap]
    print(f"  cmap entries={len(cmap)} missing={missing}")
    return missing


def advance(font: TTFont, text: str) -> float:
    upm = font["head"].unitsPerEm
    cmap = font.getBestCmap()
    return sum(font["hmtx"][cmap[ord(c)]][0] for c in text) / upm


def main() -> int:
    missing = report(GAUGE, GAUGE_CHARS) + report(MONTSERRAT, MONTSERRAT_CHARS)
    for path, samples in (
        (GAUGE, ["0", "862.00", "4,200.00"]),
        (MONTSERRAT, ["100", "1000", "Mbps"]),
    ):
        if not path.exists():
            continue
        font = TTFont(path)
        for text in samples:
            em = advance(font, text)
            print(f"  {path.name:16s} {text!r:12s} em={em:.6f} @36px={em * 36:.3f} @144px={em * 144:.3f}")
        if path is GAUGE:
            digits = {round(advance(font, d), 6) for d in "0123456789"}
            print(f"  gaugemono digit advance set={sorted(digits)}")
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
