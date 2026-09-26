#!/usr/bin/env node
// Explicit native diagnostic, not a normal unit test. Caller owns the GPU lock.
import {spawnSync} from 'node:child_process';
import {mkdir, readFile, writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
process.chdir(fileURLToPath(new URL('../', import.meta.url)));
const output = 'target/improve-delivery/v4/pose-sweep';
await mkdir(output, {recursive: true});
const before = await readFile('config/gui.toml');
const result = spawnSync('cargo', ['run', '--release', '--', '--hidden', '--mute', '--auto-exit', '6'], {
  encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
  env: {...process.env, RE_FLORA_FALLEN_LEAF_REVIEW: 'fixture', RE_FLORA_LEAF_MODEL_REVIEW: 'b',
    RE_FLORA_MODEL_COVERAGE_FIXTURE: 'leaf-pose-sweep', RE_FLORA_MODEL_COVERAGE_OUTPUT: output},
});
const text = (result.stdout || '') + (result.stderr || '');
await writeFile(`${output}/run.log`, text);
assert.equal(result.status, 0, `App/build failed; inspect ${output}/run.log`);
assert.doesNotMatch(text, /\bERROR\b|VUID-|panicked at/, `inspect ${output}/run.log`);
assert.deepEqual(await readFile('config/gui.toml'), before, 'Fixture changed saved settings');
const batches = [...text.matchAll(/MODEL-COVERAGE-SWEEP\] batch=(\d+)\/15 cases=906/g)].map(m => +m[1]);
assert.deepEqual(batches, Array.from({length: 15}, (_, i) => i + 1), 'Incomplete deterministic sweep');
const checks = [...text.matchAll(/MODEL-REPAIR-CHECK\] original_samples=[1-9]\d* original_changed=0 added=[1-9]\d*/g)];
assert.ok(checks.length >= 15, 'Every batch must finish the strict independent oracle');
assert.match(text, /LEAF-MODEL-CHECK\] mode=B resolution=64 active=10 checked_hits=[1-9]\d*/,
  'The final batch must be validated, not merely submitted');
console.log('PASS: 906 unscreened deterministic cases: both captures and ULP neighbors; three rotation axes, 16 phases, 8/16/64px and 0.25/1/4 sizes.');
