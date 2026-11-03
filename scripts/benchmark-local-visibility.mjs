#!/usr/bin/env node
// Real Release-path regression gate. Run exclusively: never overlap GPU jobs.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { parseArgs } from 'node:util';
import path from 'node:path';
import assert from 'node:assert/strict';

const help = `Usage: node scripts/benchmark-local-visibility.mjs [--binary PATH] [--output DIR] [--repeats N] [--seconds N]
       node scripts/benchmark-local-visibility.mjs --analyze DIR
Runs original/shared Release modes in alternating order at spatial spacing 32,
nighttime 0.95, garden-arrival, seed 20260519. Defaults: target/release/re-flora,
3 pairs, 30 seconds/run, new timestamped target/local-visibility-perf directory.
Temporarily edits config/gui.toml, restoring its exact bytes even on failure.
Do not run concurrently with another app/build/GPU job. No visible window/audio.
Requires >=50 GPU samples in the last 8 seconds and completed shared-cache build.
--analyze DIR replays DIR/final-{off,on}.log (sparse historical evidence allowed).
Exit 0: no median grass GPU, steady FPS or whole-profiled-run FPS regression;
whole-run throughput includes cache construction (not process startup).
Exit 1: regression/run failure;
2: invalid usage. One pair is diagnostic only; acceptance needs repeated evidence.
Examples:
  node scripts/benchmark-local-visibility.mjs --repeats 1 --output target/visibility-red
  node scripts/benchmark-local-visibility.mjs --analyze target/source-vis`;
