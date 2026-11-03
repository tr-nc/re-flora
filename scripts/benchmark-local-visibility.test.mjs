import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const command = path.resolve('scripts/benchmark-local-visibility.mjs');
function profile(gpu, fps = 10, ready = true, counter = seconds => seconds * fps) {
  const rows = ['[00:00:00.000 INFO] [SCENE_PIXELS] screen=2560x1440 scene=512x288'];
  if (ready) rows.push('[00:00:01.000 INFO] [LOCAL_VISIBILITY] build ready_recorded=true');
  for (let seconds = 10; seconds <= 20; seconds++) {
    const frame = counter(seconds), prefix = `[00:00:${seconds}.000 INFO]`;
    rows.push(`${prefix} [PERF] frame ${frame} total 50ms`);
    rows.push(`${prefix} [GPU_FRAME_SCOPE] frame ${frame} graphics.flora_lighting_cache=${gpu * 1000}us`);
  }
  return rows.join('\n');
}
function compare(off, on) {
  const dir = mkdtempSync(path.join(tmpdir(), 'visibility-gate-'));
  try {
    writeFileSync(path.join(dir, 'final-off.log'), off);
    writeFileSync(path.join(dir, 'final-on.log'), on);
    return spawnSync(process.execPath, [command, '--analyze', dir], { encoding: 'utf8' });
  } finally { rmSync(dir, { recursive: true }); }
}
test('identical original/shared measurements pass', () => {
  const result = compare(profile(3), profile(3));
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).passed, true);
});
test('a slower cache fails even with unchanged frame throughput', () => {
  assert.equal(compare(profile(3), profile(4)).status, 1);
});
test('reduced throughput fails even if grass GPU looks cheaper', () => {
  assert.equal(compare(profile(3), profile(2, 5)).status, 1);
});
test('fast steady state cannot hide a worse whole-run result including construction', () => {
  const on = profile(2, 11, true, seconds => seconds === 10 ? 121 : seconds === 11 ? 122 : seconds * 11);
  const result = compare(profile(3), on);
  assert.equal(result.status, 1);
  const { summary } = JSON.parse(result.stdout);
  assert.ok(summary.on.actualFps > summary.off.actualFps);
  assert.ok(summary.on.wholeProfiledFps < summary.off.wholeProfiledFps);
});
test('missing cache publication is not mistaken for a fast shared path', () => {
  const result = compare(profile(3), profile(2, 10, false));
  assert.equal(result.status, 1);
  assert.match(result.stderr, /not ready before timing window/);
});
test('usage errors remain discoverable without running the app', () => {
  const result = spawnSync(process.execPath, [command, '--repeats', '0'], { encoding: 'utf8' });
  assert.equal(result.status, 2);
  assert.match(result.stderr, /Require --repeats >=1/);
  assert.match(result.stderr, /Usage: node scripts\/benchmark-local-visibility.mjs/);
});
