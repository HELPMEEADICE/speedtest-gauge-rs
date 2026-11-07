#!/usr/bin/env node
/**
 * 从 animator.ts 的 cubicBezier 逻辑生成缓动黄金向量，写入 tests/animation.rs 的标记块。
 *
 * 用法:
 *   node tools/gen_easing_vectors.mjs           # 生成/覆盖 tests/animation.rs 里的向量块
 *   node tools/gen_easing_vectors.mjs --check   # 只校验，不一致则退出码 1
 *
 * 说明:
 *   - 下面的 cubicBezier 是 src/gauge/animator.ts:26-65 的**逐行转写**（不是重新推导），
 *     牛顿 8 次 + 二分 24 次、1e-6 提前退出、x<=0 -> 0 / x>=1 -> 1；
 *   - 采样点 t_i = i / (N - 1)，i in [0, N)，即含 0 与 1 两个端点；
 *   - 数值用 String(v) 输出（最短往返表示），Rust 侧解析得到**同一个** f64，
 *     所以断言的容差可以取 1e-12 而实测误差通常为 0。
 */

import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const N = 200;

/** animator.ts:26-65 的逐行转写。 */
function cubicBezier(x1, y1, x2, y2) {
  const cx = 3 * x1;
  const bx = 3 * (x2 - x1) - cx;
  const ax = 1 - cx - bx;
  const cy = 3 * y1;
  const by = 3 * (y2 - y1) - cy;
  const ay = 1 - cy - by;

  const sampleX = (t) => ((ax * t + bx) * t + cx) * t;
  const sampleY = (t) => ((ay * t + by) * t + cy) * t;
  const sampleDX = (t) => (3 * ax * t + 2 * bx) * t + cx;

  const solveX = (x) => {
    let t = x;
    for (let i = 0; i < 8; i++) {
      const err = sampleX(t) - x;
      if (Math.abs(err) < 1e-6) return t;
      const d = sampleDX(t);
      if (Math.abs(d) < 1e-6) break;
      t -= err / d;
    }
    let lo = 0;
    let hi = 1;
    t = x;
    for (let i = 0; i < 24; i++) {
      const v = sampleX(t);
      if (Math.abs(v - x) < 1e-6) break;
      if (v < x) lo = t;
      else hi = t;
      t = (lo + hi) / 2;
    }
    return t;
  };

  return (x) => {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    return sampleY(solveX(x));
  };
}

const EASE_OUT = cubicBezier(0, 0, 0.58, 1);
const EASE_IN_OUT = cubicBezier(0.42, 0, 0.58, 1);

/** Rust 浮点字面量：JS 的 `1` / `0` 必须补 `.0`（Rust 不做 int -> f64 隐式转换）。 */
function lit(v) {
  let s = String(v);
  if (!s.includes('.') && !s.includes('e') && !s.includes('E')) s += '.0';
  return s;
}

/**
 * 生成 rustfmt 稳定的一行一个元素格式。
 *
 * 早先这里把所有元素塞在**同一行**，结果 `cargo fmt` 会把它重排成一行一个——于是
 * `--check` 的文本比较永远不可能通过。现在生成的就是 rustfmt 的输出形态（竖排 + 尾逗号），
 * 并且 `--check` 改成**按数值比较**（见下），双重保险。
 */
function rustArray(name, values) {
  const body = values.map((v) => `    ${lit(v)},`).join('\n');
  return `const ${name}: [f64; ${N}] = [\n${body}\n];`;
}

const ts = Array.from({ length: N }, (_, i) => i / (N - 1));
const out = ts.map((t) => EASE_OUT(t));
const inout = ts.map((t) => EASE_IN_OUT(t));

const block = [
  '// <gen:easing-vectors>',
  '// 由 `node tools/gen_easing_vectors.mjs` 生成，请勿手改。',
  `// 采样点 t_i = i / ${N - 1}，i in [0, ${N})；值由 animator.ts 的 cubicBezier 逻辑算出。`,
  `const EASING_N: usize = ${N};`,
  rustArray('EASING_T', ts),
  rustArray('EASING_OUT', out),
  rustArray('EASING_IN_OUT', inout),
  '// </gen:easing-vectors>',
].join('\n');

const here = dirname(fileURLToPath(import.meta.url));
const target = join(here, '..', 'tests', 'animation.rs');
const START = '// <gen:easing-vectors>';
const END = '// </gen:easing-vectors>';

if (!existsSync(target)) {
  console.error(`missing ${target}: 先创建带标记块的测试文件`);
  process.exit(2);
}

const source = readFileSync(target, 'utf8');
const from = source.indexOf(START);
const to = source.indexOf(END);
if (from < 0 || to < 0 || to < from) {
  console.error(`marker block not found in ${target}`);
  process.exit(2);
}
const next = source.slice(0, from) + block + source.slice(to + END.length);

/**
 * 按**数值**校验，而不是按文本校验。
 *
 * 生成块会被 `cargo fmt` 重新排版（缩进、换行、尾逗号），rustfmt 的版次之间也可能有差异；
 * 文本比较会让"数值完全正确但排版变了"这种情况误报。这里把文件里的三个数组抽出来解析成
 * f64 再逐点比对，容差 1e-12（`lit()` 用最短往返表示，实测误差通常是 0）。
 */
function arrayOf(fileSource, name) {
  const re = new RegExp(`const ${name}: \\[f64; ${N}\\] = \\[([\\s\\S]*?)\\];`);
  const m = fileSource.match(re);
  if (!m) return null;
  return (m[1].match(/-?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?/g) || []).map(Number);
}

const check = process.argv.includes('--check');
if (check) {
  const problems = [];
  const nMatch = source.match(/const EASING_N: usize = (\d+);/);
  if (!nMatch || Number(nMatch[1]) !== N) {
    problems.push(`EASING_N = ${nMatch ? nMatch[1] : '(缺失)'}，期望 ${N}`);
  }
  for (const [name, expected] of [
    ['EASING_T', ts],
    ['EASING_OUT', out],
    ['EASING_IN_OUT', inout],
  ]) {
    const got = arrayOf(source, name);
    if (!got) {
      problems.push(`${name}: 在 tests/animation.rs 里找不到 \`const ${name}: [f64; ${N}]\``);
      continue;
    }
    if (got.length !== expected.length) {
      problems.push(`${name}: ${got.length} 个元素，期望 ${N} 个`);
      continue;
    }
    for (let i = 0; i < expected.length; i++) {
      if (!(Math.abs(got[i] - expected[i]) <= 1e-12)) {
        problems.push(`${name}[${i}]: 文件里是 ${got[i]}，脚本算出 ${expected[i]}`);
        break;
      }
    }
  }
  if (problems.length) {
    console.error('MISMATCH: tests/animation.rs 的向量与脚本算出的一致吗？不一致：');
    for (const p of problems) console.error('  - ' + p);
    console.error('提示：跑 `node tools/gen_easing_vectors.mjs` 重新生成后再 `cargo fmt`。');
    process.exit(1);
  }
  console.log(`OK: ${N} 点 x 2 条曲线，数值逐点一致 (EASE_OUT/EASE_IN_OUT，容差 1e-12)`);
  process.exit(0);
}

writeFileSync(target, next, 'utf8');
console.log(`wrote ${N} 点 x 2 条曲线到 ${target}`);
