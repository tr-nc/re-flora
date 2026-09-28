#!/usr/bin/env node
// Native saved-field/cache correctness; deliberately not a unit test or benchmark.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {fileURLToPath} from 'node:url';

const help = `Usage: node scripts/validate-flower-controls.mjs [--seconds <number>]
Hidden muted Release/Vulkan sweep of all eight published flowers, independent
head/height controls, head/whole A/B, 8/32/64px and 8/16/37/512 flower views.
Checks targeted baking, all baked records, independent geometry/depth, current
allocation sizes, ready-slot retirement and resize. Never saves GUI/player data.
Requires Cargo, Slang and a Vulkan desktop session. --seconds defaults to 30,
minimum 15; use 60 if all 18 phases do not finish. Not maximum-VRAM or perf evidence.
Artifacts: target/flower-controls-review/{validation.log,scene.png,summary.json}
Serialize GPU runs, e.g.:
  flock --close /tmp/re-flora-vegi-cache-controls-gpu.lock node scripts/validate-flower-controls.mjs --seconds 30
Exit codes: 0 pass/help, 1 runtime/validation failure, 2 usage error.`;
const args = process.argv.slice(2);
if (args.length === 1 && ['-h', '--help'].includes(args[0])) { console.log(help); process.exit(0); }
const seconds = args.length === 0 ? 30 : Number(args[1]);
if (!(args.length === 0 || (args.length === 2 && args[0] === '--seconds')) || !Number.isFinite(seconds) || seconds < 15) {
  console.error(`Invalid arguments: expected --seconds >= 15.\n${help}`); process.exit(2);
}
process.chdir(fileURLToPath(new URL('../', import.meta.url)));
const dir = 'target/flower-controls-review';
mkdirSync(dir, {recursive: true});
try {
  const before = readFileSync('config/gui.toml');
  const env = {...process.env};
  for (const key of Object.keys(env)) if (key.startsWith('RE_FLORA_')) delete env[key];
  Object.assign(env, {RE_FLORA_FLOWER_MODEL_REVIEW: 'controls', RE_FLORA_MODEL_CACHE_REVIEW: '1', RE_FLORA_VEGETATION_RESPONSE_VALIDATE: '1'});
  if (process.platform === 'linux' && env.DISPLAY) { delete env.WAYLAND_DISPLAY; delete env.WAYLAND_SOCKET; }
  const run = spawnSync('cargo', ['run', '--release', '--', '--hidden', '--mute', '--windowed', '--resize-lifecycle-test',
    '--screenshot', 'player-default', `${dir}/scene.png`, '--screenshot-delay', '6', '--auto-exit', String(seconds)],
    {env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024});
  const text = (run.stdout || '') + (run.stderr || '');
  writeFileSync(`${dir}/validation.log`, text);
  assert.equal(run.status, 0, `App failed: ${run.error || run.signal || dir + '/validation.log'}`);
  assert.doesNotMatch(text, /\bERROR\b|VUID-|panicked at|Validation Error|Validation Warning/);
  assert.deepEqual(readFileSync('config/gui.toml'), before, 'Saved config changed');
  assert.match(text, /Application exited successfully/);
  assert.match(text, /held_pose=passed lifetime_remap=passed/);
  assert.equal([...text.matchAll(/FLOWER_REVIEW_PLANT\] species=/g)].length, 8);
  const phases = [...text.matchAll(/FLOWER_CONTROLS_PHASE\] phase=(\d+) heads_only=(true|false) resolution=(\d+) views=(\d+) head_scale=([\d.]+) height_scale=([\d.]+) size=([\d.]+)/g)];
  assert.deepEqual(phases.map(p => +p[1]), Array.from({length: 18}, (_, i) => i), 'Incomplete sweep: retry --seconds 60');
  const expected = [[0,1,2,3],[3],[],[3],[3],[],[3],[],[],[],[3],[3],[3],[],[3],[],[3],[]];
  const results = [];
  let sawPendingRetirement = false;
  for (let i = 0; i < phases.length; i++) {
    const p = phases[i], segment = text.slice(p.index, phases[i + 1]?.index);
    const builds = [...segment.matchAll(/MODEL_CACHE_BUILD\] kind=(\d+) views=(\d+) resolution=(\d+) shapes=(\d+) bytes=(\d+) blocks=(\d+) resident_bytes=(\d+).*head_scale=([\d.]+) height_scale=([\d.]+)/g)];
    assert.deepEqual(builds.map(b => +b[1]), expected[i], `Phase ${i}: unrelated/missing rebuild`);
    for (const b of builds) {
      assert.equal(+b[5], +b[2] * (+b[3]) ** 2 * (+b[4]) * 32, 'Cache must follow current demand');
      assert.equal(+b[7], +b[5] + +b[6] * 23, 'Only page alignment/table overhead allowed');
      if (+b[1] === 3) {
        assert.equal(+b[2], +p[4]); assert.equal(+b[3], +p[3]);
        assert.equal(+b[8], +p[5]); assert.equal(+b[9], +p[6]);
      }
    }
    const baked = [...segment.matchAll(/MODEL_CACHE_BAKE_CHECK\] kind=(\d+) checked=(\d+) mismatches=(\d+)/g)];
    assert.deepEqual(baked.map(b => +b[1]), expected[i], `Phase ${i}: incomplete bake readback`);
    for (const b of baked) { assert.equal(+b[3], 0); assert.equal(+b[2] * 32, +builds.find(v => v[1] === b[1])[5]); }
    const geometry = [...segment.matchAll(/MODEL_CACHE_GEOMETRY_CHECK\] kind=(\d+) resolution=(\d+) views=(\d+) cases=(\d+) checked_hits=(\d+) centers=(\d+) coverage=(\d+) max_depth_error=([\d.]+)/g)];
    assert.deepEqual(geometry.map(g => +g[1]), expected[i], `Phase ${i}: independent geometry oracle missing`);
    for (const g of geometry) {
      const b = builds.find(v => v[1] === g[1]);
      assert.equal(+g[2], +b[3]); assert.equal(+g[3], +b[2]); assert.equal(+g[4], +b[4] * 2);
      assert.ok(+g[5] > 0 && +g[6] > 0 && +g[7] > 0 && +g[8] < 0.00002);
    }
    const species = new Set([...segment.matchAll(/FLOWER_DRAW\] species=([\w-]+)/g)].map(m => m[1]));
    assert.equal(species.size, 8, `Phase ${i}: all species must draw`);
    const consumption = [...segment.matchAll(/MODEL_CACHE_CONSUMED\].*flower=(\d+)/g)].slice(3).reduce((sum, m) => sum + +m[1], 0);
    assert.ok(consumption > 0, `Phase ${i}: no cached flower consumption`);
    const residency = [...segment.matchAll(/MODEL_CACHE_RESIDENCY\] slot=(\d+) active_bytes=(\d+) retained_bytes=(\d+) direction_count=(\d+) direction_bytes=(\d+)/g)];
    assert.ok(residency.length >= 5, `Phase ${i}: not enough completed slots`);
    for (const r of residency) {
      assert.ok(+r[3] >= +r[2]);
      sawPendingRetirement ||= +r[3] > +r[2];
      assert.equal(+r[4], Math.max(16, +p[4]));
      assert.equal(+r[5], +r[4] * 16);
    }
    const last = residency.at(-1);
    assert.equal(+last[3], +last[2], `Phase ${i}: old surface generation remained resident`);
    results.push({phase: i, headsOnly: p[2] === 'true', resolution: +p[3], views: +p[4], headScale: +p[5], heightScale: +p[6],
      rebuiltKinds: builds.map(b => +b[1]), flowerBytes: builds.filter(b => +b[1] === 3).map(b => +b[5]),
      bakedRecords: baked.map(b => +b[2]), independentHits: geometry.map(g => +g[5]), maxDepthErrors: geometry.map(g => +g[8]),
      activeBytes: +last[2], retainedBytes: +last[3], directionBytes: +last[5], consumedTexels: consumption});
  }
  const readySlots = new Set([...text.matchAll(/MODEL_CACHE_RESIDENCY\] slot=(\d+)/g)].map(m => +m[1]));
  // Production currently uses MAX_FRAMES_IN_FLIGHT=1. In that configuration the
  // previous generation's fence is already complete at every replacement; do
  // not fabricate overlapping-GPU evidence. Multi-slot retention is also tested
  // deterministically in GpuPagedStorage and transient descriptor unit tests.
  if (readySlots.size > 1) assert.ok(sawPendingRetirement, 'Multi-slot run must retain pending generations');
  else assert.equal(readySlots.size, 1, 'Missing ready-slot evidence');
  assert.deepEqual([...text.matchAll(/MODEL_CACHE_DIRECTIONS\] count=(\d+) bytes=(\d+)/g)].map(m => [+m[1], +m[2]]), [[32,512]]);
  const resize = text.indexOf('FLOWER_REVIEW_RESIZE] after_submitted_frames=408');
  assert.ok(resize >= 0 && text.slice(resize).includes('RESIZE_LIFECYCLE] phase=published'), 'Missing post-switch resize');
  for (const m of text.matchAll(/frame_generation=(\d+) swapchain_generation=(\d+) tracer_generation=(\d+)/g)) {
    assert.equal(m[1], m[2]); assert.equal(m[1], m[3]);
  }
  assert.ok(readFileSync(`${dir}/scene.png`).length > 100);
  writeFileSync(`${dir}/summary.json`, JSON.stringify({configUnchanged: true, performanceEvidence: false, readySlotCount: readySlots.size, sawPendingRetirement, results}, null, 2) + '\n');
  console.log(`PASS: 18 flower-control phases, eight species, isolated rebuilds, demand-sized allocation/retirement and resize.\nArtifacts: ${dir}`);
} catch (e) {
  console.error(`${e.message}\nInspect ${dir}/validation.log; use --help for prerequisites or --seconds 60 for incomplete phases.`);
  process.exitCode = 1;
}
