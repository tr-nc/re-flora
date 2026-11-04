#!/usr/bin/env node
// Opt-in real-output temporal regression; audit GPU work is never perf evidence.
import {spawnSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdirSync,existsSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {parseArgs} from 'node:util';
import {pathToFileURL} from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
import {setSavedValue,nativeChecks,nativeArguments} from './validate-light-ris.mjs';

const help=`Usage: node scripts/validate-light-ris-temporal.mjs --output NEW_DIR [--motion static|wind] [--visibility compact|source] [--light-budget 1..64] [--seconds N]
Build with cargo build --release first; run exclusively from the repository root.
Uses hidden/muted Release, garden-arrival, night 0.95, seed 20260519, spacing 32.
Tracks actual rendered direct light at stable receiver identities in a separate audit.
static (default) suppresses wind deflection/vibration; wind retains saved motion.
--visibility source disables GPU culling to diagnose compaction-order dependence;
it is a diagnostic control, never a production fix or performance comparison.
Requires stationary lit coverage and <=2% reference-normalized static output jumps,
with no changed proposal pool at an unchanged receiver. Dynamic error is reported,
not mistaken for a static reference. Default 12s; --seconds must be >=10.
Temporarily edits config/gui.toml and restores its exact bytes even on failure.
Exit 0: checks passed; 1: native/evidence/temporal failure; 2: invalid invocation.
Logs and summary.json are kept on failure. Retry in a NEW_DIR. Nothing is pushed.`;

export function temporalMetrics(text) {
  nativeChecks(text);
  const rows=[...text.matchAll(/\[LIGHT_RIS\]\[TEMPORAL\] records=(\d+) matched=(\d+) stationary=(\d+) lit_stationary=(\d+) changed=(\d+) pool_changed=(\d+) max_relative_delta=([\d.e+-]+) mean_error_delta=([\d.e+-]+)/g)]
    .map(m=>({records:+m[1],matched:+m[2],stationary:+m[3],litStationary:+m[4],changed:+m[5],poolChanged:+m[6],maxRelativeDelta:+m[7],meanErrorDelta:+m[8]}));
  assert.ok(rows.length>=10,'Missing repeated real receiver observations');
  // Ignore startup: compare the last half of actual readbacks, not process startup.
  const tail=rows.slice(Math.floor(rows.length/2));
  const sum=k=>tail.reduce((n,r)=>n+r[k],0);
  return {frames:tail.length,matched:sum('matched'),stationary:sum('stationary'),litStationary:sum('litStationary'),changed:sum('changed'),poolChanged:sum('poolChanged'),maxRelativeDelta:Math.max(...tail.map(r=>r.maxRelativeDelta)),meanErrorDelta:tail.reduce((s,r)=>s+r.meanErrorDelta*r.matched,0)/Math.max(sum('matched'),1)};
}
export function assertStable(m) {
  assert.ok(m.stationary>=500 && m.litStationary>=30,'Insufficient unchanged lit receiver coverage');
  assert.equal(m.poolChanged,0,'Unchanged receiver switched candidate pools');
  assert.ok(m.maxRelativeDelta<=0.02,`Static rendered lighting jumped by ${m.maxRelativeDelta} of reference`);
}
function main() {
  let o;
  try {
    o=parseArgs({options:{output:{type:'string'},motion:{type:'string',default:'static'},visibility:{type:'string',default:'compact'},'light-budget':{type:'string',default:'8'},seconds:{type:'string',default:'12'},help:{type:'boolean',short:'h'}}}).values;
    if(o.help){console.log(help);return;}
    assert.ok(o.output,'Missing --output NEW_DIR');
    assert.ok(['static','wind'].includes(o.motion),'Unknown --motion');
    assert.ok(['compact','source'].includes(o.visibility),'Unknown --visibility');
    assert.ok(Number.isInteger(+o['light-budget'])&&+o['light-budget']>=1&&+o['light-budget']<=64,'Invalid --light-budget');
    assert.ok(Number.isFinite(+o.seconds)&&+o.seconds>=10,'Invalid --seconds');
  } catch(e){console.error(`${e.message}\n${help}`);process.exitCode=2;return;}
  const output=path.resolve(o.output),config='config/gui.toml',binary=path.resolve('target/release/re-flora');
  let original;
  try {
    assert.ok(!existsSync(output),'Output exists; choose NEW_DIR');
    mkdirSync(output,{recursive:true});original=readFileSync(config);
    const hash=p=>createHash('sha256').update(readFileSync(p)).digest('hex');
    const binaryHash=hash(binary),cameraHash=hash('config/camera_snapshots.toml');
    let saved=original.toString();
    const values={time_of_day:0.95,auto_daynight_cycle:false,local_lighting_enabled:true,grass_local_light_field:false,grass_light_ris:true,local_light_budget:+o['light-budget'],grass_light_ris_rays:4,grass_light_ris_candidates:8,
      ...(o.motion==='static'?{vegetation_response_gain:0,grass_vibration_amplitude_voxels:0}:{})};
    for(const [id,value]of Object.entries(values))saved=setSavedValue(saved,id,String(value));
    writeFileSync(config,saved);
    const env={...process.env,RUST_LOG:'info'};
    for(const k of Object.keys(env))if(k.startsWith('RE_FLORA_'))delete env[k];
    env.RE_FLORA_LIGHT_RIS_TEMPORAL_AUDIT='1';
    if(o.visibility==='source')env.RE_FLORA_GRASS_VISIBILITY='0';
    const result=spawnSync(binary,nativeArguments('audit',+o.seconds),{env,encoding:'utf8',maxBuffer:256*1024*1024});
    const text=(result.stdout??'')+(result.stderr??'');
    writeFileSync(path.join(output,'native.log'),text);
    assert.equal(result.status,0,`Native failure: ${result.error??result.status}`);
    assert.equal(readFileSync(config,'utf8'),saved,'Native run changed saved inputs');
    assert.equal(hash(binary),binaryHash);assert.equal(hash('config/camera_snapshots.toml'),cameraHash);
    const metrics=temporalMetrics(text);
    const counts=[...text.matchAll(/\[LOCAL_LIGHT\]\[LIVE\].*? count=(\d+) capacity=(\d+)/g)];
    const uploadedLights=+(counts.at(-1)?.[1]??0);
    assert.ok(uploadedLights>=Math.min(8,+o['light-budget']),'No actual light upload');
    const report={binaryHash,cameraHash,motion:o.motion,visibility:o.visibility,uploadedLights,...metrics};
    writeFileSync(path.join(output,'summary.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report,null,2));
    assertStable(metrics);
  } catch(e){console.error(`${e.stack}\nInspect ${output}/native.log and summary.json.\n${help}`);process.exitCode=1;}
  finally{if(original)writeFileSync(config,original);}
}
if(process.argv[1]&&import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href)main();
