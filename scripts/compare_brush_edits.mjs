#!/usr/bin/env node
// Real hidden Release app acceptance; no GPU/window work belongs in cargo test.
// Usage: node scripts/compare_brush_edits.mjs BASELINE CANDIDATE OUTPUT_DIR [samples=12]
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync, openSync, closeSync } from 'node:fs';
import { resolve, join } from 'node:path';
const [baseline, candidate, output, count = '12'] = process.argv.slice(2);
if (!baseline || !candidate || !output) throw Error('usage: node scripts/compare_brush_edits.mjs BASELINE CANDIDATE OUTPUT_DIR [samples=12]; build both binaries with cargo build --release; run from repository root');
const root = resolve(output);
mkdirSync(root, { recursive: true });
const cases = ['add-empty-tiny', 'add-sparse-point', 'add-dense-point', 'add-dense-long', 'add-dense-cross', 'noop-dense-point', 'noop-dense-cross', 'remove-empty-point', 'remove-dense-point', 'remove-dense-cross', 'dig-dense-point', 'place-dense-point'];
const stats = a => { a.sort((x, y) => x - y); return { median: a[Math.floor(a.length / 2)], p95: a[Math.min(a.length - 1, Math.floor(a.length * .95))] }; };
const results = [];
for (const [index, scenario] of cases.entries()) {
  const row = { scenario };
  // Alternate order to reduce thermal/clock order bias. Never run GPU cases concurrently.
  const pair = [['baseline', baseline], ['candidate', candidate]];
  if (index % 2) pair.reverse();
  for (const [variant, binary] of pair) {
    const file = join(root, `${scenario}-${variant}`);
    const fd = openSync(file + '.log', 'w');
    const run = spawnSync(resolve(binary), ['--hidden', '--mute', '--flat-terrain'], {
      env: { ...process.env, RUST_LOG: 'info', RE_FLORA_BRUSH_EDIT_BENCH: scenario, RE_FLORA_BRUSH_BENCH_REPORT: file + '.json', RE_FLORA_BRUSH_BENCH_SAMPLES: count },
      stdio: ['ignore', fd, fd], timeout: 180000,
    });
    closeSync(fd);
    const log = readFileSync(file + '.log', 'utf8');
    if (run.status !== 0 || !log.includes('[BRUSH_EDIT_BENCH] complete') || !log.includes('Application exited successfully') || /\bERROR\b|VUID-|panicked/.test(log)) throw Error(`failed ${file}: status=${run.status}, error=${run.error}`);
    const events = JSON.parse(readFileSync(file + '.json', 'utf8')).events.filter(e => !e.warmup);
    if (events.length !== Number(count)) throw Error(`wrong sample count: ${file}`);
    const signatures = [...new Set(events.map(e => JSON.stringify([e.before_roots, e.after_roots, e.changed_voxels, e.root_hash, e.atlas_hash])))];
    if (signatures.length !== 1) throw Error(`nondeterministic replay: ${file}: ${signatures}`);
    row[variant] = { edit_ms: stats(events.map(e => e.edit_ms)), signature: signatures[0] };
    if (events[0].async_metrics) {
      row[variant].async = {
        submit_ms: stats(events.map(e => e.async_metrics.submit_ms)),
        finish_ms: stats(events.map(e => e.async_metrics.finish_ms)),
        accepted_to_publish_ms: stats(events.map(e => e.async_metrics.accepted_to_publish_ms)),
        frames: [...new Set(events.map(e => e.async_frames))],
        retries: [...new Set(events.map(e => e.async_metrics.retries))],
      };
    }
  }
  if (row.baseline.signature !== row.candidate.signature) throw Error(`output changed: ${JSON.stringify(row)}`);
  results.push(row);
  writeFileSync(join(root, 'summary.json'), JSON.stringify(results, null, 2));
  console.log(`${scenario}: ${row.baseline.edit_ms.median.toFixed(3)} -> ${row.candidate.edit_ms.median.toFixed(3)} ms; identical output`);
}
