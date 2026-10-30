#!/usr/bin/env node
// Recompute the report from preserved per-frame samples and targeted native logs.
import fs from 'node:fs';
import path from 'node:path';
import {gzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
const source=path.resolve(process.argv[2]??'target/world-frame-cost');
const out=path.resolve('docs/evidence/world-frame-cost');
const groups=['normal','detailed','no-flora','no-shadows','late','sparse','hydration','hydration-io'];
const quantile=(values,p)=>{const a=values.toSorted((a,b)=>a-b),i=(a.length-1)*p,lo=Math.floor(i),hi=Math.ceil(i);return a[lo]+(a[hi]-a[lo])*(i-lo);};
const stats=a=>({n:a.length,mean:a.reduce((s,v)=>s+v,0)/a.length,p50:quantile(a,.5),p95:quantile(a,.95),min:Math.min(...a),max:Math.max(...a)});
const metrics=samples=>Object.fromEntries([...new Set(samples.flatMap(s=>Object.keys(s.values)))].map(k=>[k,stats(samples.map(s=>s.values[k]??0))]));
const clock=s=>{const m=s.match(/^\[(\d\d):(\d\d):(\d\d)\.(\d{3})/);return m?(+m[1]*3600 + +m[2]*60 + +m[3]+ +m[4]/1000):null;};
const sha=b=>createHash('sha256').update(b).digest('hex');
const result={beforeCommit:'73dde10c',afterCommit:'8d98fc5a',hardware:'RTX 3060 Ti 8 GiB / i5-12600KF / NVIDIA 580.159.04 / Linux',groups:{},sourceReferences:{grassPlan:'src/tracer/flora_frame_plan.rs:92-151',grassDraw:'src/tracer/mod.rs:5552-5586',grassShader:'shader/slang/grass_band_cached.vert.slang:26-43',grassCache:'src/tracer/mod.rs:4931-4997',grassLighting:'shader/slang/flora_lighting_cache.comp.slang:47-120',grassSpecies:'src/flora/species.rs:157-183',flowerPreparation:'src/tracer/mod.rs:5099-5197',hydration:'src/builder/surface/hydration.rs:19-149',atlasRead:'src/builder/plain/mod.rs:576-613',cpuAcquire:'src/app/core/mod.rs:3386-3391',ddgiBudget:'src/ddgi/config.rs:1-9',profileCapacity:'src/app/core/mod.rs:1200-1205'}};
let evidence='Native evidence excerpts; full frame timing samples are in *.samples.json.gz.\n';
for(const group of groups){
 const manifest=JSON.parse(fs.readFileSync(path.join(source,group,'summary.json')));
 if(result.configHash)assert.equal(manifest.configHash,result.configHash);else result.configHash=manifest.configHash;
 const g={options:manifest.options,binaryHashes:manifest.binaryHashes,runs:[],aggregate:{}};result.groups[group]=g;
 const pool={before:{cpu:[],gpu:[]},after:{cpu:[],gpu:[]}};
 for(const row of manifest.runs){
  const raw=fs.readFileSync(path.join(source,group,`${row.name}.samples.json`));
  const samples=JSON.parse(raw);const name=`${group}-${row.name}`;
  fs.writeFileSync(path.join(out,`${name}.samples.json.gz`),gzipSync(raw,{level:9}));
  const log=fs.readFileSync(path.join(source,group,`${row.name}.log`),'utf8');
  assert(!/\bERROR\b|VUID-|SYNC-HAZARD|panicked at/.test(log));
  const meta={...row,cpu:metrics(samples.cpu),gpu:metrics(samples.gpu),logSha256:sha(log),samplesSha256:sha(raw),samplesFile:`${name}.samples.json.gz`};
  g.runs.push(meta);pool[row.variant].cpu.push(...samples.cpu);pool[row.variant].gpu.push(...samples.gpu);
  evidence+=`\n=== ${name} ===\n`;
  const lines=log.split('\n');
  evidence+=lines.filter(l=>/Chosen swapchain present mode|\[SCENE_PIXELS\]|\[STARTUP\] status=ready|Contree pool sizes|\[DDGI\] allocated|\[DDGI\]\[FLORA_CONSUMER\]|\[VEGETATION_RESPONSE\] grid|\[GRASS_PIXEL_PROFILE\]|\[SHUTDOWN\] phase=complete|Application exited successfully/.test(l)).join('\n')+'\n';
  const consumer=log.match(/\[DDGI\]\[FLORA_CONSUMER\].*instance_count=(\d+) cache_entries=(\d+)/);
  if(consumer)meta.grass={instances:+consumer[1],bandEntries:+consumer[2],submittedBands:+consumer[1]*8,submittedIndices:+consumer[1]*8*36,usefulBandIndices:+consumer[2]*36};
  if(group.startsWith('hydration')){
   const ready=clock(row.readyLine), byFrame=new Map(samples.cpu.map(s=>[s.frame,s]));
   let io=[],pending=null;const events=[];
   for(const line of lines){
    const read=line.match(/\[PERF\]\[ATLAS_READ\] transfer_wait_ms=([\d.]+) host_copy_ms=([\d.]+) bytes=(\d+) regions=(\d+)/);
    if(read)io.push({wait:+read[1],copy:+read[2],bytes:+read[3],regions:+read[4]});
    const h=line.match(/\[PLANT_HYDRATION\] dt=([\d.]+) plants=(\d+) read_bytes=(\d+).*cpu_ms=([\d.]+)/);
    if(h){const wait=io.reduce((s,r)=>s+r.wait,0),copy=io.reduce((s,r)=>s+r.copy,0);pending={time:clock(line)-ready,dt:+h[1],plants:+h[2],readBytes:+h[3],wallMs:+h[4],requests:io.length,transferWaitMs:wait,firstTransferWaitMs:io[0]?.wait??0,remainingTransferWaitMs:wait-(io[0]?.wait??0),hostCopyMs:copy,remainingMs:+h[4]-wait-copy};io=[];}
    const f=line.match(/\[CPU_FRAME_SCOPE\] frame (\d+)/);
    if(f&&pending){const frame=+f[1],s=byFrame.get(frame);if(s&&pending.time>= +manifest.options.warmup)events.push({...pending,frame,frameMs:s.values['frame.cpu_total']/1000,untrackedMs:s.values['frame.untracked_cpu']/1000});pending=null;}
   }
   meta.hydration={events,wallMs:stats(events.map(e=>e.wallMs)),transferWaitMs:stats(events.map(e=>e.transferWaitMs)),firstTransferWaitMs:stats(events.map(e=>e.firstTransferWaitMs)),remainingTransferWaitMs:stats(events.map(e=>e.remainingTransferWaitMs)),hostCopyMs:stats(events.map(e=>e.hostCopyMs)),remainingMs:stats(events.map(e=>e.remainingMs)),requests:stats(events.map(e=>e.requests)),amortizedWallMs:events.reduce((s,e)=>s+e.wallMs,0)/samples.cpu.length,slowFrames:samples.cpu.filter(s=>s.values['frame.cpu_total']>25000).length,slowFramesWithHydration:events.filter(e=>e.frameMs>25).length,unclassifiedResidualMeanMs:samples.cpu.reduce((s,v)=>s+v.values['frame.untracked_cpu']/1000,0)/samples.cpu.length-events.reduce((s,e)=>s+e.wallMs,0)/samples.cpu.length};
   evidence+=lines.filter(l=>l.includes('[PLANT_HYDRATION]')||l.includes('[ATLAS_READ]')).join('\n')+'\n';
  }
 }
 for(const [variant,p] of Object.entries(pool))g.aggregate[variant]={cpu:metrics(p.cpu),gpu:metrics(p.gpu),fps:stats(g.runs.filter(r=>r.variant===variant).map(r=>r.frameRate))};
}
const detailed=result.groups.detailed.aggregate;
const parents=new Set(['frame.render','tracer.render','graphics.pass','tracer.shadow_prepass']);
result.gpuLeaves=Object.entries(detailed.after.gpu).filter(([k])=>!parents.has(k)).map(([name,b])=>({name,before:detailed.before.gpu[name]??null,after:b,deltaMeanUs:b.mean-(detailed.before.gpu[name]?.mean??0),lateAfter:result.groups.late.aggregate.after.gpu[name]??null})).sort((a,b)=>b.after.mean-a.after.mean);
result.gpuUnscopedUs=Object.fromEntries(['before','after'].map(v=>[v,detailed[v].gpu['frame.render'].mean-result.gpuLeaves.reduce((s,r)=>s+(r[v]?.mean??0),0)]));
fs.writeFileSync(path.join(out,'native-evidence.log.gz'),gzipSync(evidence,{level:9}));
fs.writeFileSync(path.join(out,'data.json'),JSON.stringify(result,null,2));
console.log(JSON.stringify({normal:result.groups.normal.aggregate,hydration:result.groups['hydration-io'].runs.map(r=>({name:r.name,...r.hydration,events:undefined})),leafTop:result.gpuLeaves.slice(0,8).map(r=>({name:r.name,after:r.after.mean/1000,delta:r.deltaMeanUs/1000})),unscoped:result.gpuUnscopedUs},null,2));
