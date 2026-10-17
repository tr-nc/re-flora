#!/usr/bin/env node
// Explicit real-game validation, never part of ordinary cargo test.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {parseArgs} from 'node:util';

const help = `Usage: node scripts/validate-sprinkler-model.mjs [--mist] [--seconds <positive-number>]
Check the sprinkler model, surface cache, continuous mist and placement lifecycle.
Runs nine hidden, muted Release scenes with Vulkan synchronization validation:
near, garden, orbit, lifecycle, dense/contrast resolve, 8/16/32 model pixels, and
live cache/resolution controls. Checks actual cached draws, not just saved values.
--mist selects fourteen optical checks instead: back/front light, zero density, inside,
wall occlusion, night, live density, particles off, god rays off, shadows off, resize,
placement lifecycle, and a matched on/off low-angle hilly-terrain seam regression.
Also verifies zero water particles and real watering brushes at zero, normal and strong
optical density. The seam check requires ImageMagick (magick). This is not a benchmark.
--seconds is the duration of each run (default: 8); each must reach frame 180.
Requires a current Release binary, a Vulkan desktop session, and the Khronos
validation layer (set VK_LAYER_PATH or VULKAN_SDK if it is not installed globally).
No GUI/camera settings or terrain snapshots are saved. No visible game is launched.
Artifacts: target/sprinkler-review/{*.png,*.log,settings,summary.json};
--mist uses target/sprinkler-mist-review/ instead.
Example: cargo build --release && node scripts/validate-sprinkler-model.mjs
Recovery for an incomplete fixture: node scripts/validate-sprinkler-model.mjs --seconds 20
Exit: 0 passed/help; 1 runtime/validation failure; 2 invalid arguments.`;
let seconds, mist;
try {
  const {values} = parseArgs({options: {
    mist: {type: 'boolean', default: false},
    seconds: {type: 'string', default: '8'}, help: {type: 'boolean', short: 'h'},
  }});
  if (values.help) { console.log(help); process.exit(0); }
  seconds = Number(values.seconds);
  mist = values.mist;
  assert.ok(Number.isFinite(seconds) && seconds > 0, '--seconds must be a positive number');
} catch (error) { console.error(`${error.message}\n${help}`); process.exit(2); }

