#!/usr/bin/env node
// Deterministic native regression; caller owns the GPU lock. The live leaf
// A/B runner remains unchanged and is a separate acceptance gate.
import {spawnSync} from 'node:child_process';
import {mkdir, readFile, writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';

process.chdir(fileURLToPath(new URL('../', import.meta.url)));
const fixture = process.argv[2] || 'leaf-boundary';
assert.ok(['leaf-boundary', 'leaf-small-boundary'].includes(fixture),
  'Usage: node scripts/validate-model-coverage-boundary.mjs [leaf-boundary|leaf-small-boundary]');
const output = process.env.RE_FLORA_MODEL_COVERAGE_OUTPUT ||
  `target/improve-delivery/v4/${fixture}`;
const resolution = fixture === 'leaf-boundary' ? 64 : 16;
await mkdir(output, {recursive: true});
const before = await readFile('config/gui.toml');
const result = spawnSync('cargo', ['run', '--release', '--', '--hidden', '--mute', '--auto-exit', '3'], {
  encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
  env: {...process.env, RE_FLORA_FALLEN_LEAF_REVIEW: 'fixture',
    RE_FLORA_LEAF_MODEL_REVIEW: 'b', RE_FLORA_MODEL_COVERAGE_FIXTURE: fixture, RE_FLORA_MODEL_COVERAGE_OUTPUT: output},
});
const text = (result.stdout || '') + (result.stderr || '');
await writeFile(`${output}/run.log`, text);
assert.equal(result.status, 0, `App/build failed; inspect ${output}/run.log`);
assert.doesNotMatch(text, /\bERROR\b|VUID-|panicked at/, `inspect ${output}/run.log`);
assert.deepEqual(await readFile('config/gui.toml'), before, 'Fixture changed saved settings');
assert.match(text, /MODEL-REPAIR-CHECK\] original_samples=[1-9]\d* original_changed=0 added=[1-9]\d*/);
assert.match(text, new RegExp(`LEAF-MODEL-CHECK\\] mode=B resolution=${resolution} active=1 checked_hits=[1-9]\\d*`));
const tile = await readFile(`${output}/final.bin`);
assert.equal(tile.length, 4096 * 16);
if (resolution === 64) {
  assert.equal(tile.readFloatLE((26 * 64 + 11) * 16 + 12), 1, 'Captured boundary is not the minimized case');
  assert.ok(tile.readFloatLE((26 * 64 + 12) * 16 + 12) < 1, 'Missing genuinely covered neighbor');
  // The captured pose also exposes near-tied coverage depths. Projection refactors
  // must preserve the supporting material, not just the mask and approximate depth.
  // These center-empty pairs share triangle centroids (27 and 26 respectively) in
  // the original producer. Compare actual final RGB bytes: this avoids baking a
  // particular sun/environment intensity into a color golden.
  const center = await readFile(`${output}/center.bin`);
  const pairs = [[[41, 32], [43, 31]], [[42, 32], [41, 30]]];
  const rgb = ([x, y]) => tile.subarray((y * 64 + x) * 16, (y * 64 + x) * 16 + 12);
  for (const pixel of pairs.flat()) {
    const at = (pixel[1] * 64 + pixel[0]) * 16;
    assert.equal(center.readFloatLE(at + 12), 1, `Expected coverage-only pixel ${pixel}`);
    assert.ok(tile.readFloatLE(at + 12) < 1, `Missing material witness ${pixel}`);
    assert.ok(tile.readFloatLE(at) + tile.readFloatLE(at + 4) > 0, `Black material witness ${pixel}`);
  }
  assert.deepEqual(pairs.map(([a, b]) => rgb(a).equals(rgb(b))), [true, true],
    'Coverage supporting triangles changed at (41,32)/(42,32), despite unchanged center samples');
  console.log('PASS: fixed 64px leaf boundary, real producer/readback, independent coverage and depth, original RGBA/depth and supporting materials unchanged.');
} else {
  assert.equal(tile.readFloatLE((10 * 64 + 5) * 16 + 12), 1, 'Captured small boundary changed');
  assert.ok(tile.readFloatLE((10 * 64 + 6) * 16 + 12) < 1, 'Missing genuinely covered neighbor');
  console.log('PASS: fixed 16px small leaf boundary, independent coverage/depth and original RGBA/depth checks.');
}
