#!/usr/bin/env node
// Consumer-only four-way ablation. Hold the garden heavy-resource flock throughout.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const MODES = ['both', 'mist', 'shared', 'droplets'];
const help = `Usage: node scripts/benchmark-sprinkler-optics.mjs [options]
Measure the EXISTING fused optical shader with both layers, mist only, droplets only,
and an empty diagnostic pass. Full source preparation, bounds, tiles, irrigation and
other particles stay enabled in every case. This is attribution, NOT an optimization.

  --binary PATH     Release + no_validation_layer binary (default target/release/re-flora)
  --snapshot PATH   Realized garden (default target/garden-delivery/world/implementation/garden-final.rflterrain)
  --output DIR      NEW/empty evidence directory (default target/sprinkler-optical-attribution/measurement)
  --counts LIST     Comma-separated distinct populations from 1,4,8,16 (default 1,4,8,16)
  --layout CHOICE   spread or overlap (default spread)
  --duty N          30 or 100 percent (default 30)
  --seconds N       App seconds per run, >=20 (default 22)
  --repeats N       Interleaved repetitions, >=3 (default 3)
  --help, -h        Show help without running the app

Uses hidden/muted native fullscreen, garden-arrival camera, saved quality, automatic
present mode. Records the first run's native screen/scene dimensions and requires them
to remain unchanged (no hardcoded old monitor size). Does not write preferences,
camera or snapshots. Requires flowing mist and droplets enabled in saved settings. Warmup ends after publication +1s and at least
5s of profiler samples; at least 100 samples and 12 measured seconds per run are required.
Per-run means/p50/p90, actual cadence, raw logs and binary/input hashes are retained.
Attribution uses means (not sums/differences of percentiles). Symmetric marginal shares
allocate interaction equally; they are NOT direct independent shader timestamps.

Example:
  flock -o /tmp/re-flora-garden-delivery-heavy.lock bash -c 'cargo build --release --features no_validation_layer && node scripts/benchmark-sprinkler-optics.mjs'
Do not overlap any build/app/benchmark. Restore cargo build --release afterward.
Exit: 0 evidence captured, 1 runtime/evidence failure, 2 usage error.`;

const hash = file => createHash('sha256').update(readFileSync(file)).digest('hex');
const mean = values => values.reduce((a, b) => a + b, 0) / values.length;
const quantile = (values, p) => values.toSorted((a, b) => a - b)[Math.floor((values.length - 1) * p)];
const stats = values => ({ mean: mean(values), p50: quantile(values, 0.5), p90: quantile(values, 0.9) });
const stamp = m => +m[1] * 3600 + +m[2] * 60 + +m[3];

export function sceneDimensions(text) {
  const matches = [...text.matchAll(/\[SCENE_PIXELS\] screen=(\d+x\d+) scene=(\d+x\d+).*?pixel_stride=(\d+)/g)];
  assert.ok(matches.length, 'Missing scene dimensions');
  const dimensions = matches.map(m => ({ screen: m[1], scene: m[2], pixel_stride: +m[3] }));
  for (const value of dimensions) assert.deepEqual(value, dimensions[0], 'Scene dimensions changed within the run');
  return dimensions[0];
}

