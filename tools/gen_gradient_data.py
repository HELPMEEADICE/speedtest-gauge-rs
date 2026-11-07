#!/usr/bin/env python3
"""从 docs/gradient-stops.json 生成 src/gradient_data.rs（可重复、无外部依赖）。

用法:
    python tools/gen_gradient_data.py          # 生成/覆盖 src/gradient_data.rs
    python tools/gen_gradient_data.py --check  # 只校验：文件与生成结果不一致则退出码 1

数据链路:
    src/gauge/gradient-stops.ts  (原项目, 由 tools/emit_stops.py 采样生成)
        -> docs/gradient-stops.json  (本 crate 的权威副本)
        -> src/gradient_data.rs      (本脚本, 4 组 x 28 条 &[(f64, [u8; 3])])

约定:
    t 保留 4 位小数精度（与 .ts 源码字面量一致，去掉多余的尾随零，如 0.0370 -> 0.037）；
    颜色为 8-bit sRGB，[r, g, b] 三个 u8 十六进制字面量。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

# 变体顺序与 const 名固定，保证生成结果稳定。
GROUPS: list[tuple[str, str]] = [
    ("dl-multi", "DL_MULTI"),
    ("ul-multi", "UL_MULTI"),
    ("dl-single", "DL_SINGLE"),
    ("ul-single", "UL_SINGLE"),
]

EXPECTED_COUNT = 28

HEADER = """\
//! 角向渐变的颜色停止点表。
//!
//! **自动生成，请勿手改**：由 `tools/gen_gradient_data.py` 从 `docs/gradient-stops.json`
//! 生成（`python tools/gen_gradient_data.py`；`--check` 可校验一致性）。
//!
//! 数据来历：原项目 `src/gauge/gradient-stops.ts`（由 `tools/emit_stops.py` 沿弧中径对
//! speedtest.net 的渐变贴图采样，`t = i/27`）。采样证明颜色沿半径不变，所以整条弧可以
//! 精确表达成一个角向渐变。
//!
//! 每条 `(t, [r, g, b])`：`t = 0.0` 是弧起点（-135°），`t = 1.0` 是弧终点（+135°）；
//! 颜色为 8-bit sRGB。"""


def fmt_t(value: float) -> str:
    """4 位小数、去掉尾随零；至少保留一位小数（Rust 侧必须是浮点字面量）。"""
    text = f"{value:.4f}".rstrip("0")
    if text.endswith("."):
        text += "0"
    return text


def parse_hex(text: str) -> tuple[int, int, int]:
    body = text.lstrip("#")
    if len(body) != 6:
        raise ValueError(f"非法颜色 {text!r}")
    return int(body[0:2], 16), int(body[2:4], 16), int(body[4:6], 16)


def generate(doc: dict) -> str:
    lines: list[str] = [HEADER]
    for key, const in GROUPS:
        if key not in doc:
            raise KeyError(f"gradient-stops.json 缺少分组 {key!r}")
        entries = doc[key]
        if len(entries) != EXPECTED_COUNT:
            raise ValueError(f"{key!r} 应有 {EXPECTED_COUNT} 条，实际 {len(entries)}")
        lines.append(f"\n/// `{key}`，{EXPECTED_COUNT} 条。")
        lines.append(f"pub const {const}: &[(f64, [u8; 3])] = &[")
        for entry in entries:
            t = fmt_t(float(entry["t"]))
            r, g, b = parse_hex(entry["hex"])
            lines.append(f"    ({t}, [0x{r:02x}, 0x{g:02x}, 0x{b:02x}]),")
        lines.append("];")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description="生成 src/gradient_data.rs")
    parser.add_argument("--check", action="store_true", help="只校验，不写文件")
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[1]
    source = root / "docs" / "gradient-stops.json"
    target = root / "src" / "gradient_data.rs"

    doc = json.loads(source.read_text(encoding="utf-8"))
    text = generate(doc)
    digest = hashlib.sha256(text.encode("utf-8")).hexdigest()

    if args.check:
        current = target.read_text(encoding="utf-8") if target.exists() else ""
        if current != text:
            print(f"MISMATCH: {target} 与生成结果不一致 (expected sha256 {digest})", file=sys.stderr)
            return 1
        print(f"OK: {target.name} 与 {source.name} 一致 (sha256 {digest})")
        return 0

    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open("w", encoding="utf-8", newline="\n") as handle:
        handle.write(text)
    total = sum(len(doc[key]) for key, _ in GROUPS)
    print(f"wrote {target.name}: {len(GROUPS)} 组 / {total} 条 (sha256 {digest})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
