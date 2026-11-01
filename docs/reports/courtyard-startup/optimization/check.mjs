// Point-in-time evidence checks. Native runs, not this parser, establish performance.
import { readFileSync, writeFileSync, openSync, readSync, closeSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
const base = fileURLToPath(new URL('.', import.meta.url));
const log = name => gunzipSync(readFileSync(`${base}raw/${name}.log.gz`)).toString();
const median = xs => { const a = [...xs].sort((a,b)=>a-b); return a.length % 2 ? a[(a.length-1)/2] : (a[a.length/2-1]+a[a.length/2])/2; };
const parse = name => {
  const s = log(name);
  const readyMs = +s.match(/\[STARTUP\] status=ready .*total_ms=([\d.]+)/)[1];
  const stages = [...s.matchAll(/\[STARTUP\] status=done stage=(\S+) stage_ms=([\d.]+)/g)].map(m=>({name:m[1],ms:+m[2]}));
  assert(Math.abs(stages.reduce((n,s)=>n+s.ms,0)-readyMs)<2);
  return {name,readyMs,cache:s.match(/\[PIPELINE_CACHE\] status=(hit|miss)/)?.[1],stages,
    colliderMs:+s.match(/\[COLLISION\]\[WORLD\].*elapsed_ms=([\d.]+)/)[1],
    surfaces:[...s.matchAll(/\[TREE\]\[RASTER_STATIC\].*trees=(\d+).*compile_ms=([\d.]+)/g)].map(m=>({trees:+m[1],ms:+m[2]})),
    fragments:[...s.matchAll(/\[TREE\]\[COMPILE_CACHE\] reused=(\d+) rebuilt=(\d+)/g)].map(m=>({reused:+m[1],rebuilt:+m[2]}))};
};
const warm = [1,2,3].map(n=>parse(`accepted-warm-${n}`));
for(const r of warm) {
  const s=log(r.name);
  assert.equal(r.cache,'hit'); assert.equal(r.surfaces.length,1); assert.equal(r.surfaces[0].trees,5);
  for(const expected of ['sources=494044 proxies=138380','surface_cells=87512 triangles=365304','rest_fingerprint=958ea1cadea650a0','fingerprint=d031de6e','phase=complete failures=0']) assert(s.includes(expected),expected);
  assert(!/ ERROR |panicked at/.test(s));
}
const cold=parse('accepted-cold'); assert.equal(cold.cache,'miss');
const sequential=parse('sequential-control');
assert.deepEqual(sequential.fragments.map(r=>r.reused),[0,1,2,3,4]);
const replay=['collider-replay-default','collider-replay-seed7'].map(name=>{
  const matches=[...log(name).matchAll(/\[COLLIDER_EXPORT_REPLAY\].*voxels=(\d+) disagreements=(\d+)/g)];
  assert.equal(matches.length,8192); assert(matches.every(m=>m[2]==='0'));
  const voxels=matches.reduce((n,m)=>n+Number(m[1]),0); assert.equal(voxels,268435456);
  return {name,blocks:matches.length,voxels,disagreements:0};
});
assert(log('batch-before-sparse-fix').includes('capture flora=511008 authored=180 trees=5'));
for(const name of ['batch-after-sparse-fix','sequential-save-control']) assert(log(name).includes('capture flora=511077 authored=180 trees=5'));
for(const name of ['accepted-load','accepted-old-save']) assert(log(name).includes('restored flora=511077 authored=180 trees=5'));
for(const [name,text] of [['growth-blocked','obstruction_preserved=true verified=true'],['growth-cycle','age=0.40625'],['growth-edit','wood_edit_pauses=true verified=true'],['growth-override','manual_override_freezes=true verified=true'],['half-age-load','loaded=true age=0.5 elapsed=180.001 leaves=18 verified=true']]) assert(log(name).includes(text));
assert(log('tests-root').includes('1824 passed; 0 failed; 15 ignored'));
assert(log('tests-terrain').includes('8 passed; 0 failed'));
assert(!log('tests-physics').includes('test result: FAILED'));
const physicsPasses=[...log('tests-physics').matchAll(/test result: ok\. (\d+) passed/g)].reduce((n,m)=>n+Number(m[1]),0);
assert.equal(physicsPasses,78);
const baseline=JSON.parse(readFileSync(`${base}../measurements.json`));
const baselineNormal=baseline.runs.find(r=>r.name==='clean-2.log.gz').readyMs;
const warmMedian=median(warm.map(r=>r.readyMs));
const result={schema:1,runtimeCommit:'70179a06',baselineRuntimeCommit:baseline.baseCommit,units:'milliseconds',
  baselineNormalReadyMs:baselineNormal,baselineProfileMedianMs:baseline.warmReadyMedianMs,
  warmMedianMs:warmMedian,speedup:baselineNormal/warmMedian,reductionPercent:100*(1-warmMedian/baselineNormal),
  warm,cold,steps:['admission-warm','batch-warm','cache-warm','collider-warm','geometry-warm-1','geometry-warm-2'].map(parse),
  seed7:[1,2].map(n=>parse(`accepted-seed7-${n}`)),sequential,replay,physicsPasses};
writeFileSync(`${base}measurements.json`,JSON.stringify(result,null,2)+'\n');

// Optional: compare the large local fixtures without committing either save.
// node check.mjs --snapshots /path/to/baseline.rflterrain /path/to/optimized.rflterrain
if(process.argv[2]==='--snapshots') {
  function fixture(path) {
    const fd=openSync(path,'r'); const header=Buffer.alloc(64); readSync(fd,header,0,64,0);
    const chunks=new Map(); let offset=64;
    for(let n=0;n<header.readUInt32LE(48);n++) {
      const h=Buffer.alloc(40);readSync(fd,h,0,40,offset);
      const bytes=Number(h.readBigUInt64LE(24));
      assert.equal(bytes,Number(h.readBigUInt64LE(16))); assert.equal(h.readUInt32LE(12),0);
      chunks.set([h.readUInt32LE(0),h.readUInt32LE(4),h.readUInt32LE(8)].join(','),{offset:offset+40,bytes});offset+=40+bytes;
    }
    const h=Buffer.alloc(12);readSync(fd,h,0,12,offset); const data=Buffer.alloc(Number(h.readBigUInt64LE(0)));readSync(fd,data,0,data.length,offset+12);
    return {fd,header,chunks,garden:JSON.parse(data)};
  }
  const a=fixture(process.argv[3]),b=fixture(process.argv[4]);
  assert(a.header.equals(b.header)); assert.equal(a.chunks.size,b.chunks.size);
  const hash=createHash('sha256');let bytes=0;
  for(const key of [...a.chunks.keys()].sort()) {
    const aa=a.chunks.get(key),bb=b.chunks.get(key);assert(bb);assert.equal(aa.bytes,bb.bytes);
    const x=Buffer.alloc(aa.bytes),y=Buffer.alloc(bb.bytes);readSync(a.fd,x,0,x.length,aa.offset);readSync(b.fd,y,0,y.length,bb.offset);
    assert(x.equals(y),`terrain differs in chunk ${key}`);hash.update(x);bytes+=x.length;
  }
  const roots=g=>g.flora.chunks.flatMap(c=>c.species.flatMap(s=>s.instances.map(i=>`${c.coordinate}:${s.key}:${i[0]&0xffffff}`))).sort();
  const ar=roots(a.garden),br=roots(b.garden);assert.deepEqual(ar,br);assert.deepEqual(a.garden.trees.trees,b.garden.trees.trees);
  assert.equal(br.length,511077);assert.equal(b.garden.trees.trees.length,5);
  const state={runtimeCommit:'70179a06',terrainChunks:a.chunks.size,terrainBytes:bytes,terrainBytesEqual:true,terrainSha256:hash.digest('hex'),
    floraRoots:br.length,floraRootIdentitiesEqual:true,floraRootsSha256:createHash('sha256').update(JSON.stringify(br)).digest('hex'),
    trees:b.garden.trees.trees.length,treeDescriptionsPositionsAgesAndClocksEqual:true,
    treeDataSha256:createHash('sha256').update(JSON.stringify(b.garden.trees.trees)).digest('hex'),
    note:'Terrain compared byte for byte. Root identity includes chunk, species and position, not mutable spawn age. Saves remain local; snapshots.json is the captured comparison result.'};
  writeFileSync(`${base}snapshots.json`,JSON.stringify(state,null,2)+'\n');closeSync(a.fd);closeSync(b.fd);
}
const state=JSON.parse(readFileSync(`${base}snapshots.json`)); assert(state.terrainBytesEqual&&state.floraRootIdentitiesEqual&&state.treeDescriptionsPositionsAgesAndClocksEqual);
assert.equal(state.terrainBytes,536870912);
console.log(JSON.stringify({baselineNormalMs:baselineNormal,warmMedianMs:warmMedian,speedup:result.speedup,reductionPercent:result.reductionPercent,coldMs:cold.readyMs,replay,snapshot:state},null,2));
