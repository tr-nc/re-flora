#!/usr/bin/env node
// Hidden native evidence only. All GPU runs serialize on the shared summer
// lock. In-app fixture drives saved GUI fields; this script never edits config.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const binary = path.join(root, 'target/release/re-flora');
const mode = process.argv[2] ?? 'compare';
assert.ok(['compare', 'grid', 'glass', 'sun', 'gui', 'smoke'].includes(mode), 'Usage: node scripts/validate-ordered-dither.mjs [compare|grid|glass|sun|gui|smoke] (build Release first)');
const output = path.join(root, 'target/ordered-dither-native', mode);
assert.ok(!fs.existsSync(output), `Refusing stale evidence directory: ${output}. Move it aside before retrying.`);
fs.mkdirSync(output, { recursive: true });
const protectedFiles = ['config/gui.toml', 'config/camera_snapshots.toml'].map(p => path.join(root, p));
const before = protectedFiles.map(p => fs.readFileSync(p));
const env = { ...process.env, CARGO_BUILD_JOBS: '2', RUST_LOG: 'info' };
delete env.WAYLAND_DISPLAY;
const sequence = ['compare', 'grid', 'glass', 'sun'].includes(mode);
if (sequence) {
  env.RE_FLORA_ORDERED_DITHER_REVIEW = mode === 'glass' ? 'compare' : mode;
  env.RE_FLORA_ORDERED_DITHER_OUT = output;
} else if (mode === 'gui') {
  env.RE_FLORA_DEBUG_PANEL_REVIEW = '1';
  env.RE_FLORA_DEBUG_SEARCH_REVIEW = 'ordered dithering';
}
const args = ['--hidden', '--mute', '--windowed'];
if (mode === 'glass') args.push('--glass-voxel-test-scene');
if (mode === 'gui') args.push('--screenshot', 'player-default', path.join(output, 'controls.png'), '--screenshot-delay', '2');
args.push('--auto-exit', mode === 'smoke' ? '0.5' : mode === 'gui' ? '4' : '180');
// Keep the latest-log query inside the same lock: another native run can
// otherwise publish its startup log between app exit and our query.
const latestFile = path.join(output, 'latest-log.txt');
const command = ['--close', '/tmp/re-flora-summer-gpu.lock', 'bash', '-c',
  '"$1" "${@:3}"; status=$?; "$1" --latest-log > "$2"; exit "$status"',
  'ordered-dither-native', binary, latestFile, ...args];
try {
  const run = spawnSync('flock', command, { cwd: root, env, encoding: 'utf8', maxBuffer: 128 * 1024 * 1024 });
  fs.writeFileSync(path.join(output, 'stdout.log'), (run.stdout ?? '') + (run.stderr ?? ''));
  assert.equal(run.status, 0, `native run failed: ${run.error ?? run.signal ?? run.status}`);
  assert.ok(!/\bERROR\b|panicked|VUID-|Validation Error/.test((run.stdout ?? '') + (run.stderr ?? '')), 'Inspect stdout.log for native/validation errors');
  const logfile = fs.readFileSync(latestFile, 'utf8').trim();
  assert.ok(path.resolve(logfile).startsWith(path.join(root, 'target/re-flora-logs/')), `Unexpected log outside own worktree: ${logfile}`);
  const log = fs.readFileSync(logfile, 'utf8');
  fs.writeFileSync(path.join(output, 'run.log'), log);
  assert.ok(!/\bERROR\b|panicked|VUID-|Validation Error/.test(log), 'Inspect run.log for native errors');
  assert.match(log, /failures=0/);
  if (sequence) assert.match(log, /\[ORDERED_DITHER_REVIEW\] complete/);
  const images = fs.readdirSync(output).filter(p => p.endsWith('.png')).sort().map(file => {
    const bytes = fs.readFileSync(path.join(output, file));
    assert.equal(bytes.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
    return { file, width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20), sha256: crypto.createHash('sha256').update(bytes).digest('hex') };
  });
  assert.equal(images.length, sequence ? (mode === 'grid' ? 16 : 11) : mode === 'gui' ? 1 : 0);
  if (mode === 'grid') {
    assert.ok(images.some(p => p.width === 1023 && p.height === 767), 'Odd/partial block screenshot missing');
    const plans = [...log.matchAll(/\[SCENE_PIXELS\].*ratio=(\d+):1.*samples_per_axis=(\d+).*filter=(\S+)/g)];
    for (const ratio of [1, 4, 16, 64]) assert.ok(plans.some(p => +p[1] === ratio), `Missing ratio ${ratio}`);
    for (const axis of [1, 2, 4]) assert.ok(plans.some(p => +p[2] === axis), `Missing source axis ${axis}`);
    assert.ok(plans.some(p => p[3] === 'contrast'));
    assert.ok(plans.some(p => p[3].startsWith('box')));
  }
  const phases = log.split('\n').filter(p => p.includes('[ORDERED_DITHER_REVIEW]'));
  const report = { mode, command: ['flock', ...command], validation_layers: env.VK_INSTANCE_LAYERS ?? 'not forced', logfile, images, phases, errors: 0, settings_written: false, performance_measured: false };
  fs.writeFileSync(path.join(output, 'evidence.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(`${mode}: PASS (${images.length} native PNGs, failures=0); ${output}`);
} finally {
  protectedFiles.forEach((p, i) => assert.ok(fs.readFileSync(p).equals(before[i]), `Unexpected saved-file mutation, investigate without discarding: ${p}`));
}
