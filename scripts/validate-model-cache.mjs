#!/usr/bin/env node
// Explicit GPU correctness run, not performance evidence or a Cargo unit test.
import {spawnSync} from 'node:child_process';
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
const help=`Usage: node scripts/validate-model-cache.mjs [--seconds 20] [--output target/model-cache-review]
Runs hidden/muted Release/Vulkan with real planted flowers, attached/fallen apples,
64 rotating model leaves and 21 butterflies. Requires a desktop/Vulkan session.
Checks all eight float bits of every consumed cached surface against the shared
canonical generator. Sweeps 8/16/37/128/512 views, independent resolutions, A/B,
lighting/growth/size, actual apple drops and resize after submitted cached draws.
512-view correctness cases use 8px; this is NOT a multi-GiB stress/perf test.
Checks no rebuild for pose/light/display/resize-only changes. No player saves
or saved GUI edits. Do not run another game concurrently. Logs/image/summary kept.
Exit codes: 0 pass/help, 1 validation/runtime failure, 2 invalid usage.
Example: node scripts/validate-model-cache.mjs --seconds 30`;
const args=process.argv.slice(2), options={seconds:20,output:'target/model-cache-review'};
if(args.length===1&&['--help','-h'].includes(args[0])){console.log(help);process.exit(0);}
for(let i=0;i<args.length;i+=2){
 const key=args[i]?.replace(/^--/,'');
 if(!args[i].startsWith('--')||!Object.hasOwn(options,key)||!args[i+1]||args[i+1].startsWith('--')){console.error(`Invalid option/value near ${args[i]}\n${help}`);process.exit(2);}
 options[key]=key==='seconds'?Number(args[i+1]):args[i+1];
}
if(!Number.isFinite(options.seconds)||options.seconds<12){console.error(`--seconds must be at least 12\n${help}`);process.exit(2);}
process.chdir(fileURLToPath(new URL('../',import.meta.url)));
async function main(){
 const dir=options.output;await mkdir(dir,{recursive:true});
 const before=await readFile('config/gui.toml');
 const env={...process.env};
 for(const key of Object.keys(env))if(key.startsWith('RE_FLORA_'))delete env[key];
 Object.assign(env,{RE_FLORA_MODEL_CACHE_REVIEW:'1',RE_FLORA_APPLE_MODEL_REVIEW:'resolution',
  RE_FLORA_MODEL_PIXEL_PREVIEW_REVIEW:'1',RE_FLORA_MODEL_PIXEL_STRESS_LEAVES:'64',RE_FLORA_FLOWER_MODEL_REVIEW:'b'});
 if(process.platform==='linux'&&env.DISPLAY){delete env.WAYLAND_DISPLAY;delete env.WAYLAND_SOCKET;env.WINIT_X11_SCALE_FACTOR='2.25';}
 const p=spawnSync('cargo',['run','--release','--','--hidden','--mute','--windowed','--resize-lifecycle-test',
  '--screenshot','player-default',`${dir}/scene.png`,'--screenshot-delay','6','--auto-exit',String(options.seconds)],
  {env,encoding:'utf8',maxBuffer:64*1024*1024});
 const text=(p.stdout||'')+(p.stderr||'');await writeFile(`${dir}/validation.log`,text);
 assert.equal(p.status,0,`App failed: ${p.error||''}; inspect ${dir}/validation.log`);
 assert.doesNotMatch(text,/\bERROR\b|VUID-|panicked at/);
 assert.deepEqual(await readFile('config/gui.toml'),before,'Cache review modified saved GUI settings');
 assert.match(text,/Application exited successfully/);
 assert.match(text,/MODEL_PIXEL_STRESS\] leaves=64 butterflies=21/);
 assert.equal([...text.matchAll(/FLOWER_REVIEW_PLANT\] species=/g)].length,8);
 assert.match(text,/\[COLLISION\]\[FRUIT\] dropped tree=/,'No actual fruit drop');
 assert.match(text,/MODEL_CACHE_RESIZE\] ready_banks=true replay_after_consumers=true/);
 const generations=[...text.matchAll(/RESIZE_LIFECYCLE\] phase=frame frame_generation=(\d+) swapchain_generation=(\d+) tracer_generation=(\d+)/g)];
 assert.ok(generations.length>0&&generations.every(([,a,b,c])=>a===b&&b===c),'Inconsistent resize generation');
 assert.ok([...text.matchAll(/RESIZE_LIFECYCLE\] phase=requests_complete count=5/g)].length>=2,'Need startup and post-consumer resize sequences');
 const phases=[...text.matchAll(/\[MODEL_CACHE_PHASE\] phase=(\d+) views=(\d+) resolutions=(\d+)\/(\d+)\/(\d+)\/(\d+)/g)];
 assert.deepEqual(phases.map(m=>+m[1]),Array.from({length:13},(_,i)=>i),'Incomplete cache sweep; retry --seconds 30');
 const expectedBuilds=[[0,1,2,3],[],[0],[1],[2],[3],[0,1,2,3],[0,1,2,3],[0,1,2,3],[0,1,2,3],[0,1,2,3],[0,1,2,3],[]];
 const totals=[0,0,0,0],results=[];
 for(let i=0;i<phases.length;i++){
  const m=phases[i],segment=text.slice(m.index,phases[i+1]?.index);
  const builds=[...segment.matchAll(/MODEL_CACHE_BUILD\] kind=(\d+) views=(\d+) resolution=(\d+) shapes=(\d+)/g)];
  assert.deepEqual(builds.map(b=>+b[1]),expectedBuilds[i],`Phase ${i}: wrong bank rebuild set`);
  for(const b of builds){assert.equal(+b[2],+m[2]);assert.equal(+b[3],+m[3+(+b[1])]);}
  const counters=[0,0,0,0];
  for(const row of segment.matchAll(/MODEL_CACHE_ORACLE\] leaf=(\d+) apple=(\d+) butterfly=(\d+) flower=(\d+) mismatches=(\d+)\/(\d+)\/(\d+)\/(\d+)/g)){
   for(let k=0;k<4;k++){counters[k]+=+row[k+1];assert.equal(+row[k+5],0,`Phase ${i}: kind ${k} mismatch`);}
  }
  assert.ok(counters.every(n=>n>0),`Phase ${i}: every consumer must check real cached samples`);
  counters.forEach((n,k)=>totals[k]+=n);
  results.push({phase:i,views:+m[2],resolutions:m.slice(3,7).map(Number),rebuilt_kinds:builds.map(b=>+b[1]),checked_texels:counters});
 }
 const png=await readFile(`${dir}/scene.png`);assert.ok(png.length>100);
 const summary={consumers:['model leaves','attached/fallen apples','butterflies','whole flowers/complete heads'],checked_texels:totals,mismatches:[0,0,0,0],
  config_unchanged:true,width:png.readUInt32BE(16),height:png.readUInt32BE(20),performance_evidence:false,results};
 await writeFile(`${dir}/summary.json`,JSON.stringify(summary,null,2)+'\n');
 console.log(`PASS: all four shared cache banks, bit-exact GPU oracle, isolated rebuilds, pose/light/display reuse, fruit drops and resize.\nChecked texels: ${totals.join('/')}\nArtifacts: ${dir}`);
}
main().catch(e=>{console.error(`${e.message}\nInspect ${options.output}/validation.log; use --help for prerequisites, or --seconds 30 for incomplete phases.`);process.exitCode=1;});
