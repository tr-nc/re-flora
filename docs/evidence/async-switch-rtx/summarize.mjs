import {readFileSync,writeFileSync} from 'node:fs';
const d=JSON.parse(readFileSync(process.argv[2],'utf8'));
const stats=xs=>{const a=[...xs].sort((a,b)=>a-b),n=a.length;if(!n)return null;return {n,mean:a.reduce((s,x)=>s+x,0)/n,median:n%2?a[(n-1)/2]:(a[n/2-1]+a[n/2])/2,p95:a[Math.ceil(n*.95)-1],min:a[0],max:a[n-1]};};
const summary=[...new Set(d.runs.map(r=>r.scenario))].map(scenario=>{
 const row={scenario};for(const mode of ['sync','async']){
  const runs=d.runs.filter(r=>r.scenario===scenario&&r.mode===mode),events=runs.flatMap(r=>r.report.events.filter(e=>!e.warmup));
  row[mode]={edit_ms:stats(events.map(e=>e.edit_ms)),run_medians:runs.map(r=>stats(r.report.events.filter(e=>!e.warmup).map(e=>e.edit_ms)).median),cold_ms:runs.map(r=>r.report.events[0].edit_ms)};
  if(events[0]?.async_metrics){for(const key of ['submit_ms','finish_ms','accepted_to_publish_ms'])row[mode][key]=stats(events.map(e=>e.async_metrics[key]));row[mode].frames={};for(const e of events)row[mode].frames[e.async_frames]=(row[mode].frames[e.async_frames]??0)+1;}
 }
 row.median_delta_pct=(row.async.edit_ms.median/row.sync.edit_ms.median-1)*100;
 row.p95_delta_pct=(row.async.edit_ms.p95/row.sync.edit_ms.p95-1)*100;
 return row;
});
const output={quantiles:'median = mean of middle two for even n; p95 = nearest rank ceil(n * .95); warmup = first 3 per process',summary};
writeFileSync(process.argv[3],JSON.stringify(output,null,2));
for(const r of summary)console.log(`${r.scenario}: sync ${r.sync.edit_ms.median.toFixed(3)} / ${r.sync.edit_ms.p95.toFixed(3)}, async ${r.async.edit_ms.median.toFixed(3)} / ${r.async.edit_ms.p95.toFixed(3)}, delta ${r.median_delta_pct.toFixed(1)}%, pub ${r.async.accepted_to_publish_ms?.median.toFixed(3)??'-'} / ${r.async.accepted_to_publish_ms?.p95.toFixed(3)??'-'}, runs ${r.sync.run_medians.map(x=>x.toFixed(3))} vs ${r.async.run_medians.map(x=>x.toFixed(3))}`);
