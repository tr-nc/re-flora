#!/usr/bin/env node
// Run after cargo build --release. Uses fixed-frame grass fixtures; no GUI save
// or config-file edits. GPU runs are intentionally sequential, never parallel.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';

if (process.argv.includes('--help')) {
  console.log('Usage: node scripts/validate-grass-stem-ab.mjs\nRequires: cargo build --release, working Vulkan display, NVIDIA/other GPU timestamps.\nRuns 10 fixed scenes (including near-plane regression cases), A/B twice (second pair reversed), 120 warmup + 300 sample frames.\nArtifacts: target/grass-stem-ab/{*.png,*.log,summary.json}. Config files are not changed.');
  process.exit(0);
}
assert.equal(process.argv.length, 2, 'No options supported; use --help.');
const root = path.resolve(import.meta.dirname, '..');
const output = path.join(root, 'target/grass-stem-ab');
const binary = path.join(root, 'target/release/re-flora');
assert.ok(fs.existsSync(binary), 'Run cargo build --release first.');
fs.mkdirSync(output, { recursive: true });
const scenes = ['near-both', 'mid-both', 'far-both', 'mid-tall', 'mid-short', 'near-curved', 'top-both', 'low-both', 'inside-both', 'low-curved'];
const runs = [];
function percentile(values, fraction) {
  const sorted = values.toSorted((a,b) => a-b);
  return sorted[Math.min(sorted.length-1, Math.floor(sorted.length*fraction))];
}
for (let repeat = 0; repeat < 2; repeat++) {
  for (const scene of scenes) {
    for (const mode of repeat === 0 ? ['a', 'b'] : ['b', 'a']) {
      const caseName = `${scene}-${mode}`;
      console.log(`running ${caseName} repeat=${repeat+1}`);
      const env = { ...process.env, RE_FLORA_GRASS_STEM_REVIEW: caseName };
      delete env.WAYLAND_DISPLAY;
      if (repeat === 0) env.RE_FLORA_GRASS_STEM_CAPTURE = output;
      else delete env.RE_FLORA_GRASS_STEM_CAPTURE;
      const result = spawnSync(binary, ['--hidden', '--mute', '--windowed', '--perf', '--authored-flora-bench'], {
        cwd: root, env, encoding: 'utf8', maxBuffer: 64*1024*1024, timeout: 120000,
      });
      const log = `${result.stdout ?? ''}${result.stderr ?? ''}`;
      fs.writeFileSync(path.join(output, `${caseName}-${repeat+1}.log`), log);
      assert.equal(result.status, 0, `${caseName} failed: ${result.error ?? log.slice(-3000)}`);
      assert.ok(!/\bERROR\b|VUID|panicked at/.test(log), `${caseName}: inspect errors in log`);
      assert.match(log, /phase=complete failures=0/);
      const start = log.match(/\[GRASS_STEM_REVIEW\].*phase=sample app_frame=(\d+).*grass=\[([^\]]+)\]/);
      const end = log.match(/\[GRASS_STEM_REVIEW\].*phase=complete app_frame=(\d+).*grass=\[([^\]]+)\]/);
      assert.ok(start && end, 'Missing sample boundary');
      assert.equal(start[2], end[2], 'Grass population changed during sample');
      const metrics = { 'frame.render': [], 'graphics.flora': [], 'graphics.flora_lighting_cache': [] };
      const cpu = [];
      for (const line of log.split('\n')) {
        const gpu = line.match(/\[PERF\]\[GPU_FRAME_SCOPE\] frame (\d+) .*dropped=(\d+) (.*)/);
        if (gpu && Number(gpu[1]) >= Number(start[1])+4 && Number(gpu[1]) < Number(end[1])) {
          assert.equal(Number(gpu[2]), 0, 'Dropped GPU scopes');
          for (const metric of Object.keys(metrics)) {
            const escaped = metric.replaceAll('.', '\\.');
            const value = gpu[3].match(new RegExp(`(?:^| )${escaped}=(\\d+)us`));
            assert.ok(value, `Missing ${metric}`);
            metrics[metric].push(Number(value[1]));
          }
        }
        const value = line.match(/\[PERF\] frame (\d+) total ([\d.]+)ms/);
        if (value && Number(value[1]) >= Number(start[1])+4 && Number(value[1]) < Number(end[1])) cpu.push(Number(value[2])*1000);
      }
      assert.ok(metrics['graphics.flora'].length >= 250, 'Insufficient GPU samples');
      if (repeat === 0) assert.ok(fs.existsSync(path.join(output, `${caseName}.png`)), 'Missing screenshot');
      runs.push({ scene, mode, repeat: repeat+1, grass: start[2], metrics, cpu });
      console.log(`  grass GPU p50=${percentile(metrics['graphics.flora'], .5)}us total GPU p50=${percentile(metrics['frame.render'], .5)}us`);
    }
  }
}
const summary = scenes.map(scene => {
  const selected = runs.filter(run => run.scene === scene);
  assert.equal(new Set(selected.map(run => run.grass)).size, 1, 'A/B grass populations differ');
  const modes = Object.fromEntries(['a', 'b'].map(mode => {
    const samples = selected.filter(run => run.mode === mode);
    const timings = Object.fromEntries(Object.keys(samples[0].metrics).map(metric => {
      const values = samples.flatMap(run => run.metrics[metric]);
      return [metric, { samples: values.length, p50_us: percentile(values,.5), p95_us: percentile(values,.95) }];
    }));
    const cpu = samples.flatMap(run => run.cpu);
    timings.cpu_frame = { samples: cpu.length, p50_us: percentile(cpu,.5), p95_us: percentile(cpu,.95) };
    return [mode, timings];
  }));
  return { scene, grass: selected[0].grass, ...modes };
});
// A diagnostic regression alarm for the original tens-of-ms cliff, not a
// release budget or a claim that analytic grass is faster than voxel grass.
const low = summary.find(row => row.scene === 'low-both');
assert.ok(low.b['graphics.flora'].p50_us < 5000, 'Near-plane regression: low-both B exceeds the 5ms diagnostic alarm.');
fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify({ resolution: '2560x1440 (check screenshot dimensions)', warmup_frames: 120, repeats: 2, summary }, null, 2));
console.log(JSON.stringify(summary, null, 2));
