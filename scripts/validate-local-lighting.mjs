#!/usr/bin/env node
// Native all-consumer lighting comparison; never overlap other app/GPU jobs.
import {readFileSync,writeFileSync,mkdirSync,existsSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {parseArgs} from 'node:util';
import {pathToFileURL} from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
import {setSavedValue,nativeChecks,perfMetrics,median} from './validate-light-ris.mjs';

const help=`Usage: node scripts/validate-local-lighting.mjs [--case perf|capture] --output NEW_DIR
Build with cargo build --release first. Run from the repository root, exclusively.
Temporarily changes saved GUI values and restores their exact bytes, also on failure.
  perf (default): three alternating 20s on/off pairs at day 0.45 and night 0.95.
    Same eight-light budget, no RIS/visibility-cache experiment, spacing 32, seed 20260519.
    Reports actual GPU grass preparation, frame/wall-clock FPS and final uploaded count.
  capture: same day/night on/off scenes, plus checked/unchecked GUI workbench screenshots.
    Screenshots at 6s, process exit at 10s. No visible game is launched.
--output must not exist. Logs and summary.json are retained for inspection.
Exit 0: evidence collected (a performance regression is reported, not hidden).
Exit 1: native/validation failure; inspect NEW_DIR/*.log before retrying in a new directory.
Exit 2: invalid invocation. No files are pushed and no shutdown is performed.
Examples:
  node scripts/validate-local-lighting.mjs --case perf --output target/local-lighting-perf
  node scripts/validate-local-lighting.mjs --case capture --output target/local-lighting-capture`;

export function comparisonConfig(original,time,enabled) {
  let saved=original;
  for(const [id,value] of Object.entries({time_of_day:time,auto_daynight_cycle:false,
    local_lighting_enabled:enabled,local_light_budget:8,grass_light_ris:false,grass_local_light_field:false}))
    saved=setSavedValue(saved,id,String(value));
  return saved;
}
export function validateCounts(text,enabled,expectedCount=enabled?8:0) {
  nativeChecks(text);
  const counts=[...text.matchAll(/\[LOCAL_LIGHT\]\[LIVE\].*? count=(\d+) capacity=(\d+)/g)];
  assert.ok(counts.length,'Missing live light upload');
  assert.equal(+counts.at(-1)[1],expectedCount,'Unexpected actual uploaded light count');
  if(!enabled)assert.ok(counts.every(m=>+m[1]===0),'Disabled run uploaded local lights');
  assert.doesNotMatch(text,/\[NIGHT_FLOWERS\]/,'Retired flower light provider still running');
}
export function summarize(rows) {
  assert.ok(rows.every(r=>r.scene===rows[0].scene && r.screen===rows[0].screen),'Render dimensions changed');
  return Object.fromEntries(['day','night'].map(period=>[period,Object.fromEntries([true,false].map(enabled=>[
    enabled?'on':'off',Object.fromEntries(['grassGpuMs','actualFps','wholeProfiledFps'].map(key=>[key,median(rows.filter(r=>r.period===period&&r.enabled===enabled).map(r=>r[key]))]))]))]));
}
function main() {
  let options;
  try {
    options=parseArgs({options:{case:{type:'string',default:'perf'},output:{type:'string'},help:{type:'boolean',short:'h'}}}).values;
    if(options.help){console.log(help);return;}
    assert.ok(['perf','capture'].includes(options.case),'Unknown --case');
    assert.ok(options.output,'Missing --output NEW_DIR');
  } catch(e){console.error(`${e.message}\n${help}`);process.exitCode=2;return;}
  const output=path.resolve(options.output),config='config/gui.toml',binary=path.resolve('target/release/re-flora');
  const hash=file=>createHash('sha256').update(readFileSync(file)).digest('hex');
  let original;
  try {
    assert.ok(existsSync(binary),'Missing Release binary; run cargo build --release');
    assert.ok(!existsSync(output),'Output directory exists; choose a NEW_DIR');
    mkdirSync(output,{recursive:true});original=readFileSync(config);
    const binaryHash=hash(binary),cameraHash=hash('config/camera_snapshots.toml'),rows=[];
    const run=(name,time,enabled,gui=false)=>{
      const saved=comparisonConfig(original.toString(),time,enabled);writeFileSync(config,saved);
      const env={...process.env,RUST_LOG:'info'};
      for(const key of Object.keys(env))if(key.startsWith('RE_FLORA_'))delete env[key];
      const perf=options.case==='perf',file=path.join(output,`${name}.png`);
      if(perf)env.RE_FLORA_GARDEN_PERF='1';
      if(gui){env.RE_FLORA_DEBUG_PANEL_REVIEW='1';env.RE_FLORA_DEBUG_SEARCH_REVIEW='local lighting enabled';}
      const args=['--hidden','--windowed','--mute',...(!gui?['--garden-seed','20260519']:[]),'--environment-probe-spacing-voxels','32',
        ...(perf?['--camera-snapshot','garden-arrival','--perf']:['--screenshot',gui?'player-default':'garden-arrival',file,'--screenshot-delay','6']),
        '--auto-exit',perf?'20':'10'];
      const result=spawnSync(binary,args,{env,encoding:'utf8',maxBuffer:256*1024*1024});
      const text=(result.stdout??'')+(result.stderr??'');writeFileSync(path.join(output,`${name}.log`),text);
      assert.equal(result.status,0,`Native run failed: ${name}`);validateCounts(text,enabled,gui?0:enabled?8:0);
      assert.equal(readFileSync(config,'utf8'),saved,'Runtime changed saved inputs');
      assert.equal(hash(binary),binaryHash,'Binary changed');assert.equal(hash('config/camera_snapshots.toml'),cameraHash,'Camera changed');
      return perf?perfMetrics(text,50,enabled?8:0):{file:path.basename(file),sha256:hash(file)};
    };
    for(const [period,time] of [['day',0.45],['night',0.95]]) {
      for(let repeat=0;repeat<(options.case==='perf'?3:1);repeat++)for(const enabled of repeat%2?[false,true]:[true,false]) {
        const row={period,time,repeat,enabled,...run(`${period}-${repeat}-${enabled?'on':'off'}`,time,enabled)};
        rows.push(row);console.log(JSON.stringify(row));writeFileSync(path.join(output,'runs.json'),JSON.stringify({binaryHash,cameraHash,rows},null,2));
      }
    }
    if(options.case==='capture')for(const enabled of [true,false])rows.push({gui:true,enabled,...run(`gui-${enabled?'on':'off'}`,0.45,enabled,true)});
    const summary=options.case==='perf'?summarize(rows):null;
    writeFileSync(path.join(output,'summary.json'),JSON.stringify({binaryHash,cameraHash,case:options.case,rows,summary},null,2)+'\n');
    console.log(JSON.stringify({summary},null,2));
  } catch(e){console.error(`${e.stack}\nInspect ${output}/*.log, then retry with a NEW --output directory.\n${help}`);process.exitCode=1;}
  finally {if(original)writeFileSync(config,original);}
}
if(process.argv[1]&&import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href)main();
