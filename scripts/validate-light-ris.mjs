#!/usr/bin/env node
// Native Release evidence. Run exclusively; each case restores exact saved bytes.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { parseArgs } from 'node:util';
import { pathToFileURL } from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';

const help = `Usage: node scripts/validate-light-ris.mjs [--case perf|audit|scaling|capture] [--output NEW_DIR] [--repeats N] [--seconds N] [--light-budget N]
Uses target/release/re-flora, hidden/muted native rendering, local lighting enabled, spacing 32 and night 0.95.
Temporarily changes config/gui.toml and restores its exact bytes, including on failure.
Do not overlap app/build/GPU jobs. Build with cargo build --release first.
  perf (default): alternating original/RIS pairs; default 3 pairs, 20s/run.
    Records GPU grass preparation, actual frame/wall-clock FPS and whole-run FPS.
    Requires >=50 GPU samples in the final 8s. Budgets and dimensions must match.
  audit: default and seed-7 actual receiver readbacks; default 10s each.
    Requires finite, matching normal-program output and nonzero direct-light evidence.
    RGB aggregate error is reported, not confused with exact per-receiver equality.
  scaling: original/RIS, saved upload budget 64; default 180s timeout per run.
    Requires actual accepted counts 1,2,4,8,16,32,64,0 and completed removal.
  capture: original/RIS garden-arrival screenshots at 6s; default 10s/run.
--output must not exist (default: timestamped target/light-ris/ directory).
--repeats applies only to perf; --seconds >=12 for perf, >=8 otherwise.
--light-budget 1..64 (default 8) applies to perf/audit/capture; scaling forces 64.
A higher budget does not invent lights: metrics report the actual final uploaded count.
Exit 0: evidence collected (perf regression is reported as performancePassed=false).
Exit 1: native/validation failure; inspect retained .log files before retrying in a NEW_DIR.
Exit 2: invalid arguments. No visible game is launched and nothing is pushed.
Examples:
  node scripts/validate-light-ris.mjs --case perf --output target/ris-perf
  node scripts/validate-light-ris.mjs --case audit --output target/ris-audit
  node scripts/validate-light-ris.mjs --case scaling --seconds 240 --output target/ris-scale`;

