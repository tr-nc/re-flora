#!/usr/bin/env node
// Opt-in real Release/Vulkan diagnosis, never a Cargo unit test or release gate.
import {spawn} from 'node:child_process';
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';

const help=`Usage: node scripts/diagnose-flower-performance.mjs [options]
  --suite matrix|offscreen   Default: matrix (8 runs); offscreen: 2-run repro
  --seconds NUMBER          Seconds per run, at least 6 (default: 8)
  --binary PATH             Default: target/release/re-flora; build Release first
  --output DIRECTORY        Default: target/flower-perf-diagnosis
  --check-offscreen         Fail if 128 unseen flowers add >1 ms median tile work
  -h, --help                Show this help

Requires a Vulkan-capable desktop session. Do not run another game concurrently.
Uses 0/32/128 real Wild Geranium plants, front/away/outside-chunk views, 8/32px.
Hidden, muted, windowed; Linux/X11 is pinned to 2880x1620. Settings are changed
in memory only; config/gui.toml is checked unchanged and player saves are unused.
Leaves logs + summary.json. Timings exclude 2.5 s warmup; frame intervals use
consecutive GPU log timestamps/frame numbers, NOT slow-frame-only PERF/FRAME.
The 1 ms check is a deliberately loose reproducer alarm, NOT a release budget.
Exit: 0 measurement/help completed, 1 runtime/reproducer failure, 2 invalid usage.

Build: cargo build --release
Fast red/green loop: node scripts/diagnose-flower-performance.mjs --suite offscreen --check-offscreen
Full diagnosis: node scripts/diagnose-flower-performance.mjs --suite matrix`;
const options={suite:'matrix',seconds:8,binary:'target/release/re-flora',output:'target/flower-perf-diagnosis',check:false};
function usage(message){console.error(`${message}\n${help}`);process.exit(2);}
const args=process.argv.slice(2);
if(args.length===1&&['-h','--help'].includes(args[0])){console.log(help);process.exit(0);}
for(let i=0;i<args.length;i++){
 if(args[i]==='--check-offscreen'){options.check=true;continue;}
 const key=args[i]?.slice(2);
 if(!args[i].startsWith('--')||!['suite','seconds','binary','output'].includes(key)||!args[i+1]||args[i+1].startsWith('--'))usage(`Invalid option/value near ${args[i]}`);
 options[key]=key==='seconds'?Number(args[++i]):args[++i];
}
if(!['matrix','offscreen'].includes(options.suite))usage('--suite must be matrix or offscreen');
if(!Number.isFinite(options.seconds)||options.seconds<6)usage('--seconds must be at least 6');
process.chdir(fileURLToPath(new URL('../',import.meta.url)));
let child,interrupted=false;
for(const signal of ['SIGINT','SIGTERM'])process.once(signal,()=>{interrupted=true;child?.kill('SIGTERM');});
function run(parameters){return new Promise((resolve,reject)=>{
 let text='';
 const env={...process.env,RE_FLORA_FLOWER_BENCH:parameters};
 // Inherited diagnostic fixtures would contaminate this workload.
 for(const key of Object.keys(env))if(key.startsWith('RE_FLORA_')&&key!=='RE_FLORA_FLOWER_BENCH')delete env[key];
 if(process.platform==='linux'&&env.DISPLAY){delete env.WAYLAND_DISPLAY;delete env.WAYLAND_SOCKET;env.WINIT_X11_SCALE_FACTOR='2.25';}
 child=spawn(path.resolve(options.binary),['--hidden','--mute','--windowed','--perf','--auto-exit',String(options.seconds)],{env,stdio:['ignore','pipe','pipe']});
 child.stdout.on('data',b=>text+=b);child.stderr.on('data',b=>text+=b);
 child.once('error',reject);child.once('close',code=>{child=undefined;resolve({text,code});});
});}
function quantile(values,q){assert.ok(values.length,'No timing samples');const s=values.toSorted((a,b)=>a-b);return s[Math.floor((s.length-1)*q)];}
function metrics(text,needsFlowerScope){
 const rows=[...text.matchAll(/\[(\d+):(\d+):(\d+\.\d+) [^\n]*?\[PERF\]\[GPU_FRAME_SCOPE\] frame (\d+) ([^\n]+)/g)].map(m=>{
  const scopes={};
  for(const v of m[5].matchAll(/([\w.]+)=(\d+)us/g))scopes[v[1]]=(scopes[v[1]]||0)+Number(v[2])/1000;
  return {time:(+m[1]*3600 + +m[2]*60 + +m[3])*1000,frame:+m[4],dropped:Number(m[5].match(/\bdropped=(\d+)/)?.[1]),scopes};
 });
 assert.ok(rows.length>=5,'Too few GPU samples; rerun with --seconds 20');
 const warm=rows.filter(r=>r.time>=rows[0].time+2500);
 assert.ok(warm.length>=3,'Too few post-warmup samples; rerun with --seconds 20');
 assert.ok(warm.every(r=>r.dropped===0&&Number.isFinite(r.scopes['frame.render'])),'Incomplete GPU profiler scopes');
 if(needsFlowerScope)assert.ok(warm.every(r=>Number.isFinite(r.scopes['models.flowers.tiles'])),'Submitted flowers have no GPU scope; do not treat missing evidence as zero work');
 const intervals=warm.slice(1).map((r,i)=>(r.time-warm[i].time)/(r.frame-warm[i].frame));
 const median=scope=>quantile(warm.map(r=>r.scopes[scope]||0),.5);
 return {samples:warm.length,gpu_ms_p50:median('frame.render'),gpu_ms_p95:quantile(warm.map(r=>r.scopes['frame.render']),.95),
  flower_tile_ms_p50:median('models.flowers.tiles'),flower_display_ms_p50:median('models.flowers.display'),
  vegetation_response_ms_p50:median('vegetation_response.pass'),wind_volume_ms_p50:median('wind_volume.pass'),
  frame_interval_ms_p50:quantile(intervals,.5),frame_interval_ms_p95:quantile(intervals,.95),fps_from_median_interval:1000/quantile(intervals,.5)};
}
async function main(){
 await mkdir(options.output,{recursive:true});
 const original=await readFile('config/gui.toml');
 const configHash=createHash('sha256').update(original).digest('hex');
 const cases=options.suite==='offscreen'?['0,away,32,heads','128,away,32,heads']:
  ['0,front,32,heads','32,front,32,heads','128,front,32,heads','0,away,32,heads','128,away,32,heads','128,away,8,heads','0,outside,32,heads','128,outside,32,heads'];
 const results=[];
 for(const parameters of cases){
  assert.ok(!interrupted,'Interrupted');
  assert.deepEqual(await readFile('config/gui.toml'),original,'Config changed concurrently; stop the other app/editor and retry');
  const name=parameters.replaceAll(',','-'),{code,text}=await run(parameters);
  await writeFile(path.join(options.output,`${name}.log`),text);
  assert.equal(code,0,`${name}: app failed; inspect retained log`);
  assert.doesNotMatch(text,/\bERROR\b|VUID-|panicked at/,`${name}: runtime error`);
  assert.match(text,/Application exited successfully/);
  const [count,view,resolution,mode]=parameters.split(',');
  assert.ok(text.includes(`[FLOWER_BENCH] species=wild-geranium count=${count} view=${view[0].toUpperCase()+view.slice(1)} resolution=${resolution} heads_only=${mode==='heads'}`),'Requested fixture was not activated');
  const plan=text.match(/\[FLOWER_BENCH_PLAN\] plants=(\d+) tiles=(\d+) texels=(\d+)/);
  assert.ok(plan,'Missing flower dispatch accounting; rebuild with cargo build --release');
  if(view==='front')assert.equal(+plan[1],+count,'Front-view population was not submitted');
  if(view==='outside')assert.equal(+plan[1],0,'Outside-chunk fixture must reject the flower chunk');
  const extent=text.match(/Hidden window render extent is (\d+)x(\d+)/);
  assert.ok(extent,'Missing render extent');
  const row={count:+count,view,resolution:+resolution,mode,width:+extent[1],height:+extent[2],planned_plants:+plan[1],planned_tiles:+plan[2],planned_texels:+plan[3],...metrics(text,+plan[2]>0)};
  if(results.length)assert.deepEqual([row.width,row.height],[results[0].width,results[0].height],'Viewport changed between runs');
  assert.deepEqual(await readFile('config/gui.toml'),original,'Diagnostic changed config/gui.toml; inspect before continuing');
  results.push(row);console.log(JSON.stringify(row));
  await writeFile(path.join(options.output,'summary.json'),JSON.stringify({binary:path.resolve(options.binary),seconds:options.seconds,config_sha256:configHash,config_unchanged:true,release_acceptance:false,results},null,2)+'\n');
 }
 if(options.check){
  const baseline=results.find(r=>r.count===0&&r.view==='away');
  const populated=results.find(r=>r.count===128&&r.view==='away'&&r.resolution===32);
  const extra=populated.flower_tile_ms_p50-baseline.flower_tile_ms_p50;
  assert.ok(extra<=1,`OFFSCREEN REGRESSION: 128 unseen flowers add ${extra.toFixed(3)} ms median pixel-generation GPU work (diagnostic alarm: 1 ms; not a release budget)`);
 }
 console.log(`Measurements complete, not performance acceptance. Artifacts: ${options.output}`);
}
main().catch(error=>{console.error(`${error.message}\nInspect logs in ${options.output}. Build with cargo build --release; retry command and options: --help.`);process.exitCode=1;});
