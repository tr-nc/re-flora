#!/usr/bin/env node
// Native correctness/lifecycle check, not a performance or visual acceptance test.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { parseArgs } from 'node:util';

const help = `Usage: node scripts/validate-scene-supersampling.mjs [--help]
Validate Release startup and runtime scene pixel controls: 1:1/4:1/16:1/64:1,
4/16 source pixels, both color resolves, native capping and odd/tiny windows. Hidden and muted;
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
    const plans = [...log.matchAll(/\[SCENE_PIXELS\] screen=(\d+)x(\d+) scene=(\d+)x(\d+) ratio=(\d+):1 pixel_stride=(\d+) pixels=(\d+)x(\d+) requested_samples=(\d+) samples_per_axis=(\d+) filter=(\S+)/g)].map(match => ({
      screen: [+match[1], +match[2]], scene: [+match[3], +match[4]], ratio: +match[5], stride: +match[6],
      pixels: [+match[7], +match[8]], requestedSamples: +match[9], samplesPerAxis: +match[10], filter: match[11],
    }));
    assert.ok(plans.length > 0, `${name}: no sampling plans logged`);
    for (const plan of plans) {
      assert.ok([1, 2, 4].includes(plan.samplesPerAxis));
      assert.equal(plan.ratio, plan.stride * plan.stride);
      assert.deepEqual(plan.pixels, plan.screen.map(size => Math.max(1, Math.ceil(size / plan.stride))));
      assert.equal(plan.samplesPerAxis, Math.min(Math.sqrt(plan.requestedSamples), plan.stride));
      assert.deepEqual(plan.scene, plan.pixels.map(size => size * plan.samplesPerAxis));
      assert.ok(plan.filter === (plan.samplesPerAxis === 1 ? 'point' : `box${plan.samplesPerAxis}x${plan.samplesPerAxis}`) || (plan.samplesPerAxis > 1 && plan.filter === 'contrast'));
    }
    if (lifecycle) {
      assert.match(log, /SCENE_SUPERSAMPLING_REVIEW\] phase=complete frames=66/);
      for (const ratio of [1, 4, 16, 64]) assert.ok(plans.some(plan => plan.ratio === ratio), `missing ratio ${ratio}:1`);
      for (const axis of [1, 2, 4]) assert.ok(plans.some(plan => plan.samplesPerAxis === axis), `missing sampling axis ${axis}`);
      for (const axis of [2, 4]) assert.ok(plans.some(plan => plan.samplesPerAxis === axis && plan.filter === 'contrast'), `missing contrast density ${axis}`);
      for (const stride of [2, 4, 8]) assert.ok(plans.some(plan => plan.stride === stride && plan.filter === 'contrast'), `missing contrast group stride ${stride}`);
      assert.ok(plans.some(plan => plan.ratio === 1 && plan.requestedSamples === 16 && plan.samplesPerAxis === 1), 'native AA bypass missing');
      assert.ok(plans.some(plan => plan.ratio === 4 && plan.requestedSamples === 16 && plan.samplesPerAxis === 2), '4:1 AA cap missing');
      assert.ok(plans.some(plan => plan.screen[0] === 1023 && plan.screen[1] === 767 && plan.samplesPerAxis === 4), 'odd-size 16x phase missing');
      assert.ok(plans.some(plan => plan.screen[0] === 9 && plan.screen[1] === 8 && plan.samplesPerAxis === 1), 'one-pixel-high 2D scene missing');
      assert.ok(plans.some(plan => plan.screen[0] === 1280 && plan.screen[1] === 720 && plan.samplesPerAxis === 2), 'resized 4x phase missing');
      const transitions = [...log.matchAll(/\[SCENE_SUPERSAMPLING\] enabled=(true|false) scene_samples=(\d+) frame_extent_generation=(\d+)/g)];
      assert.ok(transitions.length >= 8, 'runtime mode transitions missing');
      assert.equal((log.match(/\[RESIZE\] published generation=/g) ?? []).length, 3, 'sampling toggles must not recreate the swapchain');
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
