#!/usr/bin/env node
// Native Vulkan correctness/visual fixture; deliberately not a cargo unit test.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';

const help = `Usage: node scripts/validate-stem-sampling.mjs [--seconds <positive-number>]

Capture original, continuous, world-direction and surface-attached flower stems
in hidden muted Release mode, then exercise all modes with fixed-position yaw /
pitch and orbit, wind, density/radius extremes, near-plane clipping and resize.
Requires Cargo, Slang, Vulkan and a desktop display. GUI settings are never saved.
--seconds controls sweep duration (default 12); fixed captures take 4 seconds each.
Artifacts: target/stem-sampling-review/{*.png,*.log,summary.json}
This checks runtime correctness and supplies visual evidence, not performance or
flicker acceptance. Thin direction-sampled branches may still disappear.

Example: env -u WAYLAND_DISPLAY node scripts/validate-stem-sampling.mjs --seconds 20
Exit codes: 0 passed/help; 1 runtime/validation failure; 2 invalid arguments.`;
const args = process.argv.slice(2);
if (args.length === 1 && ['-h', '--help'].includes(args[0])) {
  console.log(help); process.exit(0);
}
const seconds = args.length === 0 ? 12 : Number(args[1]);
if (!(args.length === 0 || (args.length === 2 && args[0] === '--seconds')) ||
    !Number.isFinite(seconds) || seconds <= 0) {
  console.error(`Expected no arguments or --seconds <positive-number>.\n${help}`);
  process.exit(2);
}
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = path.join(root, 'target/stem-sampling-review');
fs.mkdirSync(output, {recursive: true});
const hash = data => createHash('sha256').update(data).digest('hex');
const configHash = () => hash(fs.readFileSync(path.join(root, 'config/gui.toml')));
const before = configHash();
const speciesCount = JSON.parse(fs.readFileSync(path.join(root, 'assets/models/flowers.json'))).flowers.length;
const summary = {configSha256: before, speciesCount, runs: []};
try {
  for (const mode of ['stem-original', 'stem-continuous', 'stem-direction', 'stem-surface', 'stems']) {
    const sweep = mode === 'stems';
    const image = path.join(output, `${mode}.png`);
    if (!sweep) fs.rmSync(image, {force: true});
    const cli = ['run', '--release', '--', '--hidden', '--mute'];
    if (sweep) cli.push('--windowed', '--resize-lifecycle-test', '--auto-exit', String(seconds));
    else cli.push('--screenshot', 'player-default', image, '--screenshot-delay', '3', '--auto-exit', '4');
    console.log(`Checking ${mode}…`);
    const result = spawnSync('cargo', cli, {cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
      env: {...process.env, RE_FLORA_FLOWER_MODEL_REVIEW: mode}});
    const log = `${result.stdout || ''}\n${result.stderr || ''}`;
    const logPath = path.join(output, `${mode}.log`);
    fs.writeFileSync(logPath, log);
    assert.equal(result.status, 0, `${mode}: ${result.error || result.signal || logPath}`);
    const errors = log.split('\n').filter(line => /\bERROR\b|panicked at|VUID-|Validation Error|Validation Warning/.test(line));
    assert.deepEqual(errors, [], `render validation: ${logPath}`);
    assert.match(log, /\[SHUTDOWN\] phase=complete failures=0/);
    assert.equal((log.match(/\[FLOWER_REVIEW_PLANT\]/g) || []).length, speciesCount);
    const run = {mode, log: path.relative(root, logPath)};
    if (sweep) {
      const phases = [...log.matchAll(/\[STEM_REVIEW_PHASE\] phase=(\d+) enabled=(\w+) method=(\d+) motion=(\w+)/g)];
      assert.deepEqual(phases.map(m => +m[1]), Array.from({length: 16}, (_, i) => i),
        `Incomplete sweep; retry --seconds ${Math.max(20, seconds * 2)}; inspect ${logPath}`);
      run.phases = phases.map((m, i) => {
        const section = log.slice(m.index, phases[i + 1]?.index);
        const draws = new Set([...section.matchAll(/\[FLOWER_DRAW\] species=([\w-]+)/g)].map(m => m[1]));
        // Close clipping deliberately permits offscreen plants; ordinary phases
        // must actually submit every species, not just set GUI values.
        if (i !== 14) assert.equal(draws.size, speciesCount, `phase ${i}: missing draws`);
        const cameras = [...section.matchAll(/\[STEM_REVIEW_CAMERA\].*eye=(\[[^\]]+\]) focus=(\[[^\]]+\])/g)];
        assert.ok(cameras.length >= 3, `phase ${i}: missing camera observations`);
        if (i < 4) {
          assert.equal(new Set(cameras.map(m => m[1])).size, 1, 'turn must not secretly orbit');
          assert.ok(new Set(cameras.map(m => m[2])).size > 1, 'turn must change orientation');
        }
        if (i >= 4 && i <= 9) assert.ok(new Set(cameras.map(m => m[1])).size > 1, 'orbit must move camera');
        return {phase: i, enabled: m[2] === 'true', method: +m[3], motion: m[4], species: [...draws]};
      });
      assert.deepEqual(run.phases.slice(0, 4).map(p => [p.enabled, p.method]), [[false, 0], [true, 0], [true, 1], [true, 2]]);
      assert.equal(run.phases[15].enabled, false, 'must return to original after all candidates');
      const resize = log.indexOf('[FLOWER_REVIEW_RESIZE] after_submitted_frames=72');
      assert.ok(resize >= 0 && log.slice(resize).includes('[RESIZE_LIFECYCLE] phase=published'), 'resize after experimental draws');
      // All stem policies are live uniforms. Only the initial head bank builds.
      assert.equal([...log.matchAll(/\[MODEL_CACHE_BUILD\] kind=3\b/g)].length, 1, 'stem switches rebuilt flower heads');
    } else {
      const png = fs.readFileSync(image);
      assert.equal(png.subarray(1, 4).toString(), 'PNG');
      run.image = path.relative(root, image);
      run.dimensions = [png.readUInt32BE(16), png.readUInt32BE(20)];
      assert.ok(run.dimensions.every(n => n > 0));
      run.sha256 = hash(png);
    }
    summary.runs.push(run);
    assert.equal(configHash(), before, 'review changed saved GUI settings');
  }
  fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
  console.log(`PASS: four captures, 16 live phases, both camera motions and clean Vulkan logs.\n${output}/summary.json\nVisual approval and release performance acceptance remain separate.`);
} catch (error) {
  console.error(`${error.message}\nInspect ${output}/; native log: cargo run --release -- --latest-log`);
  process.exitCode = 1;
} finally {
  if (configHash() !== before) {
    console.error('GUI settings changed; inspect config/gui.toml.'); process.exitCode = 1;
  }
}
