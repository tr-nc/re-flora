#!/usr/bin/env node
// Native synchronization regression, not a performance benchmark.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {parseArgs} from 'node:util';
const help = `Usage: node scripts/validate-vulkan-sync.mjs [--quick]
Validate native Release startup, swapchain resize and grass pixel lifecycle with
VK_LAYER_KHRONOS_validation synchronization checking enabled. Hidden and muted;
no settings are saved. Requires cargo build --release, a Vulkan display and the
Khronos validation layer. No errors/hazards may be suppressed or ignored.
--quick checks ordinary startup only; default also checks resize and pixelized
grass toggles, partial growth and post-render resize at several grid resolutions.
Artifacts: target/vulkan-sync/{settings/vk_layer_settings.txt,*.log,summary.json}.
Examples:
  cargo build --release && node scripts/validate-vulkan-sync.mjs
  node scripts/validate-vulkan-sync.mjs --quick
Exit: 0 successful/help; 1 validation/runtime failure; 2 invalid arguments.`;
let quick;
try {
  const {values} = parseArgs({options:{quick:{type:'boolean',default:false},help:{type:'boolean',short:'h'}}});
  if(values.help){console.log(help);process.exit(0);} quick=values.quick;
} catch(error){console.error(`${error.message}\n${help}`);process.exit(2);}
const root=path.resolve(import.meta.dirname,'..');
const output=path.join(root,'target/vulkan-sync');
const settings=path.join(output,'settings');
const saved=['config/gui.toml','config/camera_snapshots.toml'];
const hash=data=>createHash('sha256').update(data).digest('hex');
const hashes=()=>saved.map(file=>hash(fs.readFileSync(path.join(root,file))));
const before=hashes();
const jobs=[{name:'startup',args:['--auto-exit','0.5']},...(quick?[]:[
  {name:'resize',args:['--windowed','--resize-lifecycle-test','--auto-exit','0.5']},
  {name:'grass-analytic-lifecycle',args:['--windowed','--perf','--authored-flora-bench'],env:{
    RE_FLORA_GRASS_STEM_REVIEW:'mid-both-b',RE_FLORA_STEM_BAND_MODE:'0',
    RE_FLORA_STEM_PIXEL_LIFECYCLE:'1',RE_FLORA_STEM_SAMPLE_FRAMES:'120'}},
  {name:'grass-pixel-lifecycle',args:['--windowed','--perf','--authored-flora-bench'],env:{
    RE_FLORA_GRASS_STEM_REVIEW:'mid-both-b',RE_FLORA_STEM_BAND_MODE:'1',
    RE_FLORA_STEM_PIXEL_LIFECYCLE:'1',RE_FLORA_STEM_SAMPLE_FRAMES:'120'}}])];
const results=[];
try {
  fs.mkdirSync(settings,{recursive:true});
  fs.writeFileSync(path.join(settings,'vk_layer_settings.txt'),'khronos_validation.validate_sync = true\nkhronos_validation.duplicate_message_limit = 10000\n');
  const binary=path.join(root,'target/release/re-flora');
  assert.ok(fs.existsSync(binary),'Release binary missing; run cargo build --release');
  for(const job of jobs){
    const env={...process.env,RUST_LOG:'info',VK_INSTANCE_LAYERS:'VK_LAYER_KHRONOS_validation',
      VK_LAYER_SETTINGS_PATH:settings,VK_LAYER_DUPLICATE_MESSAGE_LIMIT:'10000'};
    for(const key of Object.keys(env))if(key.startsWith('RE_FLORA_')||key==='WAYLAND_DISPLAY')delete env[key];
    Object.assign(env,job.env);
    console.log(`validating ${job.name}`);
    const run=spawnSync(binary,['--hidden','--mute',...job.args],{cwd:root,env,encoding:'utf8',maxBuffer:64*1024*1024});
    const log=(run.stdout??'')+(run.stderr??'');
    fs.writeFileSync(path.join(output,`${job.name}.log`),log);
    assert.equal(run.status,0,`${job.name}: ${run.error??'native run failed'}`);
    assert.match(log,/\[SHUTDOWN\] phase=complete failures=0/,`${job.name}: incomplete shutdown`);
    assert.ok(!/\bERROR\b|VUID|hazard detected|panicked at|Validation (Error|Warning)/.test(log),`${job.name}: inspect its validation log`);
    if(job.name.startsWith('grass-')){
      assert.match(log,/\[GRASS_MODEL_PIXELS\] enabled=true/);
      assert.match(log,new RegExp(`GRASS_MODEL_PIXELS\\].*backend=${job.name==='grass-analytic-lifecycle'?'analytic':'hardware_raster'}`));
      assert.equal((log.match(/STEM_PIXEL_LIFECYCLE\].*resize=/g)??[]).length,3);
      assert.equal((log.match(/\[RESIZE\] published generation=/g)??[]).length,3);
      for(const resolution of [32,45,192,512])assert.match(log,new RegExp(`STEM_PIXEL_LIFECYCLE\\].*resolution=${resolution}`));
    }
    assert.deepEqual(hashes(),before,'Saved user settings changed');
    results.push({name:job.name,status:run.status,validationErrors:0,shutdownFailures:0});
  }
  fs.writeFileSync(path.join(output,'summary.json'),JSON.stringify({synchronizationValidation:true,duplicateLimit:10000,savedConfigSha256:before,results},null,2));
  console.log(`passed ${results.length} native checks; artifacts: ${output}`);
} catch(error){console.error(`${error.message}\nInspect ${output}; rebuild and retry node scripts/validate-vulkan-sync.mjs`);process.exitCode=1;}
finally{assert.deepEqual(hashes(),before,'Saved user settings changed');}
