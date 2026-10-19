#!/usr/bin/env node
// Explicit real-game validation, never part of ordinary cargo test.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {parseArgs} from 'node:util';

const help = `Usage: node scripts/validate-sprinkler-model.mjs [--mist | --flow | --wet | --dry | --rotary | --spray] [--seconds <positive-number>]
Check the sprinkler model, surface cache, continuous mist and placement lifecycle.
Runs nine hidden, muted Release scenes with Vulkan synchronization validation:
near, garden, orbit, lifecycle, dense/contrast resolve, 8/16/32 model pixels, and
live cache/resolution controls. Checks actual cached draws, not just saved values.
--mist selects fifteen optical checks instead: back/front light, zero density, inside,
wall occlusion, night, live density, particles off, god rays off, shadows off, resize,
placement lifecycle, matched on/off low-angle hilly-terrain seam regression, and
front-lit mist versus zero density for direct visibility comparison.
--flow selects twenty-one directional-mist checks: flat/top views, original-mode A/B,
cliff on/off/A/B, one-voxel wall/corner/overhead slab, live terrain removal, mode changes, particles
off, shadows off, resize, lifecycle, and the matched low-angle seam regression.
Its GPU readback asserts live density below cliff sources, no path tunnelling through
the thin wall, and resumed transport after the wall is removed. --mist and --flow
are mutually exclusive.
--wet selects fifteen soil checks: 2/4/12/24-voxel patches, 32-voxel X/Z sector-edge
spill, 30/120-Hz dose
partitions, zero/double speed, runtime controls, mist independence, manual Water/tiller,
particles off and resize. It reads actual soil voxels, checks material/coverage guards,
compares spatial coherence and seven independent soil trials per dose setting.
Angular limits constrain patch centers, not their tails; finite support and the
original irrigation radius still bound writes. Dose checks use the whole footprint.
Ambient drying/spreading are isolated; snapshots show the final trial, not an average.
--dry selects six soil checks: 0/0.2/1/3 drying multipliers, live pause/resume,
and active watering/mist while evaporation is paused. It reads real soil state at
frames 60/120/180, verifies material/state bits, and isolates vertical transfer.
Artifacts for --dry: target/soil-drying-review/{*.png,*.log,*.json,summary.json}.
--rotary selects sixteen rotating-head checks: near/top silhouettes, 8/16/32 sampling,
cache/native fallback and live mode switching, directional mist/on/off/legacy,
particles off, resize, placement/removal, one-voxel wall, cliff settling, shared-contact soil and
original-mode soil. Outputs: target/rotary-sprinkler-review/.
--spray selects nine shared spray/soil checks: continuous and pulsed water, closed valve,
invisible mist, live angle/RPM/duty sliders, blocking walls, fast rotation, resize and
placement/removal. Reads GPU first contacts and real soil; confirms zero duty stops both
consumers and invisible mist still waters. Outputs: target/sprinkler-coupling-review/.
Choose at most one suite. None is a performance benchmark.
Also verifies zero water particles and real watering brushes at zero, normal and strong
optical density. The seam check requires ImageMagick (magick). This is not a benchmark.
--seconds is the duration of each run (default: 8); each must reach frame 180.
Requires a current Release binary, a Vulkan desktop session, and the Khronos
validation layer (set VK_LAYER_PATH or VULKAN_SDK if it is not installed globally).
No GUI/camera settings or terrain snapshots are saved. No visible game is launched.
Artifacts: target/sprinkler-review/{*.png,*.log,settings,summary.json};
--mist uses target/sprinkler-mist-review/; --flow uses target/sprinkler-mist-flow-review/;
--wet uses target/sprinkler-wetting-review/ (including voxel statistics).
Example: cargo build --release && node scripts/validate-sprinkler-model.mjs
Recovery for an incomplete fixture: repeat the same suite flag with --seconds 20
Exit: 0 passed/help; 1 runtime/validation failure; 2 invalid arguments.`;
let seconds, mist, flow, wet, dry, rotary, spray;
try {
  const {values} = parseArgs({options: {
    mist: {type: 'boolean', default: false}, flow: {type: 'boolean', default: false},
    wet: {type: 'boolean', default: false}, dry: {type: 'boolean', default: false},
    rotary: {type: 'boolean', default: false}, spray: {type: 'boolean', default: false},
    seconds: {type: 'string', default: '8'}, help: {type: 'boolean', short: 'h'},
  }});
  if (values.help) { console.log(help); process.exit(0); }
  seconds = Number(values.seconds);
  mist = values.mist;
  flow = values.flow;
  wet = values.wet;
  dry = values.dry;
  rotary = values.rotary;
  spray = values.spray;
  assert.ok([mist,flow,wet,dry,rotary,spray].filter(Boolean).length<=1, 'choose only one of --mist, --flow, --wet, --dry, --rotary, or --spray');
  assert.ok(Number.isFinite(seconds) && seconds > 0, '--seconds must be a positive number');
} catch (error) { console.error(`${error.message}\n${help}`); process.exit(2); }

