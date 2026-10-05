#!/usr/bin/env node
// Native correctness/lifecycle check, not a performance or visual acceptance test.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { parseArgs } from 'node:util';

const help = `Usage: node scripts/validate-scene-supersampling.mjs [--help]
Validate Release startup and runtime A/B/A scene supersampling, including odd
window dimensions and resize while enabled. Hidden and muted;
the real saved Debug field is edited in memory only. No config files are saved.
Requires: cargo build --release, a Vulkan display and VK_LAYER_KHRONOS_validation
(install/activate the Vulkan SDK if the layer is unavailable).
Artifacts: target/scene-supersampling/{startup.log,lifecycle.log,summary.json}.
Example: cargo build --release && node scripts/validate-scene-supersampling.mjs
Exit: 0 successful/help; 1 runtime/validation failure; 2 invalid arguments.`;
try {
  const { values } = parseArgs({ options: { help: { type: 'boolean', short: 'h' } } });
  if (values.help) { console.log(help); process.exit(0); }
} catch (error) {
  console.error(`${error.message}\n${help}`);
  process.exit(2);
}

const root = path.resolve(import.meta.dirname, '..');
const output = path.join(root, 'target/scene-supersampling');
const binary = path.join(root, 'target/release/re-flora');
const saved = ['config/gui.toml', 'config/camera_snapshots.toml'];
const hashes = () => saved.map(file => createHash('sha256').update(fs.readFileSync(path.join(root, file))).digest('hex'));
const before = hashes();
const results = [];
try {
  assert.ok(fs.existsSync(binary), 'Release binary missing; run cargo build --release');
  fs.mkdirSync(output, { recursive: true });
  const settings = path.join(output, 'settings');
  fs.mkdirSync(settings, { recursive: true });
  fs.writeFileSync(path.join(settings, 'vk_layer_settings.txt'), 'khronos_validation.validate_sync = true\nkhronos_validation.duplicate_message_limit = 10000\n');
  for (const lifecycle of [false, true]) {
    const name = lifecycle ? 'lifecycle' : 'startup';
    const env = { ...process.env, RUST_LOG: 'info', VK_INSTANCE_LAYERS: 'VK_LAYER_KHRONOS_validation', VK_LAYER_SETTINGS_PATH: settings, VK_LOADER_DEBUG: 'error,warn,layer' };
    for (const key of Object.keys(env)) if (key.startsWith('RE_FLORA_') || key === 'WAYLAND_DISPLAY') delete env[key];
    if (!env.VK_LAYER_PATH && env.VULKAN_SDK) env.VK_LAYER_PATH = path.join(env.VULKAN_SDK, 'share/vulkan/explicit_layer.d');
    if (lifecycle) env.RE_FLORA_SCENE_SUPERSAMPLING_REVIEW = '1';
    console.log(`validating ${name}`);
    const run = spawnSync(binary, ['--hidden', '--mute', '--windowed', '--auto-exit', lifecycle ? '8' : '0.5'], {
      cwd: root, env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    });
    const log = (run.stdout ?? '') + (run.stderr ?? '');
    fs.writeFileSync(path.join(output, `${name}.log`), log);
    assert.equal(run.status, 0, `${name}: ${run.error ?? 'native run failed'}`);
    assert.match(log, /\[SHUTDOWN\] phase=complete failures=0/, `${name}: incomplete shutdown`);
    assert.ok(!/\bERROR\b|VUID|hazard detected|panicked at|Validation (Error|Warning)/.test(log), `${name}: inspect validation errors`);
    assert.match(log, /Insert instance layer.*VK_LAYER_KHRONOS_validation/, `${name}: validation layer insertion not observed`);
    const plans = [...log.matchAll(/\[SCENE_PIXELS\] screen=(\d+)x(\d+) scene=(\d+)x(\d+) scale=([\d.]+) pixels=(\d+)x(\d+) samples_per_axis=(\d+) filter=(\S+)/g)].map(match => ({
      screen: [+match[1], +match[2]], scene: [+match[3], +match[4]], scale: +match[5],
      pixels: [+match[6], +match[7]], samplesPerAxis: +match[8], filter: match[9],
    }));
    assert.ok(plans.length > 0, `${name}: no sampling plans logged`);
    for (const plan of plans) {
      assert.ok([1, 2].includes(plan.samplesPerAxis));
      assert.deepEqual(plan.pixels, plan.screen.map(size => Math.max(1, Math.floor(size * plan.scale))));
      assert.deepEqual(plan.scene, plan.pixels.map(size => size * plan.samplesPerAxis));
      assert.equal(plan.filter, plan.samplesPerAxis === 2 ? 'box2x2' : 'point');
    }
    if (lifecycle) {
      assert.match(log, /SCENE_SUPERSAMPLING_REVIEW\] phase=complete frames=36/);
      assert.ok(plans.some(plan => plan.samplesPerAxis === 1));
      assert.ok(plans.some(plan => plan.samplesPerAxis === 2));
      assert.ok(plans.some(plan => plan.screen[0] === 1023 && plan.screen[1] === 767 && plan.samplesPerAxis === 2), 'odd-size B phase missing');
      assert.ok(plans.some(plan => plan.screen[0] === 1280 && plan.screen[1] === 720 && plan.samplesPerAxis === 2), 'resized B phase missing');
      const transitions = [...log.matchAll(/\[SCENE_SUPERSAMPLING\] enabled=(true|false) scene_samples=(\d+) frame_extent_generation=(\d+)/g)];
      assert.ok(transitions.length >= 5, 'runtime mode transitions missing');
      assert.equal((log.match(/\[RESIZE\] published generation=/g) ?? []).length, 2, 'sampling toggles must not recreate the swapchain');
    }
    assert.deepEqual(hashes(), before, 'Saved user settings changed');
    results.push({ name, plans, synchronizationValidation: true, validationErrors: 0 });
  }
  fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify({ savedConfigSha256: before, results }, null, 2));
  console.log(`passed native startup and A/B lifecycle; artifacts: ${output}`);
} catch (error) {
  console.error(`${error.message}\nInspect ${output} and run cargo run --release -- --tail-latest-log 200.\nAfter fixing the failure, retry: cargo build --release && node scripts/validate-scene-supersampling.mjs`);
  process.exitCode = 1;
} finally {
  assert.deepEqual(hashes(), before, 'Saved user settings changed');
}
