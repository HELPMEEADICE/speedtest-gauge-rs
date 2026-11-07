"""woff2 -> ttf conversion + FreeType/PIL metrics probe.

Run: python slint/tools/fontwork/fontwork.py
Prints a single JSON blob on stdout. No network, no writes outside the
paths listed in TARGETS.
"""
import json
import os
import sys

from fontTools.ttLib import TTFont
from PIL import ImageFont

SRC_GAUGE = r"C:\oss\Project\speedtest\public\fonts\gaugemono.woff2"
DST_GAUGE = r"C:\oss\Project\Rust\speedtest-rs\slint\.local-fonts\gaugemono.ttf"
SRC_MONT = r"C:\oss\Project\speedtest\public\fonts\montserrat.woff2"
DST_MONT = r"C:\oss\Project\Rust\speedtest-rs\slint\assets\fonts\montserrat.ttf"

GAUGE_MARK = "LOCAL CALIBRATION ONLY - PROPRIETARY - DO NOT REDISTRIBUTE"


def convert(src, dst, mark=None):
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    f = TTFont(src)
    was = f.flavor
    f.flavor = None
    if mark:
        # Plant the "proprietary / local only" banner inside name IDs 0 and 10
        # so the restriction travels with the binary itself.
        name = f["name"]
        for rec in list(name.names):
            if rec.nameID == 0:
                name.removeNames(nameID=0)
        for pid, eid, lid in ((3, 1, 0x409), (1, 0, 0)):
            name.setName(mark, 0, pid, eid, lid)
            name.setName(mark, 10, pid, eid, lid)
    f.save(dst)
    f.close()
    return was


def info(path):
    f = TTFont(path)
    head, os2, hhea, post = f["head"], f["OS/2"], f["hhea"], f["post"]
    names = {}
    for rec in f["name"].names:
        if rec.nameID in (0, 1, 2, 4, 6, 10, 16, 17) and rec.platformID == 3:
            try:
                names.setdefault(rec.nameID, rec.toUnicode())
            except Exception:
                pass
    cmap = f.getBestCmap()
    out = {
        "path": path,
        "bytes": os.path.getsize(path),
        "family(nameID1)": names.get(1),
        "subfamily(nameID2)": names.get(2),
        "fullname(nameID4)": names.get(4),
        "postscript(nameID6)": names.get(6),
        "typographic_family(16)": names.get(16),
        "typographic_subfamily(17)": names.get(17),
        "nameID0": names.get(0),
        "nameID10": names.get(10),
        "unitsPerEm": head.unitsPerEm,
        "head.yMin": head.yMin,
        "head.yMax": head.yMax,
        "hhea.ascender": hhea.ascender,
        "hhea.descender": hhea.descender,
        "hhea.lineGap": hhea.lineGap,
        "os2.sTypoAscender": os2.sTypoAscender,
        "os2.sTypoDescender": os2.sTypoDescender,
        "os2.sTypoLineGap": os2.sTypoLineGap,
        "os2.usWinAscent": os2.usWinAscent,
        "os2.usWinDescent": os2.usWinDescent,
        "os2.usWeightClass": os2.usWeightClass,
        "os2.usWidthClass": os2.usWidthClass,
        "os2.fsSelection": os2.fsSelection,
        "os2.fsType": os2.fsType,
        "head.macStyle": head.macStyle,
        "isBold_macStyle": bool(head.macStyle & 0x1),
        "isBold_fsSelection": bool(os2.fsSelection & 0x20),
        "post.isFixedPitch": post.isFixedPitch,
        "numGlyphs": f["maxp"].numGlyphs,
        "capHeight": getattr(os2, "sCapHeight", None),
        "xHeight": getattr(os2, "sxHeight", None),
        "glyph_count_cmap": len(cmap),
    }
    # cap height measured from the actual 'H' outline if OS/2 lacks it
    gs = f.getGlyphSet()
    if "H" in cmap:
        from fontTools.pens.boundsPen import BoundsPen

        bp = BoundsPen(gs)
        gs[cmap["H"]].draw(bp)
        out["measured_H_bounds"] = bp.bounds
    f.close()
    return out


def coverage(path, chars):
    f = TTFont(path)
    cmap = f.getBestCmap()
    f.close()
    return {
        "wanted": chars,
        "missing": [c for c in chars if ord(c) not in cmap],
        "advance_units": None,
    }


def measure(path, size, strings, digits=None):
    font = ImageFont.truetype(path, size=size)
    res = {"path": path, "size_px": size, "lengths": {}, "bbox": {}}
    for s in strings:
        res["lengths"][s] = round(font.getlength(s), 4)
        res["bbox"][s] = [round(v, 4) for v in font.getbbox(s)]
    if digits:
        per = {d: round(font.getlength(d), 4) for d in digits}
        res["per_glyph_length"] = per
        res["monospaced_digits"] = len(set(per.values())) == 1
    # family as FreeType sees it
    try:
        res["freetype_family"] = font.getname()
    except Exception as e:
        res["freetype_family"] = "err:%s" % e
    return res


def em_ratios(m, size):
    return {k: round(v / size, 6) for k, v in m["lengths"].items()}


def main():
    out = {}
    out["converted_gauge_from_flavor"] = convert(SRC_GAUGE, DST_GAUGE, mark=GAUGE_MARK)
    out["converted_mont_from_flavor"] = convert(SRC_MONT, DST_MONT)
    out["gauge_info"] = info(DST_GAUGE)
    out["mont_info"] = info(DST_MONT)

    out["gauge_coverage"] = coverage(DST_GAUGE, "0123456789.,")
    out["mont_coverage"] = coverage(DST_MONT, "0123456789Mbps")

    g = measure(DST_GAUGE, 36, ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
                                ".", ",", "862.00", "4,200.00", "1144", "999.99"],
                digits="0123456789")
    out["gauge_measure"] = g
    out["gauge_em"] = em_ratios(g, 36)

    m = measure(DST_MONT, 14, ["100", "1000", "Mbps", "0", "1", "2", "3", "4",
                               "5", "6", "7", "8", "9", ".", ","],
                digits="0123456789")
    out["mont_measure"] = m
    out["mont_em"] = em_ratios(m, 14)

    json.dump(out, sys.stdout, ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
