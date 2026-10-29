import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawnSync,execFileSync} from 'node:child_process';
const [binaryName,label,...requested]=process.argv.slice(2);
if(!binaryName||!label)throw new Error('node target/research/run.mjs <binary-name> <label> [cases...]');
const binary=path.resolve(`target/research/bin/${binaryName}`);
const cases=requested.length?requested:['add-empty-tiny','add-empty-point','add-sparse-point','add-dense-point','add-dense-long','add-dense-cross','noop-dense-point','remove-dense-point','remove-dense-cross','dig-empty-point','dig-dense-point','dig-dense-cross','place-dense-point'];
const dir=path.resolve(`target/research/${label}`);fs.mkdirSync(dir,{recursive:true});
const manifest={label,binary,sha256:crypto.createHash('sha256').update(fs.readFileSync(binary)).digest('hex'),source:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),runs:[]};
for(const name of cases){
 const report=path.join(dir,name+'.json'),log=path.join(dir,name+'.log');
 const env={...process.env};for(const key of Object.keys(env))if(key.startsWith('RE_FLORA_'))delete env[key];
 Object.assign(env,{RE_FLORA_EDIT_COST_PROBE:'1',RE_FLORA_BRUSH_RESEARCH:name,RE_FLORA_BRUSH_REPORT:report,RE_FLORA_BRUSH_SAMPLES:'30',RUST_LOG:'info,re_flora::builder::surface=debug'});
 if(label.includes('drained'))env.RE_FLORA_BRUSH_PREFLIGHT_DRAIN='1';
 const args=['--hidden','--mute','--flat-terrain'];
 if(!label.includes('plain'))args.push('--perf');
 if(label.includes('immediate'))args.push('--present-mode','immediate');
 const fd=fs.openSync(log,'w');const result=spawnSync(binary,args,{env,stdio:['ignore',fd,fd]});fs.closeSync(fd);
 const text=fs.readFileSync(log,'utf8');
 if(result.status!==0||!text.includes('[BRUSH_RESEARCH] complete')||!text.includes('Application exited successfully')||/\bERROR\b|VUID-|panicked/.test(text))throw new Error(`invalid run ${name}: ${result.status}, see ${log}`);
 const data=JSON.parse(fs.readFileSync(report));if(data.events.length!==33)throw new Error('bad sample count');
 let active=null;const probes=new Map();
 for(const line of text.split('\n')){
  const begin=line.match(/\[BRUSH_RESEARCH\] begin.* sample=(\d+)/);if(begin){active={};probes.set(Number(begin[1]),active);continue;}
  if(line.includes('[BRUSH_RESEARCH] end')){active=null;continue;}if(!active)continue;
  const add=(k,v)=>active[k]=(active[k]??0)+Number(v);
  if(line.includes('[GRASS_COST_PROBE]'))for(const m of line.matchAll(/(prepare_us|submit_us|wait_us|hydration_us|authored_us|total_us)=([\d.]+)/g))add('grass_'+m[1],m[2]);
  if(line.includes('[EDIT_COST_PROBE]'))for(const m of line.matchAll(/(drain_us|record_us|submit_us|wait_us|modify_gpu_us|sample_gpu_us)=([\d.]+)/g))add('terrain_'+m[1],m[2]);
  if(line.includes('[FLORA_EDIT_PASS_TIMING]'))for(const m of line.matchAll(/(pass_total|clear_occupancy|instances_to_occupancy|edit_occupancy|occupancy_to_instances)=([\d.]+)ms/g))add('grass_gpu_'+m[1]+'_us',Number(m[2])*1000);
  if(line.includes('[SURFACE_BUILD]')){const m=line.match(/ total ([\d.]+)ms/);if(m)add('surface_wall_us',Number(m[1])*1000);}
  if(line.includes('[SURFACE_BUILD_PASS_TIMING]')){const m=line.match(/pass_total=([\d.]+)ms/);if(m)add('surface_gpu_us',Number(m[1])*1000);}
 }
 for(const event of data.events)event.probes=probes.get(event.sample);
 fs.writeFileSync(report,JSON.stringify(data,null,2)+'\n');
 const events=data.events.filter(e=>!e.warmup);
 const stats=v=>{v=v.toSorted((a,b)=>a-b);return {n:v.length,median:v[Math.floor(v.length/2)],p95:v[Math.ceil(v.length*.95)-1],min:v[0],max:v.at(-1)};};
 const summary={case:name,edit_ms:stats(events.map(e=>e.edit_ms)),preflight_ms:stats(events.map(e=>e.preflight_ms)),roots:[...new Set(events.map(e=>`${e.before_roots}->${e.after_roots}`))],hashes:[...new Set(events.map(e=>e.root_hash+':'+e.atlas_hash))],changed_voxels:[...new Set(events.map(e=>e.changed_voxels))],probes:{}};
 for(const key of new Set(events.flatMap(e=>Object.keys(e.probes??{}))))summary.probes[key]=stats(events.map(e=>e.probes[key]??0));
 manifest.runs.push({name,command:[binary,...args],env,summary});
 // Record only explicitly controlled diagnostic env, never inherited credentials.
 manifest.runs.at(-1).env=Object.fromEntries(Object.entries(env).filter(([k])=>k.startsWith('RE_FLORA_')||k==='RUST_LOG'));
 fs.writeFileSync(path.join(dir,'manifest.json'),JSON.stringify(manifest,null,2)+'\n');
 console.log(JSON.stringify(summary));
}
