#!/usr/bin/env node
// Opt-in real-output temporal regression; audit GPU work is never perf evidence.
import {spawnSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdirSync,existsSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {parseArgs} from 'node:util';
import {pathToFileURL} from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
import {setSavedValue,nativeChecks,nativeArguments,stabilityMetrics} from './validate-light-ris.mjs';

const help=`Usage: node scripts/validate-light-ris-temporal.mjs --output NEW_DIR [--motion static|wind|camera|camera-wind] [--visibility compact|source] [--light-budget 1..64] [--seed U32] [--seconds N] [--stability off|on]
Build with cargo build --release first; run exclusively from the repository root.
Uses hidden/muted Release, garden-arrival, night 0.95, seed 20260519, spacing 32.
Tracks actual rendered direct light at stable receiver identities in a separate audit.
static (default) suppresses wind deflection/vibration; wind retains saved motion.
camera/camera-wind use the existing native wheel/capsule round-trip review in the
real garden (frozen/live wind). Default 40s; camera cases require >=30s.
The camera AUDIT alias preserves the garden; historical REVIEW fixtures do not.
--visibility source disables GPU culling to diagnose compaction-order dependence;
it is a diagnostic control, never a production fix or performance comparison.
Requires stationary lit coverage and <=2% reference-normalized static output jumps,
with no changed proposal pool at an unchanged receiver. Moving-receiver changes
are compared to the changing all-light reference. Excess rendered variation over
reference variation must stay within 2% of total reference illumination. Raw error
changes remain reported: references can legitimately jump. Not a pixel-error bound
or human visual acceptance. --stability on tests adaptive exact fallback (default off).
This mode has no radiance history, but no longer promises a fixed ray budget.
Temporarily edits config/gui.toml and restores its exact bytes even on failure.
Exit 0: checks passed; 1: native/evidence/temporal failure; 2: invalid invocation.
Logs and summary.json are kept on failure. Retry in a NEW_DIR. Nothing is pushed.`;

export function temporalMetrics(text,wholeSequence=false) {
  nativeChecks(text);
  const rows=[...text.matchAll(/\[LIGHT_RIS\]\[TEMPORAL\] records=(\d+) matched=(\d+) stationary=(\d+) lit_stationary=(\d+) changed=(\d+) pool_changed=(\d+) max_relative_delta=([\d.e+-]+) mean_error_delta=([\d.e+-]+)(?: lit_matched=(\d+) moving_lit=(\d+) max_moving_error_delta=([\d.e+-]+) mean_lit_error_delta=([\d.e+-]+)(?: max_moving_total_error_delta=([\d.e+-]+)(?: max_total_excess_delta=([\d.e+-]+))?)?)?/g)]
    .map(m=>({records:+m[1],matched:+m[2],stationary:+m[3],litStationary:+m[4],changed:+m[5],poolChanged:+m[6],maxRelativeDelta:+m[7],meanErrorDelta:+m[8],litMatched:+(m[9]??0),movingLit:+(m[10]??0),maxMovingErrorDelta:+(m[11]??0),meanLitErrorDelta:+(m[12]??0),maxMovingTotalErrorDelta:+(m[13]??0),maxTotalExcessDelta:+(m[14]??0)}));
  assert.ok(rows.length>=10,'Missing repeated real receiver observations');
  // Ignore startup: compare the last half of actual readbacks, not process startup.
  const tail=wholeSequence?rows:rows.slice(Math.floor(rows.length/2));
  const sum=k=>tail.reduce((n,r)=>n+r[k],0);
  return {frames:tail.length,matched:sum('matched'),stationary:sum('stationary'),litStationary:sum('litStationary'),changed:sum('changed'),poolChanged:sum('poolChanged'),maxRelativeDelta:Math.max(...tail.map(r=>r.maxRelativeDelta)),meanErrorDelta:tail.reduce((s,r)=>s+r.meanErrorDelta*r.matched,0)/Math.max(sum('matched'),1),litMatched:sum('litMatched'),movingLit:sum('movingLit'),maxMovingErrorDelta:Math.max(...tail.map(r=>r.maxMovingErrorDelta)),meanLitErrorDelta:tail.reduce((s,r)=>s+r.meanLitErrorDelta*r.litMatched,0)/Math.max(sum('litMatched'),1),maxMovingTotalErrorDelta:Math.max(...tail.map(r=>r.maxMovingTotalErrorDelta)),maxTotalExcessDelta:Math.max(...tail.map(r=>r.maxTotalExcessDelta))};
}
export function assertStable(m,stability=false) {
  assert.ok(m.stationary>=500 && m.litStationary>=30,'Insufficient unchanged lit receiver coverage');
  assert.equal(m.poolChanged,0,'Unchanged receiver switched candidate pools');
  if(!stability)assert.ok(m.maxRelativeDelta<=0.02,`Static rendered lighting jumped by ${m.maxRelativeDelta} of reference`);
  assert.ok(m.maxTotalExcessDelta<=0.02,
    `Excess rendered variation was ${m.maxTotalExcessDelta} of total reference illumination`);
}
function main() {
  let o;
  try {
    o=parseArgs({options:{output:{type:'string'},motion:{type:'string',default:'static'},visibility:{type:'string',default:'compact'},'light-budget':{type:'string',default:'8'},seed:{type:'string',default:'20260519'},stability:{type:'string',default:'off'},seconds:{type:'string'},help:{type:'boolean',short:'h'}}}).values;
    if(o.help){console.log(help);return;}
    assert.ok(o.output,'Missing --output NEW_DIR');
    assert.ok(['static','wind','camera','camera-wind'].includes(o.motion),'Unknown --motion');
    assert.ok(['compact','source'].includes(o.visibility),'Unknown --visibility');
    assert.ok(Number.isInteger(+o['light-budget'])&&+o['light-budget']>=1&&+o['light-budget']<=64,'Invalid --light-budget');
    assert.ok(['off','on'].includes(o.stability),'Invalid --stability');
    assert.ok(Number.isInteger(+o.seed)&&+o.seed>=0&&+o.seed<=0xffffffff,'Invalid --seed');
    o.seconds=+(o.seconds??(o.motion.startsWith('camera')?40:12));
    assert.ok(Number.isFinite(o.seconds)&&o.seconds>=(o.motion.startsWith('camera')?30:10),'Invalid --seconds');
  } catch(e){console.error(`${e.message}\n${help}`);process.exitCode=2;return;}
  const output=path.resolve(o.output),config='config/gui.toml',binary=path.resolve('target/release/re-flora');
  let original;
  try {
    assert.ok(!existsSync(output),'Output exists; choose NEW_DIR');
    mkdirSync(output,{recursive:true});original=readFileSync(config);
    const hash=p=>createHash('sha256').update(readFileSync(p)).digest('hex');
    const binaryHash=hash(binary),cameraHash=hash('config/camera_snapshots.toml');
    let saved=original.toString();
    const values={time_of_day:0.95,auto_daynight_cycle:false,local_lighting_enabled:true,grass_local_light_field:false,grass_light_ris:true,grass_light_ris_stability:o.stability==='on',local_light_budget:+o['light-budget'],grass_light_ris_rays:4,grass_light_ris_candidates:8,
      ...(['static','camera'].includes(o.motion)?{vegetation_response_gain:0,grass_vibration_amplitude_voxels:0}:{})};
    for(const [id,value]of Object.entries(values))saved=setSavedValue(saved,id,String(value));
    writeFileSync(config,saved);
    const env={...process.env,RUST_LOG:'info'};
    for(const k of Object.keys(env))if(k.startsWith('RE_FLORA_'))delete env[k];
    env.RE_FLORA_LIGHT_RIS_TEMPORAL_AUDIT='1';
    if(o.visibility==='source')env.RE_FLORA_GRASS_VISIBILITY='0';
    const camera=o.motion.startsWith('camera');
    if(camera)env.RE_FLORA_CAMERA_ZOOM_AUDIT='1';
    const result=spawnSync(binary,nativeArguments('audit',+o.seconds,[],+o.seed),{env,encoding:'utf8',maxBuffer:256*1024*1024});
    const text=(result.stdout??'')+(result.stderr??'');
    writeFileSync(path.join(output,'native.log'),text);
    assert.equal(result.status,0,`Native failure: ${result.error??result.status}`);
    assert.equal(readFileSync(config,'utf8'),saved,'Native run changed saved inputs');
    assert.equal(hash(binary),binaryHash);assert.equal(hash('config/camera_snapshots.toml'),cameraHash);
    if(camera)assert.match(text,/\[CAMERA_ZOOM_REVIEW\] passed/,'Camera round-trip did not complete');
    assert.match(text,/max_total_excess_delta=/,'Missing total-illumination motion evidence');
    const metrics=temporalMetrics(text,camera);
    if(o.motion.includes('wind'))assert.ok(metrics.movingLit>=10,'Insufficient moving lit receiver coverage');
    const counts=[...text.matchAll(/\[LOCAL_LIGHT\]\[LIVE\].*? count=(\d+) capacity=(\d+)/g)];
    const uploadedLights=+(counts.at(-1)?.[1]??0);
    assert.ok(uploadedLights>=Math.min(8,+o['light-budget']),'No actual light upload');
    const adaptive=o.stability==='on'?stabilityMetrics(text):undefined;
    const report={binaryHash,cameraHash,adaptive,motion:o.motion,visibility:o.visibility,seed:+o.seed,stability:o.stability,uploadedLights,...metrics};
    writeFileSync(path.join(output,'summary.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report,null,2));
    assertStable(metrics,o.stability==='on');
  } catch(e){console.error(`${e.stack}\nInspect ${output}/native.log and summary.json.\n${help}`);process.exitCode=1;}
  finally{if(original)writeFileSync(config,original);}
}
if(process.argv[1]&&import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href)main();
