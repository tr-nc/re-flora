#!/usr/bin/env node
// Sequential Release-only world-size profiling. No app/settings edits.
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
import {parseArgs} from 'node:util';
const {values:o}=parseArgs({options:{before:{type:'string'},after:{type:'string'},out:{type:'string'},order:{type:'string',default:'before,after,after,before'},seconds:{type:'string',default:'28'},warmup:{type:'string',default:'8'},flags:{type:'string',default:''},'log-filter':{type:'string',default:'info'},sparse:{type:'boolean'},detailed:{type:'boolean'},'atlas-log':{type:'boolean'},'allow-dropped':{type:'boolean'},guard:{type:'boolean'}}});
assert(o.before&&o.after&&o.out,'--before PATH --after PATH --out DIR [--flags "--no-flora"] [--guard]');
const root=process.cwd(), out=path.resolve(o.out), runtime=path.join(out,'runtime');
fs.mkdirSync(out,{recursive:true});
const hash=b=>createHash('sha256').update(b).digest('hex');
const config=fs.readFileSync('config/gui.toml'), configHash=hash(config);
const binaries={before:path.resolve(o.before),after:path.resolve(o.after)};
const manifest={options:o,configHash,binaries,binaryHashes:Object.fromEntries(Object.entries(binaries).map(([k,v])=>[k,hash(fs.readFileSync(v))])),runs:[]};
const q=(a,p)=>a.toSorted((a,b)=>a-b)[Math.floor((a.length-1)*p)];
const stats=a=>({n:a.length,mean:a.reduce((s,v)=>s+v,0)/a.length,median:q(a,.5),p95:q(a,.95),min:Math.min(...a),max:Math.max(...a)});
const clock=s=>{const m=s.match(/^\[(\d\d):(\d\d):(\d\d)\.(\d{3})/);return m?(+m[1]*3600 + +m[2]*60 + +m[3]+ +m[4]/1000):null;};
function summarize(samples){const keys=[...new Set(samples.flatMap(s=>Object.keys(s.values)))];return Object.fromEntries(keys.map(k=>[k,stats(samples.map(s=>s.values[k]??0))]));}
try {
 for(const [index,variant] of o.order.split(',').entries()) {
  assert(binaries[variant],`unknown variant ${variant}`);
  fs.rmSync(runtime,{recursive:true,force:true});fs.mkdirSync(runtime,{recursive:true});
  fs.cpSync('config',path.join(runtime,'config'),{recursive:true});
  fs.writeFileSync(path.join(runtime,'config/gui.toml'),config);
  fs.symlinkSync(path.join(root,'assets'),path.join(runtime,'assets'),'dir');
  const env={...process.env,RUST_LOG:o['log-filter']};
  for(const key of Object.keys(env))if(key.startsWith('RE_FLORA_')||(key.startsWith('VK_LAYER')&&key!=='VK_LAYER_PATH'))delete env[key];
  delete env.VK_INSTANCE_LAYERS;
  Object.assign(env,{RE_FLORA_ROOT:runtime});
  // Without RE_FLORA_SPRINKLER_REVIEW, GARDEN_PERF only enables existing
  // per-frame log cadence; it does not construct a sprinkler fixture.
  if(!o.sparse)env.RE_FLORA_GARDEN_PERF='1';
  if(o.detailed)env.RE_FLORA_STEM_PIXEL_PROFILE='1';
  if(o['atlas-log'])env.RE_FLORA_PAVEMENT_PERF_REPORT='1';
  const args=['--hidden','--mute','--perf','--camera-snapshot','player-default','--auto-exit',o.seconds,...o.flags.split(' ').filter(Boolean)];
  const name=`${index}-${variant}`;
  console.log(`START ${name} ${args.join(' ')}`);
  const run=spawnSync(binaries[variant],args,{cwd:root,env,encoding:'utf8',maxBuffer:256*1024*1024});
  const log=(run.stdout??'')+(run.stderr??'');fs.writeFileSync(path.join(out,`${name}.log`),log);
  assert.equal(run.status,0,`${name}: ${run.error??'app failed'}`);
  assert.doesNotMatch(log,/\bERROR\b|VUID-|SYNC-HAZARD|panicked at/);
  assert.match(log,/phase=complete failures=0/);
  const readyLine=log.split('\n').find(s=>s.includes('[STARTUP] status=ready stage=first_frame'));
  assert(readyLine,'missing ready marker');
  const ready=clock(readyLine), start=ready+ +o.warmup;
  const cpu=[],gpu=[],seen=new Set(),resolutions=[];
  let maxDropped=0;
  for(const line of log.split('\n')) {
   const time=clock(line);if(time===null||time<start)continue;
   if(line.includes('[SCENE_PIXELS]'))resolutions.push(line);
   const m=line.match(/\[PERF\]\[(CPU|GPU)_FRAME_SCOPE\] frame (\d+)(.*)/);if(!m)continue;
   const [,kind,id,tail]=m;
   if(kind==='GPU') {const dropped=+tail.match(/dropped=(\d+)/)[1];maxDropped=Math.max(maxDropped,dropped);assert(dropped===0||o['allow-dropped'],'profiler query overflow');if(seen.has(id))continue;seen.add(id);}
   const values={};for(const kv of tail.matchAll(/([\w.]+)=([\d.]+)us/g))values[kv[1]]=(values[kv[1]]??0)+ +kv[2];
   (kind==='CPU'?cpu:gpu).push({frame:+id,time:time-ready,values});
  }
  assert(cpu.length>=20&&gpu.length>=20,`insufficient settled frames: ${cpu.length}/${gpu.length}`);
  const screen=[...log.matchAll(/\[SCENE_PIXELS\] ([^\n]+)/g)].at(-1)?.[1];assert(screen);
  if(manifest.runs.length)assert.equal(screen,manifest.runs[0].screen,'render resolution/quality changed');
  assert.equal(resolutions.length,0,'viewport changed during measurement');
  const row={name,variant,args,screen,readyLine,resolutions,maxDropped,frameRate:(cpu.at(-1).frame-cpu[0].frame)/(cpu.at(-1).time-cpu[0].time),cpu:summarize(cpu),gpu:summarize(gpu)};
  fs.writeFileSync(path.join(out,`${name}.samples.json`),JSON.stringify({cpu,gpu}));
  manifest.runs.push(row);fs.writeFileSync(path.join(out,'summary.json'),JSON.stringify(manifest,null,2));
  assert.equal(hash(fs.readFileSync('config/gui.toml')),configHash,'user config changed');
  console.log(JSON.stringify({name,frameRate:row.frameRate,cpu:row.cpu['frame.cpu_total'],gpu:row.gpu['frame.render'],screen}));
 }
 const a=manifest.runs.filter(r=>r.variant==='before'),b=manifest.runs.filter(r=>r.variant==='after');
 if(a.length&&b.length){
  const ratio=q(b.map(r=>r.gpu['frame.render'].median),.5)/q(a.map(r=>r.gpu['frame.render'].median),.5);
  const fpsRatio=q(b.map(r=>r.frameRate),.5)/q(a.map(r=>r.frameRate),.5);
  console.log(JSON.stringify({gpuMedianRatio:ratio,frameRateRatio:fpsRatio,regression:ratio>1.02||fpsRatio<1/1.02}));
  if(o.guard&&(ratio>1.02||fpsRatio<1/1.02))process.exitCode=2;
 }
} finally {fs.rmSync(runtime,{recursive:true,force:true});}
