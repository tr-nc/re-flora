import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { metrics, attribution, analyze, options, sceneDimensions } from '../benchmark-sprinkler-optics.mjs';

function log({ dropped = 0, publication = true } = {}) {
  const lines = ['[23:59:55.000 INFO test] start'];
  if (publication) lines.push('[23:59:57.000 INFO test] [SPRINKLER_PERF_PUBLICATION] instances=16 all_instances_live=true');
  for (let i = 0; i <= 200; i++) {
    const seconds = (55 + i / 10) % 60;
    const stamp = `${i < 50 ? '23:59' : '00:00'}:${seconds.toFixed(3).padStart(6, '0')}`;
    lines.push(`[${stamp} INFO test] [PERF][GPU_FRAME_SCOPE] frame ${i} frame.render=9000us sprinkler_mist.pass=2500us sprinkler_spray.prepare=250us sprinkler_spray.wetting=500us sprinkler_mist.tiles=10us graphics.sprinklers=750us dropped=${dropped}`);
  }
  return lines.join('\n');
}

test('real-log grammar: publication warmup, midnight, ms and per-frame aggregate', () => {
  const result = metrics(log(), 'GPU');
  assert.equal(result.samples, 151);
  assert.equal(result.first_frame, 50);
  assert.equal(result.last_frame, 200);
  assert.equal(result.duration_seconds, 15);
  assert.equal(result.cadence_fps, 10);
  assert.equal(result.scopes['sprinkler_mist.pass'].mean, 2.5);
  assert.ok(Math.abs(result.scopes['sprinkler.sum'].mean - 4.01) < 1e-12);
});

test('reject missing publication, short windows, dropped or incomplete scopes', () => {
  assert.throws(() => metrics(log({ publication: false }), 'GPU'), /publication/);
  assert.throws(() => metrics(log({ dropped: 1 }), 'GPU'), /Dropped/);
  assert.throws(() => metrics(log().split('\n').slice(0, 70).join('\n'), 'GPU'), /samples/);
  assert.throws(() => metrics(log().replaceAll('sprinkler_mist.pass', 'unrelated'), 'GPU'), /Missing GPU scope/);
});

test('shared/interacting costs are not misrepresented as additive independent timers', () => {
  const result = attribution({ shared: 1, mist: 7, droplets: 5, both: 10 }, 20);
  assert.deepEqual(result.alone_increment_ms, { mist: 6, droplets: 4 });
  assert.deepEqual(result.removal_saving_ms, { mist: 5, droplets: 3 });
  assert.equal(result.interaction_ms, -1);
  assert.deepEqual(result.symmetric_attribution_ms, { shared_floor: 1, mist: 5.5, droplets: 3.5 });
  assert.deepEqual(result.symmetric_percent_of_optical, { shared_floor: 10, mist: 55, droplets: 35 });
  assert.equal(result.combined_optical_percent_of_gpu_frame, 50);
  // Preserve noisy negative estimates rather than clamping them into fake savings.
  assert.ok(attribution({ shared: 0.01, mist: 0.02, droplets: 0.015, both: 0.011 }, 1).symmetric_attribution_ms.droplets < 0);
});

test('incomplete factorial groups cannot affect completed attribution', () => {
  const rows = Object.entries({ shared: 1, mist: 7, droplets: 5, both: 10 }).map(([mode, time]) => ({
    count: 16, repeat: 0, mode, cpu: { cadence_fps: 50 }, gpu: { samples: 100,
      scopes: { 'sprinkler_mist.pass': { mean: time, p50: time - 0.5, p90: time + 1 }, 'frame.render': { mean: 20, p50: 19, p90: 22 } },
    },
  }));
  const result = analyze(rows);
  assert.deepEqual(analyze([...rows, { ...rows[3], repeat: 1, gpu: { ...rows[3].gpu, scopes: { ...rows[3].gpu.scopes, 'sprinkler_mist.pass': { mean: 999 } } } }]), result);
  assert.equal(result[0].complete_repeats, 1);
  assert.deepEqual(result[0].symmetric_percent_of_optical, { shared_floor: 10, mist: 55, droplets: 35 });
});

test('capture actual monitor dimensions instead of inheriting an old benchmark resolution', () => {
  for (const [screen, scene] of [['4096x2560', '820x512'], ['5120x2880', '1024x576']]) {
    const line = `[SCENE_PIXELS] screen=${screen} scene=${scene} ratio=25:1 pixel_stride=5`;
    assert.deepEqual(sceneDimensions(`${line}\n${line}`), { screen, scene, pixel_stride: 5 });
    assert.throws(() => sceneDimensions(`${line}\n[SCENE_PIXELS] screen=1600x1000 scene=320x200 pixel_stride=5`), /changed within/);
  }
  assert.throws(() => sceneDimensions(''), /Missing scene dimensions/);
});

test('CLI discovers valid commands and rejects unsafe/misleading matrices before launching', () => {
  assert.equal(options([]).repeats, '3');
  for (const args of [['--counts', '0'], ['--counts', '16,16'], ['--layout', 'unknown'], ['--duty', '0'], ['--repeats', '1'], ['--seconds', 'NaN']]) {
    assert.throws(() => options(args));
  }
  const script = new URL('../benchmark-sprinkler-optics.mjs', import.meta.url);
  const help = spawnSync(process.execPath, [script.pathname, '--help'], { encoding: 'utf8' });
  assert.equal(help.status, 0);
  assert.match(help.stdout, /attribution, NOT an optimization/);
  const bad = spawnSync(process.execPath, [script.pathname, '--counts', '0'], { encoding: 'utf8' });
  assert.equal(bad.status, 2);
  assert.match(bad.stderr, /--counts expects distinct values/);
  assert.match(bad.stderr, /Usage:/);
});
