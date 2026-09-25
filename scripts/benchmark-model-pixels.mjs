#!/usr/bin/env node
// Real Release app measurement, never a Cargo unit test. Config isolation is not
// implemented by the app yet: own edits are restored, concurrent edits are not.
import {spawn} from 'node:child_process';
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
const help=`Usage: node scripts/benchmark-model-pixels.mjs [--seconds 8] [--binary target/release/re-flora] [--output target/model-pixel-bench] [--stress-leaves 0] [--suite apples|stage-one]
Measures pixel-rendered 8/32/64px apples, attached and fallen, in the same hidden
windowed scene (Linux/X11: fixed 2880x1620). Build first: cargo build --release.
Requires Vulkan/display. Acceptance: GPU p95 under the 60 Hz budget, and frame
interval within 5% of the 8px reference (stage-one: 128 views, per-pixel lighting) or 60 Hz, whichever is slower.
Do not run another game instance while measuring: temporarily edits config/gui.toml
and restores it on completion or handled signals. A recovery copy is saved in the
output directory. Logs, screenshots, dimensions and summary.json are retained.
Frame intervals come from consecutive GPU log timestamps/frame numbers, NOT the
slow-frame-biased PERF/FRAME log. Warmup and screenshot capture are excluded.
Optional --stress-leaves 1..16384 adds that many rotating renderer-only leaves
(at 16px) plus 21 butterfly previews (16px), without GPU readback or CPU oracles.
This is a rendering workload, not a flight-physics benchmark.
--suite stage-one compares 8/128/512 views with both lighting modes on the
same 32px apple meshes. Views remain live-rendered: NOT atlas performance.
Example: node scripts/benchmark-model-pixels.mjs --seconds 10 --stress-leaves 256 --suite stage-one`;
const options={seconds:8,binary:'target/release/re-flora',output:'target/model-pixel-bench','stress-leaves':0,suite:'apples'};
const args=process.argv.slice(2);
if(args.length===1&&['--help','-h'].includes(args[0])){console.log(help);process.exit(0);}
for(let i=0;i<args.length;i+=2){
 const key=args[i]?.replace(/^--/,'');
 if(!args[i]?.startsWith('--')||!Object.hasOwn(options,key)||!args[i+1]||args[i+1].startsWith('--')){
  console.error(`Invalid option/value near ${args[i]}.\n${help}`);process.exit(2);
 }
 options[key]=['seconds','stress-leaves'].includes(key)?Number(args[i+1]):args[i+1];
}
if(!Number.isFinite(options.seconds)||options.seconds<6){console.error(`--seconds must be at least 6.\n${help}`);process.exit(2);}
if(!Number.isInteger(options['stress-leaves'])||options['stress-leaves']<0||options['stress-leaves']>16384){console.error(`--stress-leaves must be 0..16384.\n${help}`);process.exit(2);}
if(!['apples','stage-one'].includes(options.suite)){console.error(`--suite must be apples or stage-one.\n${help}`);process.exit(2);}
process.chdir(fileURLToPath(new URL('../',import.meta.url)));
await mkdir(options.output,{recursive:true});
const configPath='config/gui.toml',original=await readFile(configPath,'utf8');
await writeFile(path.join(options.output,'gui.before.toml'),original);
let owned=original,child,interrupted=false;
for(const signal of ['SIGINT','SIGTERM'])process.once(signal,()=>{interrupted=true;child?.kill('SIGTERM');});
function setting(text,id,value){
 const pattern=new RegExp(`(id = "${id}"[\\s\\S]*?\\[section\\.param\\.data\\]\\s*value = )[^\\n]+`);
 assert.match(text,pattern,`Missing setting ${id}`);return text.replace(pattern,`$1${value}`);
}
function run(binary,args){return new Promise((resolve,reject)=>{
 let text='';
 const env={...process.env};
 for(const key of ['RE_FLORA_LEAF_MODEL_REVIEW','RE_FLORA_BUTTERFLY_MESH_REVIEW','RE_FLORA_APPLE_MODEL_REVIEW',
  'RE_FLORA_FALLEN_LEAF_REVIEW','RE_FLORA_BUTTERFLY_REVIEW','RE_FLORA_MODEL_PIXEL_STRESS_LEAVES'])delete env[key];
 if(options['stress-leaves'])env.RE_FLORA_MODEL_PIXEL_STRESS_LEAVES=String(options['stress-leaves']);
 // Hidden Wayland windows can land on different fractional-scale monitors.
 // Pin XWayland/X11 to 2.25 (1280x720 logical -> 2880x1620 physical).
 if(process.platform==='linux'&&env.DISPLAY){delete env.WAYLAND_DISPLAY;delete env.WAYLAND_SOCKET;env.WINIT_X11_SCALE_FACTOR='2.25';}
 child=spawn(path.resolve(binary),args,{stdio:['ignore','pipe','pipe'],env});
 child.stdout.on('data',b=>text+=b);child.stderr.on('data',b=>text+=b);
 child.once('error',reject);child.once('close',code=>{child=undefined;resolve({code,text});});
});}
function quantile(values,q){const sorted=values.toSorted((a,b)=>a-b);return sorted[Math.floor((sorted.length-1)*q)];}
function metrics(text){
 const rows=[...text.matchAll(/\[(\d+):(\d+):(\d+\.\d+) [^\n]*?\[PERF\]\[GPU_FRAME_SCOPE\] frame (\d+) ([^\n]+)/g)].map(m=>({
  time:(+m[1]*3600 + +m[2]*60 + +m[3])*1000,frame:+m[4],
  scopes:Object.fromEntries([...m[5].matchAll(/([\w.]+)=(\d+)us/g)].map(v=>[v[1],+v[2]/1000]))}));
 assert.ok(rows.length>=6,'Too few GPU timing samples; increase --seconds');
 const warm=rows.filter(r=>r.time>=rows[0].time+2500);
 assert.ok(warm.length>=4,'Insufficient post-warmup samples; increase --seconds');
 const intervals=warm.slice(1).map((r,i)=>(r.time-warm[i].time)/(r.frame-warm[i].frame));
 const gpu=warm.map(r=>r.scopes['frame.render']);
 const tiles=warm.map(r=>(r.scopes['models.apple_tree.tiles']||0)+(r.scopes['models.apple_dynamic.tiles']||0));
 return {samples:warm.length,gpu_ms_p50:quantile(gpu,.5),gpu_ms_p95:quantile(gpu,.95),
  tile_ms_p50:quantile(tiles,.5),particle_tile_ms_p50:quantile(warm.map(r=>r.scopes['butterfly.tiles']||0),.5),frame_interval_ms_p50:quantile(intervals,.5),
  frame_interval_ms_p95:quantile(intervals,.95),fps_from_median_interval:1000/quantile(intervals,.5)};
}
const results=[];
try {
 const cases=options.suite==='stage-one'?[8,128,512].flatMap(views=>[0,1].map(flags=>({
  n:32,flags,views,label:`views-${views}-${flags?'one-light':'per-pixel'}`
 }))):[8,32,64].map(n=>({n,flags:0,views:128,label:n}));
 for(const scene of ['attached','fallen'])for(const {n,flags,views,label} of cases){
  if(interrupted)throw new Error('Interrupted');
  assert.equal(await readFile(configPath,'utf8'),owned,'Concurrent config edit; refusing to overwrite it');
  owned=setting(setting(original,'apple_pixel_resolution',n),'fruit_cycle',scene==='attached'?0.7:1);
  owned=setting(setting(owned,'model_pixel_single_light',Boolean(flags&1)),'model_pixel_view_count',views);
  if(options['stress-leaves'])for(const [id,value] of Object.entries({butterfly_mesh_preview:true,butterfly_pixel_resolution:16,
   falling_leaf_mesh:true,falling_leaf_pixel_resolution:16,falling_leaf_size_scale:1}))owned=setting(owned,id,value);
  await writeFile(configPath,owned);
  const name=`${scene}-${label}`,image=path.join(options.output,`${name}.png`);
  const {code,text}=await run(options.binary,['--hidden','--mute','--windowed','--perf',
   '--screenshot','player-default',image,'--screenshot-delay','1','--auto-exit',String(options.seconds)]);
  await writeFile(path.join(options.output,`${name}.log`),text);
  assert.equal(code,0,`${name}: app failed; inspect log`);
  assert.doesNotMatch(text,/\bERROR\b|VUID-|panicked at/,`${name}: runtime validation failed`);
  assert.match(text,/Application exited successfully/);
  if(options['stress-leaves'])assert.ok(text.includes(`[MODEL_PIXEL_STRESS] leaves=${options['stress-leaves']} butterflies=21`),'Requested stress workload was not activated');
  const png=await readFile(image),width=png.readUInt32BE(16),height=png.readUInt32BE(20);
  assert.ok(text.includes(`[MODEL_PIXEL_PREVIEW] single_light=${Boolean(flags&1)} views=${views} live_tiles=true continuous_oracle=false`),'View count/lighting did not reach renderer');
  const row={scene,resolution:n,flags,views,width,height,stress_leaves:options['stress-leaves'],...metrics(text)};
  if(results.length)assert.deepEqual([width,height],[results[0].width,results[0].height],'Viewport changed between runs');
  results.push(row);console.log(JSON.stringify(row));
  await writeFile(path.join(options.output,'summary.json'),JSON.stringify({binary:path.resolve(options.binary),seconds:options.seconds,results},null,2)+'\n');
 }
} finally {
 if(await readFile(configPath,'utf8')===owned)await writeFile(configPath,original);
 else console.error(`Config changed concurrently; left it untouched. Recovery copy: ${options.output}/gui.before.toml`);
}
assert.ok(results.filter(r=>r.resolution===32).every(r=>{
 const baseline=results.find(b=>b.scene===r.scene&&(options.suite==='stage-one'?(b.flags===0&&b.views===128):b.resolution===8));
 return r.gpu_ms_p95<1000/60 && r.frame_interval_ms_p50<=1.05*Math.max(1000/60,baseline.frame_interval_ms_p50);
}), 'Default 32px failed the 60 Hz GPU budget or baseline-normalized frame interval; see summary.json. Do not claim performance acceptance.');
console.log(`PASS: runtime checks, default-32px 60 Hz GPU budget and baseline-normalized frame interval. Artifacts: ${options.output}`);