export function setSavedValue(text, id, value) {
  const pattern = new RegExp(`(id = "${id}"[\\s\\S]*?\\[section.param.data\\]\\nvalue = )[^\\n]+`);
  assert.match(text, pattern, `Missing saved setting ${id}`);
  return text.replace(pattern, `$1${value}`);
}
export const median = values => {
  assert.ok(values.length, 'Missing measurements');
  const sorted = [...values].sort((a,b)=>a-b), i = Math.floor(sorted.length/2);
  return sorted.length%2 ? sorted[i] : (sorted[i-1]+sorted[i])/2;
};
const timestamp = line => {
  const m=line.match(/^\[(\d+):(\d+):(\d+)\.(\d+)/);
  return m ? ((+m[1]*60 + +m[2])*60 + +m[3])*1000 + +m[4] : NaN;
};
export function nativeChecks(text) {
  assert.doesNotMatch(text,/\bERROR\b|VUID-|SYNC-HAZARD|panicked at|non_finite=[1-9]/);
  assert.match(text,/Application exited successfully/);
}
export function perfMetrics(text, minimumSamples=50, minimumLights=8) {
  nativeChecks(text);
  assert.doesNotMatch(text,/\[LIGHT_RIS\]\[RECEIVERS\]/,'Never time audit programs');
  const dimensions=text.match(/screen=(\d+x\d+) scene=(\d+x\d+)/);
  assert.ok(dimensions,'Missing actual render dimensions');
  const frames=[], gpu=[];
  for(const line of text.split('\n')) {
    const frame=line.match(/\[PERF\](?:\[FRAME\])? frame (\d+) total /);
    if(frame) frames.push({n:+frame[1],t:timestamp(line)});
    const sample=line.match(/\[GPU_FRAME_SCOPE\] frame (\d+).*?graphics\.flora_lighting_cache=(\d+)us/);
    if(sample) gpu.push({n:+sample[1],t:timestamp(line),ms:+sample[2]/1000});
  }
  const end=frames.at(-1)?.t, tail=frames.filter(f=>f.t>=end-8000);
  const first=tail[0], last=tail.at(-1), began=frames[0];
  const values=gpu.filter(g=>g.t>=end-8000 && g.t<=end).map(g=>g.ms);
  assert.ok(values.length>=minimumSamples,`Only ${values.length} GPU samples`);
  assert.ok(last.n>first.n && last.t>first.t,'No real frame advancement');
  const counts=[...text.matchAll(/\[LOCAL_LIGHT\]\[LIVE\].*? count=(\d+) capacity=(\d+)/g)];
  const uploadedLights=+(counts.at(-1)?.[1]??0);
  assert.ok(uploadedLights>=minimumLights,'Actual selected-light upload was not observed');
  return {screen:dimensions[1],scene:dimensions[2],uploadedLights,grassGpuMs:median(values),gpuSamples:values.length,
    actualFps:1000*(last.n-first.n)/(last.t-first.t),
    wholeProfiledFps:1000*(last.n-began.n)/(last.t-began.t)};
}
export function auditMetrics(text) {
  nativeChecks(text);
  const rows=[];
  for(const line of text.split('\n')) {
    const m=line.match(/\[LIGHT_RIS\]\[RECEIVERS\] samples=(\d+) non_finite=(\d+) rendered_mismatches=(\d+) reference_nonzero=(\d+) ris_nonzero=(\d+) reference_rgb=(\[[^\]]+\]) ris_rgb=(\[[^\]]+\])/);
    if(m) rows.push({t:timestamp(line),samples:+m[1],invalid:+m[2],mismatch:+m[3],referenceNonzero:+m[4],risNonzero:+m[5],reference:JSON.parse(m[6]),ris:JSON.parse(m[7])});
  }
  assert.ok(rows.length>=3,'Missing receiver audit frame-slot readbacks');
  assert.ok(rows.every(r=>r.invalid===0 && r.mismatch===0),'Invalid or mismatching rendered irradiance');
  assert.ok(rows.some(r=>r.referenceNonzero>0) && rows.some(r=>r.risNonzero>0),'No nonzero direct lighting (not a useful audit)');
  const tail=rows.filter(r=>r.t>=rows.at(-1).t-4000);
  const sums=key=>[0,1,2].map(c=>tail.reduce((sum,r)=>sum+r[key][c],0));
  const reference=sums('reference'), ris=sums('ris');
  return {frames:tail.length,samples:tail.reduce((s,r)=>s+r.samples,0),referenceRgb:reference,risRgb:ris,
    relativeSignedRgbError:ris.map((v,i)=>reference[i]>0 ? v/reference[i]-1 : null),
    nonFinite:0,renderedMismatches:0};
}
export function scalingMetrics(text) {
  nativeChecks(text);
  assert.match(text,/\[LOCAL_LIGHT_SCALE\] complete /,'Scaling timed out before final removal/publication');
  const rows=[...text.matchAll(/\[LOCAL_LIGHT_SCALE\] count=(\d+) samples=(\d+).*?accepted=(\d+) overflow=0.*?raster_flora_cache_gpu_p50_us=([\d.]+)/g)]
    .map(m=>({count:+m[1],samples:+m[2],accepted:+m[3],grassGpuMs:+m[4]/1000}));
  assert.deepEqual(rows.map(r=>r.count),[1,2,4,8,16,32,64,0]);
  assert.ok(rows.every(r=>r.accepted===r.count),'Requested lights were not actually uploaded');
  return rows;
}

export function nativeArguments(kind, seconds, extra=[], seed=20260519) {
  return ['--hidden','--windowed','--mute',
    ...(kind!=='scaling'?['--garden-seed',String(seed)]:[]),
    '--environment-probe-spacing-voxels','32',
    ...(['perf','audit'].includes(kind)?['--camera-snapshot','garden-arrival']:[]),
    ...extra,'--auto-exit',String(seconds)];
}

