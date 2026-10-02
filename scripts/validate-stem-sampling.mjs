#!/usr/bin/env node
// Native Vulkan correctness/visual fixture; deliberately not a cargo unit test.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';

const help = `Usage: node scripts/validate-stem-sampling.mjs [--seconds <positive-number>]

Capture all four combinations of continuous/surface-cell shading and direction
pixel sampling in hidden muted Release mode, then exercise fixed/orbit/dolly/near
cameras, live wind and resize.
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
  for (const mode of ['stem-continuous', 'stem-direction', 'stem-surface', 'stem-combined', 'stems']) {
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
      const phases = [...log.matchAll(/\[STEM_REVIEW_PHASE\] phase=(\d+) pixelized=(\w+) surface_cells=(\w+)/g)];
      assert.deepEqual(phases.map(m => +m[1]), Array.from({length: 16}, (_, i) => i),
        `Incomplete sweep; retry --seconds ${Math.max(20, seconds * 2)}; inspect ${logPath}`);
      run.phases = phases.map((m, i) => {
        const section = log.slice(m.index, phases[i + 1]?.index);
        const draws = new Set([...section.matchAll(/\[FLOWER_DRAW\] species=([\w-]+)/g)].map(m => m[1]));
        // Close clipping deliberately permits offscreen plants; ordinary phases
        // must actually submit every species, not just set GUI values.
        if (i < 12) assert.equal(draws.size, speciesCount, `phase ${i}: missing draws`);
        assert.equal(m[2] === 'true', i % 4 >= 2);
        assert.equal(m[3] === 'true', i % 2 === 1);
        assert.match(section, /wind=live/);
        return {phase: i, pixelized: m[2] === 'true', surfaceCells: m[3] === 'true', species: [...draws]};
      });
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
  console.log(`PASS: four captures, 16 live phases, independent effects, live wind and clean Vulkan logs.\n${output}/summary.json\nVisual approval and release performance acceptance remain separate.`);
} catch (error) {
  console.error(`${error.message}\nInspect ${output}/; native log: cargo run --release -- --latest-log`);
  process.exitCode = 1;
} finally {
  if (configHash() !== before) {
    console.error('GUI settings changed; inspect config/gui.toml.'); process.exitCode = 1;
  }
}
