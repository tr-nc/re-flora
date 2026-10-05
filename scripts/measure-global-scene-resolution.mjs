#!/usr/bin/env node
// Diagnostic Release binaries are prepared separately; never edits source/settings.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
const root=path.resolve(import.meta.dirname,'..');
const base=path.join(root,'target/global-low-resolution/performance');
const immediate=process.argv.includes('--immediate');
const out=immediate?path.join(base,'immediate'):base;
if(process.argv.includes('--help')) {
  console.log(`Usage: node scripts/measure-global-scene-resolution.mjs [--immediate]
--immediate explicitly overrides FIFO for an uncapped diagnostic supplement,
saved under performance/immediate; default uses normal auto present selection.
Serial fixed-clock Release comparison: three cameras, two reverse-order repeats,
300 sample frames each. Requires native-p1/p4/p16 binaries in
 target/global-low-resolution/performance/binaries (scene SCALE .5/.25/.125),
and target/global-low-resolution/baseline-release (pre-experiment .5 renderer).
Only SCALE may differ between native binaries; restore .125 and rebuild Release
before handoff. No visible window, saved settings edits, or performance acceptance.
Artifacts: target/global-low-resolution/performance/{*.log,runs.json,summary.json}.
Exit 0: complete/help; 1: invalid artifacts, runtime or measurement failure.`);
  process.exit(0);
}
assert.ok(process.argv.slice(2).every(a=>a==='--immediate'),'Unknown argument; use --help');
fs.mkdirSync(out,{recursive:true});
const hash=b=>createHash('sha256').update(b).digest('hex');
const saved=['config/gui.toml','config/camera_snapshots.toml'];
const savedHashes=()=>saved.map(p=>hash(fs.readFileSync(path.join(root,p))));
const before=savedHashes();
const old=path.join(root,'target/global-low-resolution/baseline-release');
const variants=[
  {name:'old-continuous',binary:old,scale:.5,pixels:false},
  {name:'old-model-grid',binary:old,scale:.5,pixels:true},
  ...[[1,.5],[4,.25],[16,.125]].map(([den,scale])=>({name:`native-p${den}`,binary:path.join(base,`binaries/native-p${den}`),scale,pixels:false})),
];
for(const v of variants)v.binarySha256=hash(fs.readFileSync(v.binary));
const scenes=[{camera:'near-both',grid:3},{camera:'wide-both',grid:15},{camera:'low-both',grid:15}];
const percentile=(xs,p)=>{assert.ok(xs.length);const s=xs.toSorted((a,b)=>a-b);return s[Math.min(s.length-1,Math.floor(s.length*p))];};
const stats=xs=>({samples:xs.length,p50_us:percentile(xs,.5),p95_us:percentile(xs,.95)});
const runs=[];
for(let repeat=0;repeat<2;repeat++)for(const scene of (repeat?scenes.toReversed():scenes))for(const variant of (repeat?variants.toReversed():variants)) {
  const key=`${scene.camera}-g${scene.grid}-${variant.name}-r${repeat+1}`;
  console.log(`running ${key}`);
  const env={...process.env,RUST_LOG:'info'};
  for(const k of Object.keys(env))if(k.startsWith('RE_FLORA_')||k==='WAYLAND_DISPLAY'||k.startsWith('VK_LAYER')||k==='VK_INSTANCE_LAYERS')delete env[k];
  Object.assign(env,{RE_FLORA_GRASS_STEM_REVIEW:`${scene.camera}-b`,RE_FLORA_GRASS_STEM_GRID:String(scene.grid),RE_FLORA_GRASS_BAND_POSE_REUSE:'1',RE_FLORA_GRASS_BAND_PIXELIZATION:variant.pixels?'1':'0',RE_FLORA_STEM_PIXEL_RESOLUTION:'45',RE_FLORA_STEM_SAMPLE_FRAMES:'300'});
  const res=spawnSync(variant.binary,['--hidden','--mute','--windowed','--perf','--authored-flora-bench',...(immediate?['--present-mode','immediate']:[])],{cwd:root,env,encoding:'utf8',maxBuffer:128*1024*1024});
  const log=(res.stdout??'')+(res.stderr??'');fs.writeFileSync(path.join(out,`${key}.log`),log);
  assert.equal(res.status,0,`${key}: runtime failed`);
  assert.ok(!/\bERROR\b|VUID|hazard detected|panicked at|Validation (Error|Warning)/.test(log),`${key}: errors`);
  assert.match(log,/\[SHUTDOWN\] phase=complete failures=0/);
  assert.match(log,/Hidden window render extent is 2560x1440 physical pixels/);
  assert.match(log,new RegExp(`Chosen swapchain present mode: ${immediate?'IMMEDIATE':'FIFO'}`));
  const w=2560*variant.scale,h=1440*variant.scale;
  assert.match(log,new RegExp(`\\[GOD_RAY\\]\\[RESOURCES\\] scene=${w}x${h}`));
  if(variant.name.startsWith('native'))assert.match(log,new RegExp(`SCENE_PIXELS\\].*screen=2560x1440 scene=${w}x${h} scale=${variant.scale}`));
  const start=log.match(/GRASS_STEM_REVIEW\].*phase=sample app_frame=(\d+).*grass=\[([^\]]+)\] camera=(\[[^\]]+\]) target=(\[[^\]]+\])/);
  const end=log.match(/GRASS_STEM_REVIEW\].*phase=complete app_frame=(\d+)/);
  assert.ok(start&&end,`${key}: sample boundaries`);
  const metrics={},gpuFrames=[],cpuFrames=[];
  const push=(name,n)=>(metrics[name]??=[]).push(n);
  for(const line of log.split('\n')){
    const gpu=line.match(/GPU_FRAME_SCOPE\] frame (\d+).*dropped=(\d+) (.*)/);
    if(gpu&&+gpu[1]>=+start[1]+4&&+gpu[1]<+end[1]) {
      assert.equal(+gpu[2],0,`${key}: dropped timestamp scopes`);gpuFrames.push(+gpu[1]);
      const scope={};for(const m of gpu[3].matchAll(/(?:^| )([\w.]+)=(\d+)us/g))scope[m[1]]=(scope[m[1]]??0)+ +m[2];
      for(const [name,n] of Object.entries(scope))push(name,n);
      assert.ok('frame.render' in scope&&'graphics.flora' in scope);
      push('grass.preparation_plus_draw',(scope['graphics.flora_lighting_cache']??0)+scope['graphics.flora']);
      // Sum peers only, never nested parent intervals.
      push('models.preparation',Object.entries(scope).filter(([s])=>/^models\..*\.(?:prepare|tiles)$/.test(s)).reduce((n,[,t])=>n+t,0));
    }
    const cpu=line.match(/CPU_FRAME_SCOPE\] frame (\d+) (.*)/);
    if(cpu&&+cpu[1]>=+start[1]+4&&+cpu[1]<+end[1]) {
      cpuFrames.push(+cpu[1]);for(const m of cpu[2].matchAll(/(?:^| )([\w.]+)=(\d+)us/g))push(`cpu.${m[1]}`,+m[2]);
    }
  }
  assert.ok(gpuFrames.length>=290&&cpuFrames.length>=290,`${key}: insufficient samples`);
  const telemetry=spawnSync('nvidia-smi',['--query-gpu=temperature.gpu,clocks.gr,clocks.mem,pstate','--format=csv,noheader'],{encoding:'utf8'}).stdout?.trim();
  const row={key,repeat:repeat+1,...scene,variant:variant.name,scale:variant.scale,sceneExtent:[w,h],population:start[2],cameraPose:start[3],cameraTarget:start[4],gpuFrames,cpuFrames,metrics,telemetry};runs.push(row);
  assert.deepEqual(savedHashes(),before,'User settings changed');
  fs.writeFileSync(path.join(out,'runs.json'),JSON.stringify(runs,null,2));
  console.log(`  GPU frame p50=${stats(metrics['frame.render']).p50_us}us CPU total p50=${stats(metrics['cpu.frame.cpu_total']).p50_us}us`);
}
const summary=scenes.map(scene=>{
  const rows=runs.filter(r=>r.camera===scene.camera);
  for(const field of ['population','cameraPose','cameraTarget'])assert.equal(new Set(rows.map(r=>r[field])).size,1,`${scene.camera}: changed ${field}`);
  return {...scene,population:rows[0].population,variants:Object.fromEntries(variants.map(v=>{
    const selected=rows.filter(r=>r.variant===v.name);
    const names=[...new Set(selected.flatMap(r=>Object.keys(r.metrics)))];
    return [v.name,Object.fromEntries(names.map(s=>[s,stats(selected.flatMap(r=>r.metrics[s]??[]))]))];
  }))};
});
const revision=spawnSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).stdout.trim();
fs.writeFileSync(path.join(out,'summary.json'),JSON.stringify({revision,presentMode:immediate?'IMMEDIATE':'AUTO_FIFO',nativeBuildDifference:'scene_resolution::SCALE only; temporary diagnostic builds',variants,window:[2560,1440],repeats:2,sampleFrames:300,savedConfigSha256:before,summary},null,2));
console.log('passed 30 serial Release runs');