function main() {
  let options;
  try {
    options=parseArgs({options:{case:{type:'string',default:'perf'},output:{type:'string'},
      repeats:{type:'string',default:'3'},seconds:{type:'string'},'light-budget':{type:'string',default:'8'},help:{type:'boolean',short:'h'}}}).values;
    if(options.help) {console.log(help);return;}
    assert.ok(['perf','audit','scaling','capture'].includes(options.case),'Unknown --case');
    options.seconds=+(options.seconds ?? ({perf:20,audit:10,scaling:180,capture:10}[options.case]));
    assert.ok(Number.isInteger(+options.repeats) && +options.repeats>=1,'--repeats must be an integer >=1');
    assert.ok(Number.isFinite(options.seconds) && options.seconds>=(options.case==='perf'?12:8),'Invalid --seconds');
    assert.ok(options.case==='perf' || !process.argv.includes('--repeats'),'--repeats only applies to perf');
    assert.ok(Number.isInteger(+options['light-budget']) && +options['light-budget']>=1 && +options['light-budget']<=64,'--light-budget must be an integer in 1..64');
    assert.ok(options.case!=='scaling' || !process.argv.includes('--light-budget'),'scaling forces --light-budget 64');
  } catch(error) {console.error(`${error.message}\n${help}`);process.exitCode=2;return;}
  const output=path.resolve(options.output ?? `target/light-ris/${options.case}-${new Date().toISOString().replaceAll(':','-')}`);
  const config='config/gui.toml', binary=path.resolve('target/release/re-flora');
  const hash=file=>createHash('sha256').update(readFileSync(file)).digest('hex');
  let original;
  try {
    assert.ok(existsSync(binary),'Build target/release/re-flora with cargo build --release first');
    assert.ok(!existsSync(output),`Output exists: ${output}; choose a NEW_DIR`);
    mkdirSync(output,{recursive:true});
    original=readFileSync(config);
    const binaryHash=hash(binary),cameraHash=hash('config/camera_snapshots.toml'), rows=[];
    let base=original.toString();
    for(const [id,value] of Object.entries({time_of_day:0.95,local_lighting_enabled:true,grass_local_light_field:false,
      local_light_budget:options.case==='scaling'?64:+options['light-budget'],grass_light_ris_rays:4,grass_light_ris_candidates:8}))
      base=setSavedValue(base,id,String(value));
    const run=(name,on,extra=[],seed=20260519)=>{
      const saved=setSavedValue(base,'grass_light_ris',String(on));
      writeFileSync(config,saved);
      const env={...process.env,RUST_LOG:'info'};
      for(const key of Object.keys(env)) if(key.startsWith('RE_FLORA_')) delete env[key];
      if(options.case==='perf')env.RE_FLORA_GARDEN_PERF='1';
      if(options.case==='audit')env.RE_FLORA_LIGHT_RIS_AUDIT='1';
      const result=spawnSync(binary,nativeArguments(options.case,options.seconds,extra,seed),
        {env,encoding:'utf8',maxBuffer:256*1024*1024});
      const text=(result.stdout??'')+(result.stderr??'');
      writeFileSync(path.join(output,`${name}.log`),text);
      assert.equal(result.status,0,`${name} native run failed: ${result.error??result.status}`);
      assert.equal(readFileSync(config,'utf8'),saved,'Runtime changed saved inputs');
      assert.equal(hash(binary),binaryHash,'Binary changed during validation');
      assert.equal(hash('config/camera_snapshots.toml'),cameraHash,'Camera changed during validation');
      nativeChecks(text);
      if(on)assert.match(text,/\[LIGHT_RIS\] enabled=true/);
      return text;
    };
    if(options.case==='perf') {
      for(let repeat=0;repeat<+options.repeats;repeat++) for(const mode of repeat%2?['on','off']:['off','on']) {
        const row={repeat,mode,...perfMetrics(run(`${repeat}-${mode}`,mode==='on',['--perf']),50,Math.min(8,+options['light-budget']))};
        rows.push(row);console.log(JSON.stringify(row));
        writeFileSync(path.join(output,'runs.json'),JSON.stringify({binaryHash,cameraHash,rows},null,2));
      }
      assert.ok(rows.every(r=>r.screen===rows[0].screen && r.scene===rows[0].scene && r.uploadedLights===rows[0].uploadedLights),'A/B dimensions or uploaded light counts differ');
      const summary=Object.fromEntries(['off','on'].map(mode=>[mode,Object.fromEntries(
        ['grassGpuMs','actualFps','wholeProfiledFps'].map(key=>[key,median(rows.filter(r=>r.mode===mode).map(r=>r[key]))]))]));
      const performancePassed=summary.on.grassGpuMs<=summary.off.grassGpuMs && summary.on.actualFps>=summary.off.actualFps && summary.on.wholeProfiledFps>=summary.off.wholeProfiledFps;
      const report={binaryHash,cameraHash,rows,summary,performancePassed};
      writeFileSync(path.join(output,'summary.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));
    } else {
      if(options.case==='audit') for(const seed of [20260519,7]) rows.push({seed,...auditMetrics(run(`audit-${seed}`,true,[],seed))});
      if(options.case==='scaling') for(const mode of ['off','on']) rows.push({mode,counts:scalingMetrics(run(mode,mode==='on',['--perf','--environment-lighting-test-scene','local-light-scaling']))});
      if(options.case==='capture') for(const mode of ['off','on']) {
        const file=path.join(output,`${mode}.png`);
        run(mode,mode==='on',['--screenshot','garden-arrival',file,'--screenshot-delay','6']);
        rows.push({mode,file:path.basename(file),sha256:hash(file)});
      }
      const report={binaryHash,cameraHash,case:options.case,rows};
      writeFileSync(path.join(output,'summary.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));
    }
  } catch(error) {console.error(`${error.stack}\nInspect retained logs in ${output}; retry with a NEW --output directory.\n${help}`);process.exitCode=1;}
  finally {if(original)writeFileSync(config,original);}
}
if(process.argv[1] && import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href)main();