const root = path.resolve(import.meta.dirname, '..');
const output = path.join(root, mist ? 'target/sprinkler-mist-review' : 'target/sprinkler-review');
const settings = path.join(output, 'settings');
const hash = file => createHash('sha256').update(fs.readFileSync(path.join(root, file))).digest('hex');
const saved = ['config/gui.toml', 'config/camera_snapshots.toml'];
const before = saved.map(hash);
const runs = [];
let mistSeam;
function checkMistSeam(on, off) {
  const pixels = [on, off].map(file => {
    const result = spawnSync('magick', [file, '-alpha', 'off', '-resize', '960x540!',
      '-depth', '8', 'rgb:-'], {maxBuffer: 8*1024*1024});
    assert.equal(result.status, 0, 'mist seam check needs ImageMagick: install magick and retry --mist');
    return result.stdout;
  });
  // The hillside patch is behind the mist and above the nozzle, without flora
  // or HUD. Matched subtraction removes terrain detail; a continuous medium must
  // not acquire a thin dark slice when its receiver crosses a DDGI probe layer.
  const rows = Array.from({length: 540}, (_, y) => {
    let sum = 0;
    for (let x=360; x<450; ++x) for (let c=0; c<3; ++c) {
      const offset=(y*960+x)*3+c;
      sum+=(pixels[0][offset]-pixels[1][offset])*[0.2126,0.7152,0.0722][c];
    }
    return sum/90;
  });
  let peak = {row: 0, drop: 0, relativeDrop: 0};
  for (let y=285; y<320; ++y) {
    const neighbors=(rows[y-3]+rows[y+3])/2;
    const drop=neighbors-rows[y], relativeDrop=drop/Math.max(Math.abs(neighbors),1);
    if (drop>peak.drop && relativeDrop>0.25) peak={row:y,drop,relativeDrop};
  }
  assert.ok(peak.drop<=1.8,
    `horizontal mist seam: ${peak.drop.toFixed(3)} display levels at row ${peak.row}; expected a smooth profile (<=1.8)`);
  return peak;
}
try {
  fs.mkdirSync(settings, {recursive: true});
  fs.writeFileSync(path.join(settings, 'vk_layer_settings.txt'),
    'khronos_validation.validate_sync = true\nkhronos_validation.duplicate_message_limit = 10000\n');
  const binary = path.join(root, 'target/release/re-flora');
  assert.ok(fs.existsSync(binary), 'Release binary missing; run cargo build --release');
  const cases = mist
    ? ['mist-back', 'mist-front', 'mist-off', 'mist-inside', 'mist-wall', 'mist-night',
       'mist-controls', 'mist-no-particles', 'mist-no-god-rays', 'mist-no-shadows', 'mist-resize', 'lifecycle',
       'mist-level', 'mist-level-off']
    : ['near', 'garden', 'orbit', 'lifecycle', 'near-dense', 'pixels-8', 'pixels-16', 'pixels-32', 'controls'];
  for (const name of cases) {
    const extraArgs = {
      'mist-no-particles': ['--no-particles'],
      'mist-no-god-rays': ['--no-god-rays'],
      'mist-no-shadows': ['--no-shadows'],
      'mist-resize': ['--resize-lifecycle-test'],
    }[name] ?? [];
    const mode = extraArgs.length > 0 ? 'mist-back' : name === 'near-dense' ? 'near' : name;
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
    const result = spawnSync(binary, [name.startsWith('mist-level') ? '--windowed' : '--flat-terrain',
      '--hidden', '--mute', ...(mist ? ['--perf'] : []),
      '--screenshot', 'player-default', image, '--screenshot-delay', String(seconds * 0.6),
      '--auto-exit', String(seconds), ...extraArgs], {cwd: root, env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024});
    const log = (result.stdout ?? '') + (result.stderr ?? '');
    fs.writeFileSync(path.join(output, `${name}.log`), log);
    assert.equal(result.status, 0, `${name}: ${result.error ?? 'native run failed'}`);
    assert.match(log, /Insert instance layer.*VK_LAYER_KHRONOS_validation/, `${name}: validation layer not observed`);
    assert.ok(!/\bERROR\b|VUID-|SYNC-HAZARD|panicked at|Validation (Error|Warning)/.test(log), `${name}: render/validation errors`);
    assert.match(log, new RegExp(`\\[SPRINKLER_REVIEW\\] passed mode=${mode} frames=180`),
      `${name}: incomplete fixture; retry with --seconds 20`);
    assert.match(log, /\[SHUTDOWN\] phase=complete failures=0/, `${name}: incomplete shutdown`);
    assert.match(log, /MODEL_VIEW_DRAW\] object=sprinklers.*shader=cached_surface_cells/,
      `${name}: sprinkler did not use shared pre-baked surface cells`);
    if (name.startsWith('pixels-')) assert.match(log,
      new RegExp(`MODEL_SURFACE_MODE\\] object=sprinklers mode=cached.*resolution=${name.slice(7)}\\b`));
    if (name === 'controls') {
      assert.match(log, /SPRINKLER_CACHE_REVIEW\] complete steps=7/);
      assert.match(log, /MODEL_VIEW_DRAW\] object=sprinklers.*shader=native_triangle/);
      const steps = [...log.matchAll(/SPRINKLER_CACHE_REVIEW\] step=(\d+) cached=(true|false) resolution=(\d+)/g)]
        .map(m => [Number(m[1]), m[2] === 'true', Number(m[3])]);
      assert.deepEqual(steps, [[0,true,8],[1,true,16],[2,true,32],[3,false,32],[4,true,24],[5,true,8],[6,true,32]]);
    }
    assert.match(log, /SPRINKLER_MIST_REVIEW\] water_particles=0/);
    if (name === 'mist-controls') {
      const brushes = log.match(/watering_brushes=\[(\d+), (\d+), (\d+)\]/);
      assert.ok(brushes && brushes.slice(1).every(n => Number(n) > 0),
        'watering did not record brushes at every mist density');
    }
    if (name === 'mist-no-particles') assert.match(log, /particles_enabled=false/);
    if (mist && name.endsWith('-off')) assert.doesNotMatch(log, /sprinkler_mist.pass=/);
    else if (mist) assert.match(log, /sprinkler_mist.pass=/);
    if (name === 'mist-resize') {
      assert.match(log, /RESIZE_LIFECYCLE\] phase=requests_complete count=5/);
      const generations = [...log.matchAll(/RESIZE_LIFECYCLE\] phase=frame frame_generation=(\d+)/g)];
      assert.ok(new Set(generations.map(match => match[1])).size >= 2,
        'mist descriptors were not exercised across extent generations');
    }
    const png = fs.readFileSync(image);
    assert.equal(png.subarray(1, 4).toString(), 'PNG', `${name}: missing capture`);
    assert.deepEqual(saved.map(hash), before, 'Saved settings changed');
    runs.push({name, dimensions: [png.readUInt32BE(16), png.readUInt32BE(20)],
      validationErrors: 0, framesChecked: 180, image: path.relative(root, image)});
  }
  if (mist) mistSeam = checkMistSeam(path.join(output, 'mist-level.png'), path.join(output, 'mist-level-off.png'));
  fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify({saved, hashes: before,
    synchronizationValidation: true, performanceMeasured: false, mistSeam, runs}, null, 2) + '\n');
  console.log(`PASS: ${runs.length} native checks; inspect screenshots in ${output}.\nVisual approval and population performance acceptance remain separate.`);
} catch (error) {
  console.error(`${error.message}\nInspect ${output}/*.log; rebuild with cargo build --release and retry.\n${help}`);
  process.exitCode = 1;
} finally {
  if (JSON.stringify(saved.map(hash)) !== JSON.stringify(before)) {
    console.error('Saved settings changed; inspect config/gui.toml and config/camera_snapshots.toml.');
    process.exitCode = 1;
  }
}
