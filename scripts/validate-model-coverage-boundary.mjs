#!/usr/bin/env node
// Deterministic native regression; caller owns the GPU lock. The live leaf
// A/B runner remains unchanged and is a separate acceptance gate.
import {spawnSync} from 'node:child_process';
import {mkdir, readFile, writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';

process.chdir(fileURLToPath(new URL('../', import.meta.url)));
const output = 'target/improve-delivery/v3/coverage-boundary';
await mkdir(output, {recursive: true});
const before = await readFile('config/gui.toml');
const result = spawnSync('cargo', ['run', '--release', '--', '--hidden', '--mute', '--auto-exit', '3'], {
  encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
  env: {...process.env, RE_FLORA_FALLEN_LEAF_REVIEW: 'fixture',
    RE_FLORA_LEAF_MODEL_REVIEW: 'b', RE_FLORA_MODEL_COVERAGE_FIXTURE: 'leaf-boundary'},
});
const text = (result.stdout || '') + (result.stderr || '');
await writeFile(`${output}/run.log`, text);
assert.equal(result.status, 0, `App/build failed; inspect ${output}/run.log`);
assert.doesNotMatch(text, /\bERROR\b|VUID-|panicked at/, `inspect ${output}/run.log`);
assert.deepEqual(await readFile('config/gui.toml'), before, 'Fixture changed saved settings');
assert.match(text, /MODEL-REPAIR-CHECK\] original_samples=[1-9]\d* original_changed=0 added=[1-9]\d*/);
assert.match(text, /LEAF-MODEL-CHECK\] mode=B .*active=1 checked_hits=[1-9]\d*/);
console.log('PASS: fixed 64px leaf boundary, real producer/readback, independent coverage and depth, original RGBA/depth unchanged.');
