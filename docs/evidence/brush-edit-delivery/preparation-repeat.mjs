import {spawnSync} from 'node:child_process';
import fs from 'node:fs';
const root='target/grass-edit-preparation';
const stats=a=>{a.sort((a,b)=>a-b);return {median:a[Math.floor(a.length/2)],p95:a[Math.min(a.length-1,Math.floor(a.length*.95))]}};
const all=[];
for(const [i,scenario] of ['add-empty-tiny','add-dense-point','add-dense-cross','noop-dense-point'].entries()) {
  for(const variant of (i%2?['before','after','sync']:['after','before','sync'])) {
    const env={...process.env,RUST_LOG:'info',RE_FLORA_BRUSH_EDIT_BENCH:scenario,RE_FLORA_BRUSH_BENCH_REPORT:`${root}/repeat-${scenario}-${variant}.json`,RE_FLORA_BRUSH_BENCH_SAMPLES:'24'};
    if(variant==='sync')delete env.RE_FLORA_BRUSH_BENCH_ASYNC;else env.RE_FLORA_BRUSH_BENCH_ASYNC='1';
    const fd=fs.openSync(`${root}/repeat-${scenario}-${variant}.log`,'w');
    const r=spawnSync(`${root}/bin/${variant==='before'?'before':'after'}`,['--hidden','--mute','--flat-terrain'],{env,stdio:['ignore',fd,fd],timeout:180000});
    fs.closeSync(fd);
    const log=fs.readFileSync(`${root}/repeat-${scenario}-${variant}.log`,'utf8');
    if(r.status!==0||/\bERROR\b|VUID-|panicked/.test(log)||!log.includes('Application exited successfully'))throw Error(`${scenario}-${variant}`);
    const events=JSON.parse(fs.readFileSync(env.RE_FLORA_BRUSH_BENCH_REPORT,'utf8')).events.filter(e=>!e.warmup);
    const signatures=[...new Set(events.map(e=>JSON.stringify([e.before_roots,e.after_roots,e.changed_voxels,e.root_hash,e.atlas_hash])))];
    if(events.length!==24||signatures.length!==1)throw Error(`inconsistent ${scenario}-${variant}`);
    const row={scenario,variant,edit_ms:stats(events.map(e=>e.edit_ms)),signature:signatures[0]};
    if(variant!=='sync') {
      row.submit_ms=stats(events.map(e=>e.async_metrics.submit_ms));
      row.finish_ms=stats(events.map(e=>e.async_metrics.finish_ms));
      row.publication_ms=stats(events.map(e=>e.async_metrics.accepted_to_publish_ms));
      if(events.some(e=>e.async_metrics.retries))throw Error('retry');
    }
    if(all.some(other=>other.scenario===scenario&&other.signature!==row.signature))throw Error(`pair differs ${scenario}`);
    all.push(row);
    fs.writeFileSync(`${root}/repeat-summary.json`,JSON.stringify(all,null,2));
    console.log(JSON.stringify(row));
  }
}
