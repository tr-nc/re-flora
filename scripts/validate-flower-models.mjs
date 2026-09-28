#!/usr/bin/env node
// Real Vulkan/Release validation, deliberately not a cargo unit test.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';

const help = `Usage: node scripts/validate-flower-models.mjs [--seconds <positive-number>]

Runs hidden, muted Release captures of the fixed single-column voxel stem
and complete terminal head, then an 8/32/64px, view, growth, lifetime and resize
sweep. Legacy capture labels a/b are retained; both use the same selected renderer. --seconds sets the sweep duration (default: 10); captures take 3s each.
Requires Cargo, Slang and a Vulkan-capable desktop session. Never saves settings.
Artifacts: target/flower-native-review/{a,b}.png, logs and summary.json.

Example: node scripts/validate-flower-models.mjs --seconds 15
Exit codes: 0 passed/help; 1 validation/runtime failure; 2 invalid usage.`;
const args = process.argv.slice(2);
if (args.length === 1 && ['-h','--help'].includes(args[0])) {
  console.log(help); process.exit(0);
}
const seconds = args.length === 0 ? 10 : Number(args[1]);
if (!(args.length === 0 || (args.length === 2 && args[0] === '--seconds')) ||
    !Number.isFinite(seconds) || seconds <= 0) {
  console.error(`Invalid arguments: expected no options or --seconds <positive-number>.\n${help}`);
  process.exit(2);
}
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const directory = path.join(root,'target/flower-native-review');
fs.mkdirSync(directory,{recursive:true});
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const config = () => hash(fs.readFileSync(path.join(root,'config/gui.toml')));
const before = config();
const summary = {configSha256:before,pixelBudgets:{A:'one terminal head tile + single-column voxel stem (legacy label)',B:'one terminal head tile + single-column voxel stem (legacy label)'},runs:[]};
try {
  for (const mode of ['a','b','ab']) {
    const image = path.join(directory,`${mode}.png`);
    if (mode !== 'ab') fs.rmSync(image,{force:true});
    const cli = ['run','--release','--','--hidden','--mute'];
    if (mode === 'ab') cli.push('--windowed','--resize-lifecycle-test','--auto-exit',String(seconds));
    else cli.push('--screenshot','player-default',image,'--screenshot-delay','2','--auto-exit','3');
    console.log(`Validating ${mode}…`);
    const result = spawnSync('cargo',cli,{cwd:root,encoding:'utf8',maxBuffer:64*1024*1024,
      env:{...process.env,RE_FLORA_FLOWER_MODEL_REVIEW:mode,RE_FLORA_VEGETATION_RESPONSE_VALIDATE:'1'}});
    const log = `${result.stdout || ''}\n${result.stderr || ''}`;
    const logPath = path.join(directory,`${mode}.log`);
    fs.writeFileSync(logPath,log);
    assert.equal(result.status,0,`${mode} failed: ${result.error || result.signal || logPath}`);
    const diagnostics = log.split('\n').filter(line => /\bERROR\b|panicked at|VUID-|Validation Error|Validation Warning/.test(line));
    const knownPlatformWarnings = diagnostics.filter(line => line.includes('sctk_adwaita::config') && line.includes('XDG Settings Portal'));
    assert.deepEqual(diagnostics.filter(line => !knownPlatformWarnings.includes(line)),[],`render errors: ${logPath}`);
    assert.ok(log.includes('[SHUTDOWN] phase=complete failures=0'),logPath);
    assert.equal((log.match(/\[FLOWER_REVIEW_PLANT\]/g)||[]).length,8,logPath);
    assert.ok(log.includes('held_pose=passed lifetime_remap=passed'),logPath);
    const run = {mode,log:path.relative(root,logPath),knownPlatformWarnings};
    if (mode === 'ab') {
      const phases = [...log.matchAll(/\[FLOWER_REVIEW_PHASE\] phase=(\d+)/g)];
      assert.deepEqual(phases.map(m=>Number(m[1])),[0,1,2,3,4,5,6,7,8],
        `Incomplete sweep; retry node scripts/validate-flower-models.mjs --seconds ${Math.max(20,seconds*2)}`);
      run.phases = phases.map((m,index)=> {
        const segment = log.slice(m.index,phases[index+1]?.index);
        const species = [...new Set([...segment.matchAll(/\[FLOWER_DRAW\] species=([\w-]+)/g)].map(m=>m[1]))];
        assert.equal(species.length,index === 7 ? 7 : 8,`phase ${index} actual draw coverage`);
        return {phase:index,species};
      });
      assert.match(log,/single_light=true views=8.*shared_surfaces=true/);
      assert.match(log,/orthographic=true rotating_pixels=true/);
      assert.match(log,/\[FLOWER_REVIEW_LIFETIME\] removed=1/);
      assert.match(log,/\[FLOWER_REVIEW_LIFETIME\] replanted=1/);
      const resize = log.indexOf('[FLOWER_REVIEW_RESIZE] after_submitted_frames=72');
      assert.ok(resize >= 0 && log.slice(resize).includes('[RESIZE_LIFECYCLE] phase=published'),'resize after flower draws');
      for (const m of log.matchAll(/frame_generation=(\d+) swapchain_generation=(\d+) tracer_generation=(\d+)/g)) {
        assert.equal(m[1],m[2]); assert.equal(m[1],m[3]);
      }
    } else {
      const data = fs.readFileSync(image);
      assert.equal(data.subarray(1,4).toString(),'PNG');
      run.image = path.relative(root,image);
      run.dimensions = [data.readUInt32BE(16),data.readUInt32BE(20)];
      assert.ok(run.dimensions.every(d=>d>0));
      run.sha256 = hash(data);
    }
    summary.runs.push(run);
    assert.equal(config(),before,'diagnostic must not save GUI settings');
  }
  summary.result = 'passed';
  fs.writeFileSync(path.join(directory,'summary.json'),JSON.stringify(summary,null,2)+'\n');
  console.log(`PASS: 8 single-column species, 9 phases, lifetime/resize, clean Vulkan logs.\n${directory}/summary.json\nVisual approval and large-population performance acceptance remain separate.`);
} catch (error) {
  console.error(`${error.message}\nInspect ${directory}/; native log: cargo run --release -- --latest-log`);
  process.exitCode = 1;
} finally {
  if (config() !== before) {console.error('GUI settings changed during validation. Inspect config/gui.toml.');process.exitCode=1;}
}