const root = path.resolve(import.meta.dirname, '..');
const optical = mist || flow;
const suiteFlag = spray ? '--spray' : rotary ? '--rotary' : dry ? '--dry' : wet ? '--wet' : flow ? '--flow' : mist ? '--mist' : '';
const output = path.join(root, spray ? 'target/sprinkler-coupling-review' : rotary ? 'target/rotary-sprinkler-review' : dry ? 'target/soil-drying-review' : wet ? 'target/sprinkler-wetting-review' : flow ? 'target/sprinkler-mist-flow-review' : mist ? 'target/sprinkler-mist-review' : 'target/sprinkler-review');
const settings = path.join(output, 'settings');
const hash = file => createHash('sha256').update(fs.readFileSync(path.join(root, file))).digest('hex');
const saved = ['config/gui.toml', 'config/camera_snapshots.toml'];
const before = saved.map(hash);
const runs = [];
let mistSeam, mistCliff, wettingComparison, dryingComparison;
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
function checkCliffTransport() {
  const pixels = ['mist-cliff', 'mist-cliff-off', 'mist-cliff-legacy'].map(name => {
    const result = spawnSync('magick', [path.join(output, `${name}.png`), '-alpha', 'off',
      '-resize', '960x540!', '-depth', '8', 'rgb:-'], {maxBuffer: 8*1024*1024});
    assert.equal(result.status, 0, 'cliff image check needs ImageMagick: install magick and retry --flow');
    return result.stdout;
  });
  const transfer = on => {
    let sum=0, count=0;
    // Below the source/platform top, without HUD. The original base-height gate
    // cannot produce the hanging skirt here; matched off removes the cliff itself.
    for (let y=330; y<430; ++y) for (let x=245; x<635; ++x) for (let c=0; c<3; ++c) {
      const i=(y*960+x)*3+c;
      sum+=Math.abs(on[i]-pixels[1][i]);++count;
    }
    return sum/count;
  };
  const result={flowDisplayTransfer:transfer(pixels[0]), legacyDisplayTransfer:transfer(pixels[2])};
  assert.ok(result.flowDisplayTransfer>0.08 &&
    result.flowDisplayTransfer>result.legacyDisplayTransfer*2+0.03,
    `cliff mist did not visibly extend below source: ${JSON.stringify(result)}`);
  return result;
}
try {
  fs.mkdirSync(settings, {recursive: true});
  fs.writeFileSync(path.join(settings, 'vk_layer_settings.txt'),
    'khronos_validation.validate_sync = true\nkhronos_validation.duplicate_message_limit = 10000\n');
  const binary = path.join(root, 'target/release/re-flora');
  assert.ok(fs.existsSync(binary), 'Release binary missing; run cargo build --release');
  const cases = spray
    ? ['rotary-contact-continuous','rotary-contact-pulse','rotary-contact-off','rotary-contact-invisible',
       'rotary-contact-controls','rotary-contact-wall','rotary-contact-fast','spray-resize','spray-lifecycle']
    : rotary
    ? ['rotary-near','rotary-top','rotary-pixels-8','rotary-pixels-16','rotary-pixels-32',
       'rotary-controls','rotary-mist','rotary-mist-off','rotary-mist-legacy','rotary-no-particles',
       'rotary-resize','rotary-lifecycle','rotary-wall','rotary-cliff','rotary-wet','rotary-wet-original']
    : dry
    ? ['dry-zero','dry-slow','dry-normal','dry-fast','dry-controls','dry-watering']
    : wet
    ? ['wet-2','wet-4','wet-12','wet-24','wet-edge-x','wet-edge-z','wet-30fps','wet-120fps','wet-zero','wet-double',
       'wet-controls','wet-mist','wet-brush','wet-no-particles','wet-resize']
    : flow
    ? ['mist-flow', 'mist-flow-off', 'mist-flow-legacy', 'mist-flow-top', 'mist-cliff', 'mist-cliff-off',
       'mist-cliff-legacy', 'mist-thin-wall', 'mist-thin-wall-off', 'mist-corner', 'mist-corner-off',
       'mist-roof', 'mist-roof-off', 'mist-edit', 'mist-flow-controls',
       'mist-flow-no-particles', 'mist-flow-no-shadows', 'mist-flow-resize', 'lifecycle', 'mist-level', 'mist-level-off']
    : mist
    ? ['mist-back', 'mist-front', 'mist-front-off', 'mist-off', 'mist-inside', 'mist-wall', 'mist-night',
       'mist-controls', 'mist-no-particles', 'mist-no-god-rays', 'mist-no-shadows', 'mist-resize', 'lifecycle',
       'mist-level', 'mist-level-off']
    : ['near', 'garden', 'orbit', 'lifecycle', 'near-dense', 'pixels-8', 'pixels-16', 'pixels-32', 'controls'];
  for (const name of cases) {
    const extraArgs = {
      'mist-no-particles': ['--no-particles'],
      'mist-no-god-rays': ['--no-god-rays'],
      'mist-no-shadows': ['--no-shadows'],
      'mist-resize': ['--resize-lifecycle-test'],
      'mist-flow-no-particles': ['--no-particles'],
      'mist-flow-no-shadows': ['--no-shadows'],
      'mist-flow-resize': ['--resize-lifecycle-test'],
      'rotary-no-particles': ['--no-particles'],
      'rotary-resize': ['--resize-lifecycle-test'],
      'spray-resize': ['--resize-lifecycle-test'],
      'wet-no-particles': ['--no-particles'],
      'wet-resize': ['--resize-lifecycle-test'],
    }[name] ?? [];
    const mode = name==='spray-resize' ? 'rotary-contact-continuous' : name==='spray-lifecycle' ? 'rotary-lifecycle' : name==='rotary-wet' ? 'wet-rotary' : name==='rotary-wet-original' ? 'wet-12' : extraArgs.length > 0 ? (rotary ? 'rotary-mist' : wet ? 'wet-12' : flow ? 'mist-flow' : 'mist-back') : name === 'near-dense' ? 'near' : name;
    const coupled=mode.startsWith('rotary-contact-') || mode==='wet-rotary';
    const soil=mode.startsWith('wet-') && !coupled;
    const env = {...process.env};
    // Do not compose this isolated fixture with unrelated development overrides.
    for (const key of Object.keys(env)) if (key.startsWith('RE_FLORA_')) delete env[key];
    Object.assign(env, {RE_FLORA_SPRINKLER_REVIEW: mode, RUST_LOG: 'info',
      VK_INSTANCE_LAYERS: 'VK_LAYER_KHRONOS_validation', VK_LAYER_SETTINGS_PATH: settings,
      VK_LOADER_DEBUG: 'error,warn,layer'});
    if (soil || coupled) env.RE_FLORA_SPRINKLER_WET_STATS=path.join(output,`${name}.json`);
    if (dry) env.RE_FLORA_SOIL_DRY_STATS=path.join(output,`${name}.json`);
    if (!env.VK_LAYER_PATH && env.VULKAN_SDK)
      env.VK_LAYER_PATH = path.join(env.VULKAN_SDK, 'share/vulkan/explicit_layer.d');
    if (name === 'near-dense') env.RE_FLORA_SCENE_PIXEL_TRYOUT = '1';
    const image = path.join(output, `${name}.png`);
    fs.rmSync(image, {force: true});
    console.log(`validating ${name}`);
    const result = spawnSync(binary, [name.startsWith('mist-level') ? '--windowed' : '--flat-terrain',
      '--hidden', '--mute', ...(soil || dry || coupled ? ['--no-flora'] : []), ...(optical || soil || dry || rotary || spray ? ['--perf'] : []),
      '--screenshot', 'player-default', image, '--screenshot-delay', String(seconds * 0.6),
      '--auto-exit', String(seconds), ...extraArgs], {cwd: root, env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024});
    const log = (result.stdout ?? '') + (result.stderr ?? '');
    fs.writeFileSync(path.join(output, `${name}.log`), log);
    assert.equal(result.status, 0, `${name}: ${result.error ?? 'native run failed'}`);
    assert.match(log, /Insert instance layer.*VK_LAYER_KHRONOS_validation/, `${name}: validation layer not observed`);
    assert.ok(!/\bERROR\b|VUID-|SYNC-HAZARD|panicked at|Validation (Error|Warning)/.test(log), `${name}: render/validation errors`);
    assert.match(log, new RegExp(`\\[SPRINKLER_REVIEW\\] passed mode=${mode} frames=180`),
      `${name}: incomplete fixture; retry: node scripts/validate-sprinkler-model.mjs ${suiteFlag} --seconds 20`);
    assert.match(log, /\[SHUTDOWN\] phase=complete failures=0/, `${name}: incomplete shutdown`);
    assert.match(log, /MODEL_VIEW_DRAW\] object=sprinklers.*shader=cached_surface_cells/,
      `${name}: sprinkler did not use shared pre-baked surface cells`);
    if (name.startsWith('rotary-pixels-')) assert.match(log,
      new RegExp(`MODEL_SURFACE_MODE\\] object=sprinklers mode=cached.*resolution=${name.split('-').at(-1)}\\b`));
    if (name.startsWith('pixels-')) assert.match(log,
      new RegExp(`MODEL_SURFACE_MODE\\] object=sprinklers mode=cached.*resolution=${name.slice(7)}\\b`));
    if (name === 'controls' || name==='rotary-controls') {
      assert.match(log, /SPRINKLER_CACHE_REVIEW\] complete steps=7/);
      assert.match(log, /MODEL_VIEW_DRAW\] object=sprinklers.*shader=native_triangle/);
      const steps = [...log.matchAll(/SPRINKLER_CACHE_REVIEW\] step=(\d+) cached=(true|false) resolution=(\d+)/g)]
        .map(m => [Number(m[1]), m[2] === 'true', Number(m[3])]);
      assert.deepEqual(steps, [[0,true,8],[1,true,16],[2,true,32],[3,false,32],[4,true,24],[5,true,8],[6,true,32]]);
    }
    assert.match(log, /SPRINKLER_MIST_REVIEW\] water_particles=0/);
    if (name==='rotary-controls') {
      const steps=[...log.matchAll(/SPRINKLER_ROTARY_REVIEW\] step=(\d+) enabled=(true|false)/g)].map(m=>[+m[1],m[2]==='true']);
      assert.deepEqual(steps,[[0,true],[1,false],[2,true],[3,false],[4,true]]);
    }
    if (coupled) {
      assert.match(log,/SPRINKLER_CONTACT_REVIEW/);
      assert.match(log,/sprinkler_spray.prepare=/);
      const stats=JSON.parse(fs.readFileSync(path.join(output,name+'.json')));
      assert.equal(stats.mode,mode);
      if(mode.endsWith('-off')) assert.equal(stats.totalMoisture,0);
      else if(mode.endsWith('-wall')) {
        assert.ok(stats.radialWet[0]>0,'walls removed unblocked near water');
        assert.equal(stats.outsideWalls,0,'water escaped the solid enclosure');
      } else {
        assert.ok(stats.radialWet.every(n=>n>0),'spray missed a near/middle/far band');
        assert.ok(stats.underSource>0,'spray still leaves a dry inner ring');
      }
      if(mode.endsWith('-off')) assert.deepEqual(stats.contacts,[]);
      if(mode.endsWith('-invisible')) {
        assert.doesNotMatch(log,/sprinkler_mist.pass=/);
        assert.match(log,/sprinkler_spray.wetting=/);
      }
      if(mode.endsWith('-controls')) {
        assert.match(log,/stopped_air_and_water=true/);
        const steps=[...log.matchAll(/SPRINKLER_CONTACT_CONTROL\] step=(\d+) angle=([\d.]+) rpm=([\d.]+) duty=([\d.]+)/g)]
          .map(m=>m.slice(1).map(Number));
        assert.deepEqual(steps,[[0,10,0,100],[1,90,60,100],[2,40,15,0],[3,40,15,50],[4,40,15,100]]);
      }
    }
    if (soil) {
      assert.match(log,/SPRINKLER_WETTING_REVIEW/);
      const stats=JSON.parse(fs.readFileSync(path.join(output,`${name}.json`)));
      assert.equal(stats.mode,mode);
      assert.ok(stats.receivers>1000);
      if(name!=='wet-controls' && name!=='wet-brush') assert.equal(stats.trials,7);
      if(name==='wet-zero') assert.equal(stats.footprintMean,0);
      else assert.ok(stats.footprintMean>0);
      if(name.startsWith('wet-edge-')) assert.ok(stats.outsideSectorMoisture>0,
        `${name}: real wet patches were hard-clipped at the sector edge`);
      if(name==='wet-controls') {
        const controls=[...log.matchAll(/SPRINKLER_WETTING_CONTROL\] step=(\d+) scale=([\d.]+) speed=([\d.]+)/g)]
          .map(m=>[+m[1],+m[2],+m[3]]);
        assert.deepEqual(controls,[[0,2,1],[1,4,1],[2,12,1],[3,24,1.5],[4,24,0]]);
      }
    }
    if (dry) {
      const stats=JSON.parse(fs.readFileSync(path.join(output,`${name}.json`)));
      assert.equal(stats.mode,mode);
      assert.ok(stats.receivers>1000);
      assert.deepEqual(stats.samples.map(x=>x.frame),[60,120,180]);
      assert.ok(stats.samples.every(x=>Number.isFinite(x.mean) && x.mean>=0 && x.mean<=3));
      if(name==='dry-watering') {
        assert.ok(stats.samples.at(-1).mean>0,'pausing evaporation disabled irrigation');
        assert.match(log,/sprinkler_mist.pass=/,'pausing evaporation disabled mist');
      }
      if(name==='dry-controls') {
        assert.deepEqual(stats.samples.map(x=>x.speed),[1,0,3]);
        assert.equal(stats.samples[1].mean,stats.samples[0].mean,'live evaporation pause failed');
        assert.ok(stats.samples[2].mean<stats.samples[1].mean,'live evaporation resume failed');
      }
    }
    if (name === 'mist-controls') {
      const brushes = log.match(/watering_brushes=\[(\d+), (\d+), (\d+)\]/);
      assert.ok(brushes && brushes.slice(1).every(n => Number(n) > 0),
        'watering did not record brushes at every mist density');
    }
    if (name.endsWith('-no-particles')) assert.match(log, /particles_enabled=false/);
    if (optical && name.endsWith('-off')) assert.doesNotMatch(log, /sprinkler_mist.pass=|sprinkler_spray.prepare=/);
    else if (optical) assert.match(log, /sprinkler_mist.pass=/);
    if(rotary && (mode.includes('mist') || ['rotary-wall','rotary-cliff'].includes(name)) && !name.endsWith('-off')) {
      assert.match(log,/sprinkler_mist.pass=/);
      if(!name.endsWith('-legacy')) assert.match(log,/SPRINKLER_FLOW_REVIEW\] gpu_lobes=[1-9]/);
    }
    if(rotary && name.endsWith('-off')) {
      assert.doesNotMatch(log,/sprinkler_mist.pass=/);
      assert.match(log,/sprinkler_spray.prepare=/);
    }
    if (name.endsWith('-legacy') && !rotary) assert.doesNotMatch(log, /sprinkler_spray.prepare=/);
    if (flow && !name.endsWith('-off') && !name.endsWith('-legacy') && name !== 'lifecycle')
      assert.match(log, /SPRINKLER_FLOW_REVIEW\] gpu_lobes=[1-9]/);
    if (name === 'mist-cliff' || name==='rotary-cliff') assert.match(log, /below_source=[1-9]/);
    if (name === 'mist-thin-wall' || name==='rotary-wall') assert.match(log, /beyond_wall=0\b/);
    if (name === 'mist-corner') assert.match(log, /beyond_corner=0\b/);
    if (name === 'mist-roof') assert.match(log, /above_roof=0\b/);
    if (name === 'mist-edit') {
      assert.match(log, /terrain_edit=removed_wall frame=90/);
      assert.match(log, /beyond_wall=[1-9]/);
    }
    if (name === 'mist-flow-controls') {
      const steps = [...log.matchAll(/SPRINKLER_FLOW_REVIEW\] step=(\d+) flow=(true|false)/g)]
        .map(m => [Number(m[1]), m[2] === 'true']);
      assert.deepEqual(steps, [[0,true],[1,false],[2,true],[3,false],[4,true]]);
    }
    if (name.endsWith('-resize')) {
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
  if (optical) mistSeam = checkMistSeam(path.join(output, 'mist-level.png'), path.join(output, 'mist-level-off.png'));
  if (flow) mistCliff=checkCliffTransport();
  if (wet) {
    const stats=name=>JSON.parse(fs.readFileSync(path.join(output,`${name}.json`)));
    const small=stats('wet-2'), current=stats('wet-12'), large=stats('wet-24');
    assert.ok(current.adjacentDisagreement<small.adjacentDisagreement*0.65,'patch scale did not control wetting coherence');
    assert.ok(large.adjacentDisagreement<current.adjacentDisagreement,'larger wetting scale did not make broader patches');
    const meanChecks={};
    for(const name of ['wet-2','wet-4','wet-24','wet-30fps','wet-120fps']) {
      const sample=stats(name);
      // The center-gated kernel moves water across angular edges. Compare the
      // whole footprint, not just the old interior wedge that intentionally loses
      // some water to its neighbors. Material/finite-support guards run in-app.
      const differences=sample.footprintTrialMeans.map((x,i)=>x-current.footprintTrialMeans[i]);
      const average=differences.reduce((a,b)=>a+b,0)/differences.length;
      const variance=differences.reduce((a,b)=>a+(b-average)**2,0)/(differences.length-1);
      const standardError=Math.sqrt(variance/differences.length);
      // Correlated patches intentionally have higher whole-footprint variance.
      // Use the measured paired-trial uncertainty, not a fixed percentage of a
      // single random image. CPU CDF tests separately enforce unbiased dose.
      const limit=4*standardError+0.02;
      meanChecks[name]={difference:average,standardError,limit};
      assert.ok(Math.abs(average)<limit,`${name}: mean dose differs beyond paired-trial uncertainty (${JSON.stringify(meanChecks[name])})`);
    }
    assert.ok(stats('wet-double').footprintMean>current.footprintMean*1.4,'wetting speed did not increase actual moisture');
    wettingComparison={meanChecks,smallBoundary:small.adjacentDisagreement,newBoundary:current.adjacentDisagreement,
      largeBoundary:large.adjacentDisagreement,
      footprintMeans:Object.fromEntries(['wet-2','wet-4','wet-12','wet-24','wet-30fps','wet-120fps','wet-double'].map(n=>[n,stats(n).footprintMean])),
      outsideSectorMoisture:Object.fromEntries(['wet-edge-x','wet-edge-z'].map(n=>[n,stats(n).outsideSectorMoisture])),
      means:Object.fromEntries(['wet-2','wet-4','wet-12','wet-24','wet-30fps','wet-120fps','wet-double'].map(n=>[n,stats(n).mean]))};
  }
  if (dry) {
    const stats=name=>JSON.parse(fs.readFileSync(path.join(output,`${name}.json`)));
    const means=Object.fromEntries(cases.map(name=>[name,stats(name).samples.at(-1).mean]));
    assert.equal(means['dry-zero'],3,'zero drying did not preserve wet soil');
    assert.ok(means['dry-slow']>means['dry-normal']+0.001,'lower multiplier did not slow drying');
    assert.ok(means['dry-normal']>means['dry-fast']+0.001,'higher multiplier did not accelerate drying');
    dryingComparison={initialMoisture:3,means,controls:stats('dry-controls').samples};
  }
  fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify({saved, hashes: before,
    synchronizationValidation: true, performanceMeasured: false, mistSeam, mistCliff, wettingComparison, dryingComparison, runs}, null, 2) + '\n');
  console.log(`PASS: ${runs.length} native checks; inspect screenshots in ${output}.\nVisual approval and population performance acceptance remain separate.`);
} catch (error) {
  console.error(`${error.message}\nInspect ${output}/*.log. Retry: cargo build --release && node scripts/validate-sprinkler-model.mjs ${suiteFlag} --seconds 20\n${help}`);
  process.exitCode = 1;
} finally {
  if (JSON.stringify(saved.map(hash)) !== JSON.stringify(before)) {
    console.error('Saved settings changed; inspect config/gui.toml and config/camera_snapshots.toml.');
    process.exitCode = 1;
  }
}
