import {spawnSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdirSync,openSync,closeSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {resolve,join} from 'node:path';
const [binaryArg, cwdArg, outArg, countArg='24', casesArg]=process.argv.slice(2);
if(!binaryArg||!cwdArg||!outArg)throw Error('usage: node run.mjs RELEASE_BINARY ISOLATED_WORKTREE OUTPUT_DIR [SAMPLES=24] [COMMA_SEPARATED_CASES]');
const binary=resolve(binaryArg),cwd=resolve(cwdArg),out=resolve(outArg),count=Number(countArg);
mkdirSync(out,{recursive:true});
const sha=p=>createHash('sha256').update(readFileSync(p)).digest('hex');
const command=(c,args)=>spawnSync(c,args,{cwd,encoding:'utf8'}).stdout;
const metadata={created:new Date().toISOString(),revision:command('git',['rev-parse','HEAD']).trim(),binary,binary_sha256:sha(binary),cwd,config_sha256:sha(join(cwd,'config/gui.toml')),cpu:command('lscpu',[]),gpu:command('nvidia-smi',[]),args:['--hidden','--mute','--flat-terrain'],samples_per_run:count,warmups_per_run:3,order:'sync,async,async,sync',runs:[]};
const expectedExtent=process.env.BENCH_EXPECT_EXTENT;
metadata.expected_extent=expectedExtent??null;
const scenarios=casesArg?casesArg.split(','):['add-empty-tiny','add-sparse-point','add-dense-point','add-dense-long','add-dense-cross','noop-dense-point','noop-dense-cross','remove-dense-cross'];
for(const scenario of scenarios){
 let signature;
 for(const [i,mode] of ['sync','async','async','sync'].entries()){
  const prefix=join(out,`${scenario}-${i+1}-${mode}`),env={...process.env,RUST_LOG:'info',RE_FLORA_BRUSH_EDIT_BENCH:scenario,RE_FLORA_BRUSH_BENCH_REPORT:prefix+'.json',RE_FLORA_BRUSH_BENCH_SAMPLES:String(count)};
  for(const key of ['RE_FLORA_BRUSH_BENCH_ASYNC','RE_FLORA_BRUSH_PREFLIGHT_DRAIN','RE_FLORA_GRASS_TRANSACTION_REVIEW','RE_FLORA_GARDEN_PERF'])delete env[key];
  if(mode==='async')env.RE_FLORA_BRUSH_BENCH_ASYNC='1';
  const fd=openSync(prefix+'.log','w'),start=performance.now();
  const run=spawnSync(binary,metadata.args,{cwd,env,stdio:['ignore',fd,fd],timeout:180000});closeSync(fd);
  const log=readFileSync(prefix+'.log','utf8');
  if(run.status!==0||!log.includes('[BRUSH_EDIT_BENCH] complete')||!log.includes('Application exited successfully')||/\bERROR\b|VUID-|panicked/.test(log))throw Error(`failed ${prefix}: ${run.status} ${run.error}`);
  const report=JSON.parse(readFileSync(prefix+'.json','utf8')),events=report.events.filter(e=>!e.warmup);
  if(events.length!==count)throw Error('sample count');
  for(const event of report.events){
   const s=JSON.stringify([event.before_roots,event.after_roots,event.changed_voxels,event.root_hash,event.atlas_hash]);
   signature??=s;if(signature!==s)throw Error(`signature mismatch ${prefix}`);
   if(event.async_metrics?.retries)throw Error(`retry ${prefix}`);
  }
  if(sha(join(cwd,'config/gui.toml'))!==metadata.config_sha256)throw Error('config changed');
  const startup_extents=[...new Set([...log.matchAll(/\[SCENE_PIXELS\] screen=(\d+x\d+)/g)].map(m=>m[1]))],present_modes=[...new Set([...log.matchAll(/Chosen swapchain present mode: (\w+)/g)].map(m=>m[1]))];
  let currentExtent,started=false;const observed=new Set();
  for(const line of log.split('\n')){const m=line.match(/\[SCENE_PIXELS\] screen=(\d+x\d+)/);if(m)currentExtent=m[1];if(line.includes('[BRUSH_EDIT_BENCH] begin'))started=true;if(started)observed.add(currentExtent);}
  const extents=[...observed];
  metadata.runs.push({scenario,mode,round:i+1,extents,startup_extents,present_modes,seconds:(performance.now()-start)/1000,signature,log_sha256:sha(prefix+'.log'),report_sha256:sha(prefix+'.json'),log_file:prefix+'.log',report});
  writeFileSync(join(out,'data.json'),JSON.stringify(metadata,null,2));
  if(expectedExtent&&(extents.length!==1||extents[0]!==expectedExtent||present_modes.length!==1||present_modes[0]!=='MAILBOX'))throw Error(`unexpected rendering configuration ${prefix}: ${extents} ${present_modes}`);
  const a=events.map(e=>e.edit_ms).sort((a,b)=>a-b),n=a.length,median=n%2?a[(n-1)/2]:(a[n/2-1]+a[n/2])/2;console.log(`${scenario} ${mode} ${i+1}: median=${median.toFixed(3)} p95=${a[Math.ceil(n*.95)-1].toFixed(3)} ms`);
 }
}
console.log('PASS: all runs, output signatures, no retries, logs and unchanged GUI config');
