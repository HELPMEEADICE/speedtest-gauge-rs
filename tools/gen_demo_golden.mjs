// 生成 DemoSource 的黄金样本序列（Rust 侧逐条比对用）。
//
// 用法（在 slint/ 目录下）：
//   node tools/gen_demo_golden.mjs
// 产物：tests/golden/demo_samples.json
//
// 重要：本脚本**不 import** 原项目 src/gauge/pipeline.ts（那里有 window/WebSocket 依赖），
// 而是按 pipeline.ts:24-33 / 51-127 的公式与时间线**独立重写**一份，
// 这样它才是 Rust 实现的独立见证——两边算出来的数一致，才说明移植没跑偏。

import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const OUT = join(HERE, '..', 'tests', 'golden', 'demo_samples.json');

// ---- 源码默认值（pipeline.ts:61-63, :64, :76-77, :115, :117） ----
const OPTS = {
  downloadPeak: 862, // Mbps
  uploadPeak: 341, // Mbps
  holdMs: 4200, // ms
  seed: 20260101,
};
const TICK_MS = 100; // pipeline.ts:76
const RAMP_MS = 1500; // pipeline.ts:77
const PING_WAIT_MS = 1200; // pipeline.ts:117
const GAP_MS = 620; // pipeline.ts:115

// ---- mulberry32（pipeline.ts:24-33，逐字复刻） ----
function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const rnd = mulberry32(OPTS.seed);
const samples = [];
const emit = (stage, kbps) => samples.push({ stage, kbps: Math.max(0, kbps) });

// 记录时间线的关键刻度，顺便给文档和 Rust 侧一个可核对的锚点。
const timeline = [];

// t = 0：延迟一拍
emit('ping', 0);
timeline.push({ at: 0, what: 'phase(ping) + emit(ping, 0)' });

// pipeline.ts:74-97 的 runMeasuredPhase：段内相对时间 t 从 0 起每 100ms 一次，
// 首个样本与 phase(stage) 同一瞬间（源码里 loop() 是同步直接调用的）。
function runMeasuredPhase(stage, peakMbps, holdMs) {
  const total = RAMP_MS + holdMs;
  let t = 0;
  for (;;) {
    if (t >= total) return; // 等价于 `if (t >= total) { next(); return; }`
    const rampT = Math.min(1, t / RAMP_MS);
    const envelope = rampT < 1 ? 1 - Math.pow(1 - rampT, 3) : 1;
    const drift = Math.sin((t / 1000) * 1.7) * 0.012 + Math.sin((t / 1000) * 4.3) * 0.006;
    const noise = (rnd() - 0.5) * 0.03;
    const mbps = peakMbps * envelope * (1 + drift + noise);
    emit(stage, mbps * 1000);
    t += TICK_MS;
  }
}

let now = PING_WAIT_MS;
timeline.push({ at: now, what: 'runMeasuredPhase(download, 862, 4200)' });
runMeasuredPhase('download', OPTS.downloadPeak, OPTS.holdMs);
now += RAMP_MS + OPTS.holdMs;
timeline.push({ at: now, what: "phase(upload) — 不发样本" });
now += GAP_MS;
timeline.push({ at: now, what: 'runMeasuredPhase(upload, 341, 4200)' });
runMeasuredPhase('upload', OPTS.uploadPeak, OPTS.holdMs);
now += RAMP_MS + OPTS.holdMs;
timeline.push({ at: now, what: 'finish(): phase(done) + emit(done, downloadPeak*1000)' });
emit('done', OPTS.downloadPeak * 1000);

const counts = samples.reduce((acc, s) => ((acc[s.stage] = (acc[s.stage] ?? 0) + 1), acc), {});

// 一行一个样本：Rust 侧的测试只需要按行扫，不必引 serde。
const lines = samples.map(
  (s) => `    { "stage": "${s.stage}", "kbps": ${s.kbps} }`,
);
const doc =
  '{\n' +
  `  "meta": { "source": "src/gauge/pipeline.ts", "seed": ${OPTS.seed}, "downloadPeakMbps": ${OPTS.downloadPeak}, "uploadPeakMbps": ${OPTS.uploadPeak}, "holdMs": ${OPTS.holdMs}, "tickMs": ${TICK_MS}, "rampMs": ${RAMP_MS}, "pingWaitMs": ${PING_WAIT_MS}, "stageGapMs": ${GAP_MS} },\n` +
  `  "counts": { "ping": ${counts.ping ?? 0}, "download": ${counts.download ?? 0}, "upload": ${counts.upload ?? 0}, "done": ${counts.done ?? 0} },\n` +
  `  "total": ${samples.length},\n` +
  '  "samples": [\n' +
  lines.join(',\n') +
  '\n  ]\n' +
  '}\n';

mkdirSync(dirname(OUT), { recursive: true });
writeFileSync(OUT, doc, 'utf8');

console.log(`wrote ${OUT}`);
console.log(`total=${samples.length} counts=${JSON.stringify(counts)}`);
console.log('first 5:', samples.slice(0, 5).map((s) => `${s.stage} ${s.kbps}`).join(' | '));
console.log('last 5 :', samples.slice(-5).map((s) => `${s.stage} ${s.kbps}`).join(' | '));
console.log('timeline:', JSON.stringify(timeline));
