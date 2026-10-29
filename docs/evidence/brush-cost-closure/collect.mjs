// Run from the isolated replay worktree after run.mjs. Output is a portable evidence bundle.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
const [destination]=process.argv.slice(2);
if(!destination)throw new Error('node collect.mjs <results.json>');
const labels=['baseline-a1','drained-a1','drained-a2','baseline-a2','immediate-a1','immediate-a2','plain-a1','plain-drained-a1','plain-drained-a2','plain-a2','plain-immediate-a1','plain-immediate-a2'];
const stats=v=>{v=v.toSorted((a,b)=>a-b);const n=v.length;return {n,median:n%2?v[n>>1]:(v[n/2-1]+v[n/2])/2,p95:v[Math.ceil(n*.95)-1],min:v[0],max:v.at(-1)};};
const sha=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const runs=[],signatures=new Map();
for(const label of labels){
 const dir=`target/research/${label}`,manifest=JSON.parse(fs.readFileSync(`${dir}/manifest.json`));
 const mode=(label.includes('plain')?'plain':'profiled')+'_'+(label.includes('drained')?'drained':label.includes('immediate')?'immediate':'fifo');
 for(const run of manifest.runs){
  const raw=`${dir}/${run.name}.log`,log=fs.readFileSync(raw,'utf8');
  if(/\bERROR\b|VUID-|panicked/.test(log)||!log.includes('Application exited successfully')||!log.includes('[BRUSH_RESEARCH] complete'))throw new Error('invalid '+raw);
  const events=JSON.parse(fs.readFileSync(`${dir}/${run.name}.json`)).events;
  if(events.length!==33||events.filter(e=>!e.warmup).length!==30)throw new Error('missing samples');
  for(const e of events){
   const signature=[e.before_roots,e.after_roots,e.changed_voxels,e.root_hash,e.atlas_hash].join('/');
   if(signatures.has(run.name)&&signatures.get(run.name)!==signature)throw new Error(`workload/result mismatch: ${run.name}`);
   signatures.set(run.name,signature);
  }
  runs.push({label,mode,case:run.name,binary_sha256:manifest.sha256,source:manifest.source,command:run.command,env:run.env,log_sha256:sha(raw),events});
 }
}
const groups=[];
for(const key of new Set(runs.map(r=>r.mode+':'+r.case))){
 const subset=runs.filter(r=>r.mode+':'+r.case===key),events=subset.flatMap(r=>r.events.filter(e=>!e.warmup));
 const probes={};for(const name of new Set(events.flatMap(e=>Object.keys(e.probes))))probes[name]=stats(events.map(e=>e.probes[name]??0));
 groups.push({mode:subset[0].mode,case:subset[0].case,edit_ms:stats(events.map(e=>e.edit_ms)),preflight_ms:stats(events.map(e=>e.preflight_ms)),combined_ms:stats(events.map(e=>e.edit_ms+e.preflight_ms)),per_run:subset.map(r=>({label:r.label,edit_ms:stats(r.events.filter(e=>!e.warmup).map(e=>e.edit_ms))})),probes});
}
const result={schema:1,description:'Isolated Release diagnostic; no optimization shipped. Raw measured events retain per-event probes; input pick, fixture reset, validation and future frames excluded.',platform:'Apple M4 Pro / macOS / MoltenVK; fullscreen 5120x2880, scene 1024x576',total_runs:runs.length,measured_events:runs.reduce((n,r)=>n+r.events.filter(e=>!e.warmup).length,0),warmup_events:runs.reduce((n,r)=>n+r.events.filter(e=>e.warmup).length,0),signatures:Object.fromEntries(signatures),groups,runs};
fs.mkdirSync(path.dirname(destination),{recursive:true});fs.writeFileSync(destination,JSON.stringify(result)+'\n');
console.log(`runs=${result.total_runs} measured=${result.measured_events} warmup=${result.warmup_events}; all per-case root/atlas/count signatures match across repetitions and diagnostic modes`);
for(const g of groups.filter(g=>g.mode.startsWith('plain')))console.log(g.mode,g.case,'edit median/p95',g.edit_ms.median.toFixed(3),g.edit_ms.p95.toFixed(3),'preflight',g.preflight_ms.median.toFixed(3),'combined',g.combined_ms.median.toFixed(3),g.combined_ms.p95.toFixed(3));
