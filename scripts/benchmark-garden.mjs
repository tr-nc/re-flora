#!/usr/bin/env node
// Run under the shared heavy-resource lock, after building both matched variants.
import {spawnSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {parseArgs} from 'node:util';
import assert from 'node:assert/strict';
import path from 'node:path';
const help=`Usage: node scripts/benchmark-garden.mjs --before PATH --after PATH [--output DIR] [--seconds N] [--repeats N] [--matrix primary|stress|all]
Matched CURRENT-feature diagnostic vs optimized Release binaries, not the historical executable.
Loads only target/garden-delivery/world/implementation/garden-final.rflterrain; native fullscreen hidden/muted, saved quality, garden-arrival camera.
Defaults: output target/garden-delivery/final_acceptance/measurement, 14 seconds, 3 repeats, all.
Primary: spread 30% duty, populations 0/1/4/8/16. Stress: spread 100%, overlap 30% and 100%, populations 4/16.
Interleaves variants (before/after, after/before, before/after). Excludes warmup through per-instance GPU readback + 1 second and at least 5 seconds from first profiler sample.
Requires --features no_validation_layer builds with identical grounded harness. No build or other heavy job may overlap: hold flock -o /tmp/re-flora-garden-delivery-heavy.lock around this entire script.
Raw logs, per-run scope p50/p90, per-frame summed sprinkler scopes, samples and input hashes are retained. Does not write settings, camera or snapshots.
Exit: 0 evidence captured (not player/performance approval), 1 run/evidence failure, 2 usage error.`;
let o;
try{o=parseArgs({options:{before:{type:'string'},after:{type:'string'},output:{type:'string',default:'target/garden-delivery/final_acceptance/measurement'},seconds:{type:'string',default:'14'},repeats:{type:'string',default:'3'},matrix:{type:'string',default:'all'},help:{type:'boolean',short:'h'}}}).values;
if(o.help){console.log(help);process.exit(0);}
if(!o.before||!o.after||+o.seconds<10||!Number.isFinite(+o.seconds)||!Number.isInteger(+o.repeats)||+o.repeats<3||!['all','primary','stress'].includes(o.matrix))throw Error('Require both binaries, seconds >=10, repeats >=3 and a valid matrix');
}catch(e){console.error(`${e.message}\n${help}`);process.exit(2);}
const hash=f=>createHash('sha256').update(readFileSync(f)).digest('hex');
const files=['config/gui.toml','config/camera_snapshots.toml','target/garden-delivery/world/implementation/garden-final.rflterrain'];
const hashes=()=>Object.fromEntries(files.map(f=>[f,hash(f)]));
const q=(a,p)=>a.toSorted((a,b)=>a-b)[Math.floor((a.length-1)*p)];
const stamp=m=>+m[1]*3600+ +m[2]*60+ +m[3];
function metrics(text,kind){
 const pub=text.match(/\[(\d+):(\d+):(\d+\.\d+) [^\n]*\[SPRINKLER_PERF_PUBLICATION\]/);assert.ok(pub,'Missing publication marker');
 const rows=[...text.matchAll(new RegExp(`\\[(\\d+):(\\d+):(\\d+\\.\\d+) [^\\n]*?\\[PERF\\]\\[${kind}_FRAME_SCOPE\\] frame (\\d+) ([^\\n]+)`,'g'))].map(m=>{
 const scopes={};for(const v of m[5].matchAll(/([\w.]+)=(\d+)us/g))scopes[v[1]]=(scopes[v[1]]??0)+ +v[2]/1000;
 if(kind==='GPU')scopes['sprinkler.sum']=['sprinkler_spray.prepare','sprinkler_spray.wetting','sprinkler_mist.tiles','sprinkler_mist.pass','graphics.sprinklers'].reduce((a,k)=>a+(scopes[k]??0),0);
 return {time:stamp(m),frame:+m[4],scopes,dropped:+(m[5].match(/dropped=(\d+)/)?.[1]??0)};});
 const start=Math.max(rows[0].time+5,stamp(pub)+1);
 const warm=rows.filter(r=>r.time>=start);assert.ok(warm.length>=100,`Only ${warm.length} warmed ${kind} samples`);assert.ok(warm.every(r=>!r.dropped));
 const keys=[...new Set(warm.flatMap(r=>Object.keys(r.scopes)))];
 const scopes=Object.fromEntries(keys.map(k=>[k,{p50:q(warm.map(r=>r.scopes[k]??0),.5),p90:q(warm.map(r=>r.scopes[k]??0),.9)}]));
 const interval=warm.slice(1).map((r,i)=>1000*(r.time-warm[i].time)/(r.frame-warm[i].frame));
 return {samples:warm.length,first_frame:warm[0].frame,last_frame:warm.at(-1).frame,scopes,interval_ms:{p50:q(interval,.5),p90:q(interval,.9)}};
}
try{
 mkdirSync(o.output,{recursive:true});const inputs=hashes(),results=[];
 const cases=[];if(o.matrix!=='stress')for(const count of [0,1,4,8,16])cases.push({count,layout:'spread',duty:30});
 if(o.matrix!=='primary')for(const [layout,duty] of [['spread',100],['overlap',30],['overlap',100]])for(const count of [4,16])cases.push({count,layout,duty});
 const binaryHashes={before:hash(o.before),after:hash(o.after)};
 for(let repeat=0;repeat<+o.repeats;repeat++)for(const c of cases)for(const variant of repeat%2?['after','before']:['before','after']){
 const name=`${c.layout}-d${c.duty}-n${c.count}-r${repeat}-${variant}`;
 assert.deepEqual(hashes(),inputs,'Inputs changed');
 const env={...process.env,RUST_LOG:'info',RE_FLORA_GARDEN_PERF:'1',RE_FLORA_SPRINKLER_REVIEW:'rotary-perf',RE_FLORA_SPRINKLER_PERF:`${c.count},${c.layout},${c.duty}`};
 for(const key of Object.keys(env))if(key.startsWith('RE_FLORA_')&&!['RE_FLORA_GARDEN_PERF','RE_FLORA_SPRINKLER_REVIEW','RE_FLORA_SPRINKLER_PERF'].includes(key))delete env[key];
 delete env.VK_INSTANCE_LAYERS;delete env.VK_LAYER_ENABLES;delete env.VK_LOADER_LAYERS_DISABLE;
 if(env.DISPLAY){delete env.WAYLAND_DISPLAY;delete env.WAYLAND_SOCKET;}
 const run=spawnSync(path.resolve(o[variant]),['--hidden','--mute','--perf','--terrain-load',files[2],'--camera-snapshot','garden-arrival','--auto-exit',o.seconds],{env,encoding:'utf8',maxBuffer:128*1024*1024});
 const text=(run.stdout??'')+(run.stderr??'');writeFileSync(path.join(o.output,name+'.log'),text);
 assert.equal(run.status,0,`${name} exited ${run.status}: ${run.error??''}`);
 assert.doesNotMatch(text,/\bERROR\b|VUID-|SYNC-HAZARD|panicked at|Insert instance layer.*KHRONOS_validation/);
 assert.match(text,/screen=4096x2560 scene=820x512/);assert.match(text,/phase=complete failures=0/);
 assert.ok(text.includes(`[SPRINKLER_PERF_PUBLICATION] instances=${c.count} all_instances_live=true`));
 const row={name,...c,variant,repeat,cpu:metrics(text,'CPU'),gpu:metrics(text,'GPU')};results.push(row);
 assert.deepEqual(hashes(),inputs,'Inputs changed');
 writeFileSync(path.join(o.output,'summary.json'),JSON.stringify({options:o,hashes:inputs,binaryHashes,results},null,2)+'\n');
 console.log(JSON.stringify({name,samples:row.gpu.samples,frame:row.gpu.scopes['frame.render'],sprinkler:row.gpu.scopes['sprinkler.sum']}));
 }
}catch(e){console.error(`${e.stack}\nInspect ${o.output} logs. Correct the failure, then rerun into a NEW output directory; do not overwrite evidence.\n${help}`);process.exitCode=1;}
