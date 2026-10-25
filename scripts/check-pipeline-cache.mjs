#!/usr/bin/env node
// Opt-in native Vulkan/Release regression, deliberately not part of cargo test.
// Run from the repo: cargo build --release && node scripts/check-pipeline-cache.mjs
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';

const binary = resolve(process.argv[2] ?? 'target/release/re-flora');
mkdirSync('target/pipeline-cache-check', { recursive: true });
const root = mkdtempSync(resolve('target/pipeline-cache-check/run-'));
const cacheDir = join(root, 'cache');
const env = {
  ...process.env,
  RE_FLORA_PIPELINE_CACHE: 'on',
  RE_FLORA_PIPELINE_CACHE_DIR: cacheDir,
  // Keep the NVIDIA implicit cache from hiding a broken application cache.
  __GL_SHADER_DISK_CACHE: '0',
  RUST_LOG: 'warn,re_flora::run_summary=info,re_flora_vkn::pipeline=debug',
};

function run(name, overrides = {}) {
  const result = spawnSync(binary, ['--hidden', '--mute', '--auto-exit', '0.5'], {
    env: { ...env, ...overrides }, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
  });
  const log = (result.stdout ?? '') + (result.stderr ?? '');
  writeFileSync(join(root, `${name}.log`), log);
  assert.equal(result.status, 0, `${name}: ${result.error ?? log}`);
  assert.match(log, /\[STARTUP\] status=ready/);
  assert.doesNotMatch(log, /\bERROR\b|VUID-|SYNC-HAZARD|panicked at/);
  const compute = Number(log.match(/status=done stage=compute_pipelines stage_ms=([\d.]+)/)?.[1]);
  const graphics = Number(log.match(/status=done stage=graphics_pipelines stage_ms=([\d.]+)/)?.[1]);
  const total = Number(log.match(/status=ready .*total_ms=([\d.]+)/)?.[1]);
  assert.ok(Number.isFinite(compute + graphics + total), 'missing stage timings');
  console.log(`${name}: pipelines=${(compute + graphics).toFixed(1)}ms startup=${total}ms`);
  return { log, pipelines: compute + graphics, total };
}

const cold = run('cold');
assert.match(cold.log, /PIPELINE_CACHE\] status=miss/);
assert.match(cold.log, /PIPELINE_CACHE\] status=saved/);
const warm = run('warm');
assert.match(warm.log, /PIPELINE_CACHE\] status=hit/);
// Portable functional assertions above; timing remains evidence, not a universal
// GPU budget. This run prints the measured speedup for the actual driver.
console.log(`pipeline speedup: ${(cold.pipelines / warm.pipelines).toFixed(1)}x`);
const folder = join(cacheDir, 're-flora/pipelines-v1');
const files = readdirSync(folder).filter(name => name.endsWith('.bin'));
assert.equal(files.length, 1);
const path = join(folder, files[0]);
const good = readFileSync(path);

const stale = Buffer.from(good);
stale[8] ^= 1; // Build/device identity, before any driver data is handed to Vulkan.
writeFileSync(path, stale);
assert.match(run('stale-identity').log, /status=miss reason=build\/device\/driver changed/);

const damaged = Buffer.from(good);
damaged[damaged.length - 1] ^= 1;
writeFileSync(path, damaged);
assert.match(run('damaged').log, /status=miss reason=cache checksum mismatch/);
writeFileSync(path, good.subarray(0, 20));
assert.match(run('truncated').log, /status=miss reason=invalid cache length/);
assert.match(run('recovered').log, /PIPELINE_CACHE\] status=hit/);

const blocked = join(root, 'not-a-directory');
writeFileSync(blocked, 'cache failure must not prevent startup');
assert.match(run('unwritable', { RE_FLORA_PIPELINE_CACHE_DIR: blocked }).log, /save failed/);
assert.match(run('disabled', { RE_FLORA_PIPELINE_CACHE: 'off' }).log, /status=disabled/);
console.log(`PASS: native cache lifecycle, stale identity, corruption, truncation, recovery, I/O failure and opt-out. Logs: ${root}`);