const median = values => {
  const a = [...values].sort((a, b) => a - b), i = Math.floor(a.length / 2);
  assert.ok(a.length, 'Missing measurement samples');
  return a.length % 2 ? a[i] : (a[i - 1] + a[i]) / 2;
};
const time = line => {
  const m = line.match(/^\[(\d+):(\d+):(\d+)\.(\d+)/);
  return m ? ((+m[1] * 60 + +m[2]) * 60 + +m[3]) * 1000 + +m[4] : NaN;
};
function metrics(text, shared, minimumSamples) {
  assert.doesNotMatch(text, /\bERROR\b|VUID-|SYNC-HAZARD|panicked at/);
  assert.doesNotMatch(text, /\[LOCAL_VISIBILITY\]\[RECEIVERS\]/, 'Audit must be disabled during timing');
  assert.match(text, /screen=2560x1440 scene=512x288/);
  const lines = text.split('\n'), frames = new Map(), gpu = new Map(), producer = new Map();
  for (const line of lines) {
    const f = line.match(/\[PERF\](?:\[FRAME\])? frame (\d+) total /);
    if (f) frames.set(+f[1], { n: +f[1], time: time(line) });
    const g = line.match(/\[GPU_FRAME_SCOPE\] frame (\d+).*?graphics\.flora_lighting_cache=(\d+)us/);
    if (g) gpu.set(+g[1], { time: time(line), ms: +g[2] / 1000 });
    const update = line.match(/\[GPU_FRAME_SCOPE\] frame (\d+).*?graphics\.local_light_field=(\d+)us/);
    if (update) producer.set(+update[1], +update[2] / 1000);
  }
  const end = [...frames.values()].at(-1)?.time;
  assert.ok(Number.isFinite(end), 'Missing frame advancement / wall time');
  const start = end - 8000;
  if (shared) {
    const ready = lines.findLast(l => l.includes('ready_recorded=true'));
    assert.ok(ready && time(ready) + 1000 < start, 'Shared cache not ready before timing window');
    assert.ok(!lines.some(l => time(l) >= start && l.includes('[LOCAL_VISIBILITY] build')),
      'Do not hide cache rebuild work inside a steady-state comparison');
  }
  const tail = [...frames.values()].filter(f => f.time >= start), first = tail[0], last = tail.at(-1);
  const samples = [...gpu.values()].filter(g => g.time >= start && g.time <= end).map(g => g.ms);
  assert.ok(samples.length >= minimumSamples, `Only ${samples.length} GPU samples; need ${minimumSamples}`);
  const began = [...frames.values()][0];
  return { grassGpuMs: median(samples), gpuSamples: samples.length,
    actualFps: 1000 * (last.n - first.n) / (last.time - first.time),
    frames: last.n - first.n, elapsedMs: last.time - first.time,
    wholeProfiledFps: 1000 * (last.n - began.n) / (last.time - began.time),
    producerGpuTotalMs: [...producer.values()].reduce((a, b) => a + b, 0), producerDispatches: producer.size };
}
function verdict(rows) {
  const summary = Object.fromEntries(['off', 'on'].map(mode => {
    const r = rows.filter(row => row.mode === mode);
    return [mode, { grassGpuMs: median(r.map(v => v.grassGpuMs)),
      actualFps: median(r.map(v => v.actualFps)), wholeProfiledFps: median(r.map(v => v.wholeProfiledFps)), runs: r.length }];
  }));
  const passed = summary.on.grassGpuMs <= summary.off.grassGpuMs && summary.on.actualFps >= summary.off.actualFps &&
    summary.on.wholeProfiledFps >= summary.off.wholeProfiledFps;
  console.log(JSON.stringify({ passed, summary }, null, 2));
  if (!passed) console.error('FAIL: shared mode is slower than the original path');
  return { passed, summary };
}
let options;
try {
  options = parseArgs({ options: { binary: { type: 'string', default: 'target/release/re-flora' },
    output: { type: 'string' }, repeats: { type: 'string', default: '3' }, seconds: { type: 'string', default: '30' },
    analyze: { type: 'string' }, help: { type: 'boolean', short: 'h' } } }).values;
  if (options.help) { console.log(help); process.exit(0); }
  if (!Number.isInteger(+options.repeats) || +options.repeats < 1 || !Number.isFinite(+options.seconds) || +options.seconds < 25)
    throw Error('Require --repeats >=1 integer and --seconds >=25');
  if (options.analyze && process.argv.some(a => ['--binary', '--output', '--repeats', '--seconds'].includes(a)))
    throw Error('--analyze cannot be combined with run options');
} catch (e) { console.error(`${e.message}\n${help}`); process.exit(2); }
try {
  if (options.analyze) {
    const rows = ['off', 'on'].map(mode => ({ mode,
      ...metrics(readFileSync(path.join(options.analyze, `final-${mode}.log`), 'utf8'), mode === 'on', 1) }));
    process.exitCode = verdict(rows).passed ? 0 : 1;
  } else {
    const binary = path.resolve(options.binary), config = 'config/gui.toml';
    const output = options.output ?? `target/local-visibility-perf/${new Date().toISOString().replaceAll(':', '-')}`;
    assert.ok(!existsSync(output), `Output exists: ${output}; choose a NEW --output directory`);
    mkdirSync(output, { recursive: true });
    const original = readFileSync(config), base = original.toString();
    const setValue = (text, id, value) => {
      const pattern = new RegExp(`(id = "${id}"[\\s\\S]*?\\[section.param.data\\]\\nvalue = )[^\\n]+`);
      assert.match(text, pattern, `Missing saved setting ${id}`);
      return text.replace(pattern, `$1${value}`);
    };
    const hash = p => createHash('sha256').update(readFileSync(p)).digest('hex');
    const binaryHash = hash(binary), cameraHash = hash('config/camera_snapshots.toml'), rows = [];
    try {
      for (let repeat = 0; repeat < +options.repeats; repeat++) {
        for (const mode of repeat % 2 ? ['on', 'off'] : ['off', 'on']) {
          const saved = setValue(setValue(base, 'time_of_day', '0.95'), 'grass_local_light_field', String(mode === 'on'));
          writeFileSync(config, saved);
          const env = { ...process.env, RUST_LOG: 'info', RE_FLORA_GARDEN_PERF: '1' };
          for (const key of Object.keys(env)) if (key.startsWith('RE_FLORA_') && key !== 'RE_FLORA_GARDEN_PERF') delete env[key];
          const run = spawnSync(binary, ['--hidden', '--windowed', '--mute', '--perf', '--garden-seed', '20260519',
            '--environment-probe-spacing-voxels', '32', '--camera-snapshot', 'garden-arrival', '--auto-exit', options.seconds],
          { env, encoding: 'utf8', maxBuffer: 128 * 1024 * 1024 });
          const text = (run.stdout ?? '') + (run.stderr ?? '');
          writeFileSync(path.join(output, `${repeat}-${mode}.log`), text);
          assert.equal(run.status, 0, `Native ${mode} run failed: ${run.error ?? run.status}`);
          assert.equal(readFileSync(config, 'utf8'), saved, 'Runtime changed saved inputs');
          assert.equal(hash(binary), binaryHash, 'Binary changed during benchmark');
          assert.equal(hash('config/camera_snapshots.toml'), cameraHash, 'Camera changed during benchmark');
          const row = { repeat, mode, ...metrics(text, mode === 'on', 50) };
          rows.push(row); console.log(JSON.stringify(row));
          writeFileSync(path.join(output, 'runs.json'), JSON.stringify({ binaryHash, cameraHash, rows }, null, 2));
        }
      }
    } finally { writeFileSync(config, original); }
    const result = verdict(rows);
    writeFileSync(path.join(output, 'summary.json'), JSON.stringify(result, null, 2));
    process.exitCode = result.passed ? 0 : 1;
  }
} catch (e) { console.error(`${e.stack}\nInspect preserved logs; rerun into a NEW --output directory.\n${help}`); process.exitCode = 1; }
