#!/usr/bin/env node
// Explicit real-game validation, never part of ordinary cargo test.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {parseArgs} from 'node:util';

const help = `Usage: node scripts/validate-sprinkler-model.mjs [--seconds <positive-number>]
Check the native sprinkler model, water outlets and place/remove/re-place lifecycle.
Runs five hidden, muted Release scenes with Vulkan synchronization validation:
near, garden, orbit, lifecycle and near with the existing dense/contrast resolve.
--seconds is the duration of each run (default: 8); each must reach frame 180.
Requires a current Release binary, a Vulkan desktop session, and the Khronos
validation layer (set VK_LAYER_PATH or VULKAN_SDK if it is not installed globally).
No GUI/camera settings or terrain snapshots are saved. No visible game is launched.
Artifacts: target/sprinkler-review/{*.png,*.log,settings,summary.json}.
Example: cargo build --release && node scripts/validate-sprinkler-model.mjs
Recovery for an incomplete fixture: node scripts/validate-sprinkler-model.mjs --seconds 20
Exit: 0 passed/help; 1 runtime/validation failure; 2 invalid arguments.`;
let seconds;
try {
  const {values} = parseArgs({options: {
    seconds: {type: 'string', default: '8'}, help: {type: 'boolean', short: 'h'},
  }});
  if (values.help) { console.log(help); process.exit(0); }
  seconds = Number(values.seconds);
  assert.ok(Number.isFinite(seconds) && seconds > 0, '--seconds must be a positive number');
} catch (error) { console.error(`${error.message}\n${help}`); process.exit(2); }

const root = path.resolve(import.meta.dirname, '..');
const output = path.join(root, 'target/sprinkler-review');
const settings = path.join(output, 'settings');
const hash = file => createHash('sha256').update(fs.readFileSync(path.join(root, file))).digest('hex');
const saved = ['config/gui.toml', 'config/camera_snapshots.toml'];
const before = saved.map(hash);
const runs = [];
try {
  fs.mkdirSync(settings, {recursive: true});
  fs.writeFileSync(path.join(settings, 'vk_layer_settings.txt'),
    'khronos_validation.validate_sync = true\nkhronos_validation.duplicate_message_limit = 10000\n');
  const binary = path.join(root, 'target/release/re-flora');
  assert.ok(fs.existsSync(binary), 'Release binary missing; run cargo build --release');
  for (const name of ['near', 'garden', 'orbit', 'lifecycle', 'near-dense']) {
    const mode = name === 'near-dense' ? 'near' : name;
    const env = {...process.env};
    // Do not compose this isolated fixture with unrelated development overrides.
    for (const key of Object.keys(env)) if (key.startsWith('RE_FLORA_')) delete env[key];
    Object.assign(env, {RE_FLORA_SPRINKLER_REVIEW: mode, RUST_LOG: 'info',
      VK_INSTANCE_LAYERS: 'VK_LAYER_KHRONOS_validation', VK_LAYER_SETTINGS_PATH: settings,
      VK_LOADER_DEBUG: 'error,warn,layer'});
    if (!env.VK_LAYER_PATH && env.VULKAN_SDK)
      env.VK_LAYER_PATH = path.join(env.VULKAN_SDK, 'share/vulkan/explicit_layer.d');
    if (name === 'near-dense') env.RE_FLORA_SCENE_PIXEL_TRYOUT = '1';
    const image = path.join(output, `${name}.png`);
    fs.rmSync(image, {force: true});
    console.log(`validating ${name}`);
    const result = spawnSync(binary, ['--flat-terrain', '--hidden', '--mute',
      '--screenshot', 'player-default', image, '--screenshot-delay', String(seconds * 0.6),
      '--auto-exit', String(seconds)], {cwd: root, env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024});
    const log = (result.stdout ?? '') + (result.stderr ?? '');
    fs.writeFileSync(path.join(output, `${name}.log`), log);
    assert.equal(result.status, 0, `${name}: ${result.error ?? 'native run failed'}`);
    assert.match(log, /Insert instance layer.*VK_LAYER_KHRONOS_validation/, `${name}: validation layer not observed`);
    assert.ok(!/\bERROR\b|VUID-|SYNC-HAZARD|panicked at|Validation (Error|Warning)/.test(log), `${name}: render/validation errors`);
    assert.match(log, new RegExp(`\\[SPRINKLER_REVIEW\\] passed mode=${mode} frames=180`),
      `${name}: incomplete fixture; retry with --seconds 20`);
    assert.match(log, /\[SHUTDOWN\] phase=complete failures=0/, `${name}: incomplete shutdown`);
    const png = fs.readFileSync(image);
    assert.equal(png.subarray(1, 4).toString(), 'PNG', `${name}: missing capture`);
    assert.deepEqual(saved.map(hash), before, 'Saved settings changed');
    runs.push({name, dimensions: [png.readUInt32BE(16), png.readUInt32BE(20)],
      validationErrors: 0, framesChecked: 180, image: path.relative(root, image)});
  }
  fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify({saved, hashes: before,
    synchronizationValidation: true, performanceMeasured: false, runs}, null, 2) + '\n');
  console.log(`PASS: five native checks; inspect screenshots in ${output}.\nVisual approval and population performance acceptance remain separate.`);
} catch (error) {
  console.error(`${error.message}\nInspect ${output}/*.log; rebuild with cargo build --release and retry.\n${help}`);
  process.exitCode = 1;
} finally {
  if (JSON.stringify(saved.map(hash)) !== JSON.stringify(before)) {
    console.error('Saved settings changed; inspect config/gui.toml and config/camera_snapshots.toml.');
    process.exitCode = 1;
  }
}
