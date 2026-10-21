#!/usr/bin/env node
// Opt-in native Release benchmark. Invoke under the garden heavy-resource flock.
import {spawnSync} from 'node:child_process';
import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
process.chdir(fileURLToPath(new URL('../', import.meta.url)));
const help = `Usage: node scripts/benchmark-sprinklers.mjs [--binary PATH] [--output DIR] [--seconds N] [--repeats N] [--layouts spread,overlap] [--duties 30,100] [--counts 0,1,4,8,16] [--particles on|off]
Hidden/muted real Vulkan app, no forced present mode, no validation layers.
Build first: cargo build --release --features no_validation_layer (default Release requests validation).
Defaults: target/release/re-flora, target/garden-delivery/performance/run, 12 seconds, 2 repeats, both layouts/duties, all populations.
Excludes first 3 seconds of profiler samples; saves raw logs and p50/p90 CPU/GPU scopes.
Settings are in-memory; GUI/camera files are checked unchanged. Never loads/saves player terrain.
Hold flock -o /tmp/re-flora-garden-delivery-heavy.lock around this command. No concurrent app.
Exit: 0 measurement complete (not performance acceptance), 1 run/evidence failure, 2 invalid arguments.
Example: flock -o /tmp/re-flora-garden-delivery-heavy.lock node scripts/benchmark-sprinklers.mjs --duties 30 --layouts spread`;
const o={binary:'target/release/re-flora',output:'target/garden-delivery/performance/run',seconds:12,repeats:2,layouts:'spread,overlap',duties:'30,100',counts:'0,1,4,8,16',particles:'on'};
const args=process.argv.slice(2);
if(args.length===1&&['--help','-h'].includes(args[0])){console.log(help);process.exit(0);}
for(let i=0;i<args.length;i+=2){const k=args[i].slice(2);if(!args[i].startsWith('--')||!(k in o)||!args[i+1]){console.error(`Invalid option/value: ${args[i]}\n${help}`);process.exit(2);}o[k]=args[i+1];}
if(!['on','off'].includes(o.particles)||!Number.isFinite(+o.seconds)||+o.seconds<8||!Number.isInteger(+o.repeats)||+o.repeats<1||!o.layouts.split(',').every(v=>['spread','overlap'].includes(v))||!o.duties.split(',').every(v=>['30','100'].includes(v))||!o.counts.split(',').every(v=>['0','1','4','8','16'].includes(v))){console.error(`Invalid range or choice\n${help}`);process.exit(2);}
const q=(a,p)=>{assert.ok(a.length,'Missing timing samples');return a.toSorted((a,b)=>a-b)[Math.floor((a.length-1)*p)];};
function metrics(text,kind){
 const rows=[...text.matchAll(new RegExp(`\\[(\\d+):(\\d+):(\\d+\\.\\d+) [^\\n]*?\\[PERF\\]\\[${kind}_FRAME_SCOPE\\] frame (\\d+) ([^\\n]+)`,'g'))].map(m=>({time:+m[1]*3600+ +m[2]*60+ +m[3],frame:+m[4],dropped:Number(m[5].match(/\bdropped=(\d+)/)?.[1]??0),scopes:Object.fromEntries([...m[5].matchAll(/([\w.]+)=(\d+)us/g)].map(v=>[v[1],+v[2]/1000]))}));
 assert.ok(rows.length>=5,`Too few ${kind} samples; retry --seconds 20`);
 const warm=rows.filter(r=>r.time>=rows[0].time+3);assert.ok(warm.length>=3,`Insufficient warmed ${kind} samples`);
 assert.ok(warm.every(r=>r.dropped===0),`Dropped ${kind} profiler scopes; evidence is incomplete`);
 const keys=[...new Set(warm.flatMap(r=>Object.keys(r.scopes)))];
 const scopes=Object.fromEntries(keys.map(k=>[k,{p50:q(warm.map(r=>r.scopes[k]??0),.5),p90:q(warm.map(r=>r.scopes[k]??0),.9)}]));
 const interval=warm.slice(1).map((r,i)=>1000*(r.time-warm[i].time)/(r.frame-warm[i].frame));
 return {samples:warm.length,scopes,interval_ms:{p50:q(interval,.5),p90:q(interval,.9)}};
}
try{
 mkdirSync(o.output,{recursive:true});
 const files=['config/gui.toml',...['config/camera.toml','config/camera_snapshots.toml'].filter(f=>{try{readFileSync(f);return true;}catch{return false;}})];
 const hashes=()=>Object.fromEntries(files.map(f=>[f,createHash('sha256').update(readFileSync(f)).digest('hex')]));
 const before=hashes(),results=[];
 for(let repeat=0;repeat<+o.repeats;repeat++)for(const layout of o.layouts.split(','))for(const duty of o.duties.split(','))for(const count of o.counts.split(',')){
  assert.deepEqual(hashes(),before,'Preferences changed');
  const name=`${layout}-d${duty}-n${count}-r${repeat}`;
  const env={...process.env,RUST_LOG:'info',RE_FLORA_SPRINKLER_REVIEW:'rotary-perf',RE_FLORA_SPRINKLER_PERF:`${count},${layout},${duty}`,VK_LOADER_LAYERS_DISABLE:'~all~'};
  for(const key of Object.keys(env))if(key.startsWith('RE_FLORA_')&&!['RE_FLORA_SPRINKLER_REVIEW','RE_FLORA_SPRINKLER_PERF'].includes(key))delete env[key];
  delete env.VK_INSTANCE_LAYERS;delete env.VK_LAYER_ENABLES;
  // Same real native window/swapchain as normal hidden play, fixed X11 logical scale.
  if(env.DISPLAY){delete env.WAYLAND_DISPLAY;delete env.WAYLAND_SOCKET;env.WINIT_X11_SCALE_FACTOR='1';}
  const run=spawnSync(path.resolve(o.binary),['--flat-terrain','--no-flora','--hidden','--mute','--windowed',...(o.particles==='off'?['--no-particles']:[]),'--perf','--auto-exit',String(o.seconds)],{env,encoding:'utf8',maxBuffer:64*1024*1024});
  const text=(run.stdout??'')+(run.stderr??'');writeFileSync(path.join(o.output,`${name}.log`),text);
  assert.equal(run.status,0,`${name}: app failed (${run.error??''}); inspect log`);
  assert.doesNotMatch(text,/\bERROR\b|VUID-|panicked at|Insert instance layer.*KHRONOS_validation/);
  assert.ok(text.includes(`[SPRINKLER_PERF] count=${count} overlap=${layout==='overlap'}`),'Fixture missing');
  assert.match(text,/\[SHUTDOWN\] phase=complete failures=0/);
  assert.deepEqual(hashes(),before,'Preferences changed');
  const row={name,count:+count,layout,duty:+duty,repeat,cpu:metrics(text,'CPU'),gpu:metrics(text,'GPU')};
  results.push(row);writeFileSync(path.join(o.output,'summary.json'),JSON.stringify({options:o,hashes:before,results},null,2)+'\n');
  console.log(JSON.stringify({name,cpu:row.cpu.scopes['frame.cpu_total'],gpu:row.gpu.scopes['frame.render'],interval:row.gpu.interval_ms}));
 }
}catch(e){console.error(`${e.message}\nInspect ${o.output}/*.log; retry --seconds 20.\n${help}`);process.exitCode=1;}