export function metrics(text, kind, additionalSums = {}) {
  const origin = text.match(/\[(\d+):(\d+):(\d+\.\d+) /);
  assert.ok(origin, 'Missing log timestamp');
  const relative = m => (stamp(m) - stamp(origin) + 86400) % 86400;
  const publication = text.match(/\[(\d+):(\d+):(\d+\.\d+) [^\n]*\[SPRINKLER_PERF_PUBLICATION\]/);
  assert.ok(publication, 'Missing all-source GPU publication check');
  const pattern = new RegExp(`\\[(\\d+):(\\d+):(\\d+\\.\\d+) [^\\n]*?\\[PERF\\]\\[${kind}_FRAME_SCOPE\\] frame (\\d+) ([^\\n]+)`, 'g');
  const rows = [...text.matchAll(pattern)].map(m => {
    const scopes = {};
    for (const v of m[5].matchAll(/([\w.]+)=(\d+)us/g)) scopes[v[1]] = (scopes[v[1]] ?? 0) + +v[2] / 1000;
    if (kind === 'GPU') {
      assert.ok('frame.render' in scopes && 'sprinkler_mist.pass' in scopes, 'Missing GPU scope');
      scopes['sprinkler.sum'] = ['sprinkler_spray.prepare', 'sprinkler_spray.wetting', 'sprinkler_mist.tiles', 'sprinkler_mist.pass', 'graphics.sprinklers'].reduce((s, k) => s + (scopes[k] ?? 0), 0);
    }
    for (const [name, keys] of Object.entries(additionalSums)) {
      scopes[name] = keys.reduce((sum, key) => sum + (scopes[key] ?? 0), 0);
    }
    return { time: relative(m), frame: +m[4], scopes, dropped: +(m[5].match(/dropped=(\d+)/)?.[1] ?? 0) };
  });
  assert.ok(rows.length, `No ${kind} frame samples`);
  const start = Math.max(rows[0].time + 5, relative(publication) + 1);
  const warm = rows.filter(r => r.time >= start);
  assert.ok(warm.length >= 100, `Only ${warm.length} warmed ${kind} samples; increase --seconds`);
  const duration = warm.at(-1).time - warm[0].time;
  assert.ok(duration >= 12, `Only ${duration.toFixed(3)} measured seconds; increase --seconds`);
  assert.ok(warm.every(r => r.dropped === 0), 'Dropped profiler scopes');
  assert.ok(warm.slice(1).every((r, i) => r.frame > warm[i].frame && r.time >= warm[i].time), 'Nonmonotonic samples');
  const keys = [...new Set(warm.flatMap(r => Object.keys(r.scopes)))];
  const scopes = Object.fromEntries(keys.map(k => [k, stats(warm.map(r => r.scopes[k] ?? 0))]));
  const intervals = warm.slice(1).map((r, i) => 1000 * (r.time - warm[i].time) / (r.frame - warm[i].frame));
  return { samples: warm.length, first_frame: warm[0].frame, last_frame: warm.at(-1).frame, duration_seconds: duration, scopes, cadence_fps: (warm.at(-1).frame - warm[0].frame) / duration, interval_ms: stats(intervals) };
}

export function attribution(times, frame) {
  const { shared: s, mist: m, droplets: d, both: b } = times;
  const mist = ((m - s) + (b - d)) / 2;
  const droplets = ((d - s) + (b - m)) / 2;
  return {
    optical_ms: times,
    alone_increment_ms: { mist: m - s, droplets: d - s },
    removal_saving_ms: { mist: b - d, droplets: b - m },
    interaction_ms: b - m - d + s,
    symmetric_attribution_ms: { shared_floor: s, mist, droplets },
    symmetric_percent_of_optical: { shared_floor: 100 * s / b, mist: 100 * mist / b, droplets: 100 * droplets / b },
    combined_optical_percent_of_gpu_frame: 100 * b / frame,
  };
}

export function analyze(results) {
  const populations = [...new Set(results.map(r => r.count))];
  return populations.map(count => {
    const rows = results.filter(r => r.count === count);
    const perRepeat = [];
    for (const repeat of [...new Set(rows.map(r => r.repeat))]) {
      const group = rows.filter(r => r.repeat === repeat);
      if (group.length !== 4) continue; // Persist incomplete evidence, never fabricate missing channels.
      assert.equal(new Set(group.map(r => r.mode)).size, 4);
      const times = Object.fromEntries(group.map(r => [r.mode, r.gpu.scopes['sprinkler_mist.pass'].mean]));
      const combined = group.find(r => r.mode === 'both');
      perRepeat.push({ repeat, ...attribution(times, combined.gpu.scopes['frame.render'].mean) });
    }
    if (!perRepeat.length) return { count, complete_repeats: 0 };
    const completed = new Set(perRepeat.map(r => r.repeat));
    const channel = Object.fromEntries(MODES.map(mode => {
      const runs = rows.filter(r => r.mode === mode && completed.has(r.repeat));
      return [mode, {
        optical: Object.fromEntries(['mean', 'p50', 'p90'].map(k => [k, mean(runs.map(r => r.gpu.scopes['sprinkler_mist.pass'][k]))])),
        gpu_frame: Object.fromEntries(['mean', 'p50', 'p90'].map(k => [k, mean(runs.map(r => r.gpu.scopes['frame.render'][k]))])),
        actual_fps: mean(runs.map(r => r.cpu.cadence_fps)),
        samples: runs.map(r => r.gpu.samples),
      }];
    }));
    const times = Object.fromEntries(MODES.map(mode => [mode, mean(perRepeat.map(r => r.optical_ms[mode]))]));
    return { count, complete_repeats: perRepeat.length, channels: channel, ...attribution(times, channel.both.gpu_frame.mean), per_repeat: perRepeat };
  });
}

export function options(argv) {
  const o = parseArgs({ args: argv, options: {
    binary: { type: 'string', default: 'target/release/re-flora' },
    snapshot: { type: 'string', default: 'target/garden-delivery/world/implementation/garden-final.rflterrain' },
    output: { type: 'string', default: 'target/sprinkler-optical-attribution/measurement' },
    counts: { type: 'string', default: '1,4,8,16' },
    layout: { type: 'string', default: 'spread' },
    duty: { type: 'string', default: '30' },
    seconds: { type: 'string', default: '22' },
    repeats: { type: 'string', default: '3' },
    help: { type: 'boolean', short: 'h' },
  } }).values;
  if (o.help) return o;
  assert.ok(['spread', 'overlap'].includes(o.layout), '--layout expects spread|overlap');
  assert.ok(['30', '100'].includes(o.duty), '--duty expects 30|100');
  assert.ok(Number.isFinite(+o.seconds) && +o.seconds >= 20, '--seconds must be finite and >=20');
  assert.ok(Number.isInteger(+o.repeats) && +o.repeats >= 3, '--repeats must be an integer >=3');
  const counts = o.counts.split(',');
  assert.ok(counts.length && counts.every(v => ['1', '4', '8', '16'].includes(v)) && new Set(counts).size === counts.length, '--counts expects distinct values from 1,4,8,16');
  return o;
}

function run(o) {
  const files = ['config/gui.toml', 'config/camera_snapshots.toml', o.snapshot];
  const hashes = () => Object.fromEntries(files.map(file => [file, hash(file)]));
  const inputs = hashes(), binaryHash = hash(o.binary), results = [];
  let scene;
  mkdirSync(o.output, { recursive: true });
  assert.equal(readdirSync(o.output).length, 0, 'Evidence directory must be NEW/empty; choose another --output');
  for (let repeat = 0; repeat < +o.repeats; repeat++) {
    const counts = o.counts.split(',').map(Number);
    if (repeat % 2) counts.reverse();
    const order = [...MODES.slice(repeat % 4), ...MODES.slice(0, repeat % 4)];
    if (repeat % 2) order.reverse();
    for (const count of counts) for (const mode of order) {
      const name = `${o.layout}-d${o.duty}-n${count}-r${repeat}-${mode}`;
      assert.deepEqual(hashes(), inputs, 'Inputs changed');
      assert.equal(hash(o.binary), binaryHash, 'Binary changed during benchmark');
      const env = { ...process.env, RUST_LOG: 'info' };
      for (const key of Object.keys(env)) if (key.startsWith('RE_FLORA_')) delete env[key];
      Object.assign(env, {
        RE_FLORA_GARDEN_PERF: '1', RE_FLORA_SPRINKLER_REVIEW: 'rotary-perf',
        RE_FLORA_SPRINKLER_PERF: `${count},${o.layout},${o.duty}`,
        RE_FLORA_SPRINKLER_OPTICAL_DIAGNOSTIC: mode,
      });
      delete env.VK_INSTANCE_LAYERS; delete env.VK_LAYER_ENABLES; delete env.VK_LOADER_LAYERS_DISABLE;
      if (env.DISPLAY) { delete env.WAYLAND_DISPLAY; delete env.WAYLAND_SOCKET; }
      const child = spawnSync(path.resolve(o.binary), ['--hidden', '--mute', '--perf', '--terrain-load', o.snapshot, '--camera-snapshot', 'garden-arrival', '--auto-exit', o.seconds], { env, encoding: 'utf8', maxBuffer: 128 * 1024 * 1024 });
      const text = (child.stdout ?? '') + (child.stderr ?? '');
      writeFileSync(path.join(o.output, `${name}.log`), text);
      assert.equal(child.status, 0, `${name} exited ${child.status}: ${child.error ?? ''}`);
      assert.doesNotMatch(text, /\bERROR\b|VUID-|SYNC-HAZARD|panicked at|Insert instance layer.*KHRONOS_validation/);
      const dimensions = sceneDimensions(text);
      scene ??= dimensions;
      assert.deepEqual(dimensions, scene, 'Scene dimensions changed across comparison runs');
      assert.match(text, /phase=complete failures=0/);
      assert.ok(text.includes(`[SPRINKLER_OPTICAL_DIAGNOSTIC] channels=${mode} preparation=both bounds=both irrigation=unchanged saved=false`));
      assert.ok(text.includes(`[SPRINKLER_PERF_PUBLICATION] instances=${count} all_instances_live=true`));
      assert.match(text, /\[SPRINKLER_OPTICAL_PUBLICATION\].*droplets_prepared=true/);
      const row = { name, count, mode, repeat, cpu: metrics(text, 'CPU'), gpu: metrics(text, 'GPU') };
      results.push(row);
      assert.deepEqual(hashes(), inputs, 'Inputs changed');
      writeFileSync(path.join(o.output, 'summary.json'), JSON.stringify({ options: o, hashes: inputs, binaryHash, scene, results, analysis: analyze(results) }, null, 2) + '\n');
      console.log(JSON.stringify({ name, samples: row.gpu.samples, optical: row.gpu.scopes['sprinkler_mist.pass'], frame: row.gpu.scopes['frame.render'], fps: row.cpu.cadence_fps }));
    }
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  let o;
  try { o = options(process.argv.slice(2)); }
  catch (e) { console.error(`${e.message}\n${help}`); process.exit(2); }
  if (o.help) { console.log(help); process.exit(0); }
  try { run(o); }
  catch (e) { console.error(`${e.stack}\nEvidence retained in ${o.output}. Inspect the failing log; fix the cause and rerun into a NEW --output directory.\n${help}`); process.exitCode = 1; }
}
