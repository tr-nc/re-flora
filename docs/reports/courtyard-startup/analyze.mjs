// Rebuild measured evidence from the bundled Release logs; no app mutation.
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
const base = fileURLToPath(new URL('.', import.meta.url));
const sum = xs => xs.reduce((a, b) => a + b, 0);
const median = xs => { const a = [...xs].sort((a,b)=>a-b); return a.length % 2 ? a[(a.length-1)/2] : (a[a.length/2-1]+a[a.length/2])/2; };
const parse = (name) => {
  const log = gunzipSync(readFileSync(`${base}raw/${name}`)).toString();
  const lines = log.split('\n');
  const readyIndex = lines.findIndex(line => line.includes('[STARTUP] status=ready'));
  assert(readyIndex >= 0, `${name}: no ready event`);
  const startupLines = lines.slice(0, readyIndex + 1);
  const readyMs = +lines[readyIndex].match(/total_ms=([\d.]+)/)[1];
  const scopes = startupLines.flatMap(line => {
    const m = line.match(/\[STARTUP_PROBE\] id=(\d+) parent=(\d+) label=(\S+) ms=([\d.]+) thread=(\S+)/);
    return m ? [{id:+m[1],parent:+m[2],label:m[3],ms:+m[4],thread:m[5]}] : [];
  });
  const byId = new Map(scopes.map(s => [s.id,s]));
  for (const s of scopes) {
    s.childrenMs = sum(scopes.filter(c=>c.parent===s.id).map(c=>c.ms));
    s.exclusiveMs = s.ms-s.childrenMs;
    assert(s.exclusiveMs >= -0.01, `${name}: nested clock accounting ${s.label}`);
    if (s.parent) assert(byId.has(s.parent), `${name}: missing parent`);
  }
  const all = label => scopes.filter(s=>typeof label==='string'?s.label===label:label.test(s.label));
  const total = label => sum(all(label).map(s=>s.ms));
  const stages = startupLines.flatMap(line => {
    const m = line.match(/\[STARTUP\] status=done stage=(\S+) stage_ms=([\d.]+)/);
    return m ? [{stage:m[1],ms:+m[2]}] : [];
  });
  assert(Math.abs(sum(stages.map(s=>s.ms))-readyMs)<2, `${name}: startup stages do not partition ready`);
  const stage = n => stages.find(s=>s.stage===n)?.ms ?? 0;
  const populationMs = stage('world_population_and_lighting');
  const admissionMs = total('plant.space_clear');
  const surfacesMs = total(/^surface.prepare.forest/);
  const trunkMs = total('tree.trunk_transaction');
  const compileMs = total('tree.compile_placement');
  const leavesMs = total('tree.publish_leaves');
  const beforePopulationMs = stage('terrain_and_model_caches');
  const initMs = readyMs-populationMs-beforePopulationMs-stage('first_frame');
  const partition = scopes.length ? [
    {key:'admission',name:'种植障碍检查',ms:admissionMs},
    {key:'terrain',name:'地形、模型预热与初始碰撞体',ms:beforePopulationMs},
    {key:'surfaces',name:'五次树木表面准备',ms:surfacesMs},
    {key:'init',name:'窗口、Vulkan、渲染器等初始化',ms:initMs},
    {key:'first_frame',name:'首帧准备与等待',ms:stage('first_frame')},
    {key:'trunks',name:'树干事务（地形写入及可见发布）',ms:trunkMs},
    {key:'placement_compile',name:'树形与放置数据编译',ms:compileMs},
    {key:'leaves',name:'叶片发布',ms:leavesMs},
    {key:'other_population',name:'其它庭院填充、音频、提交与准备',ms:populationMs-admissionMs-surfacesMs-trunkMs-compileMs-leavesMs}
  ].sort((a,b)=>b.ms-a.ms) : [];
  if (partition.length) assert(Math.abs(sum(partition.map(s=>s.ms))-readyMs)<1e-6);
  const admission = all('plant.space_clear').map(s=>({
    ms:s.ms,generateMs:sum(scopes.filter(c=>c.parent===s.id&&c.label.startsWith('tree.generate')).map(c=>c.ms)),
    readMs:sum(scopes.filter(c=>c.parent===s.id&&c.label==='atlas.read').map(c=>c.ms)),
    cpuScanAndDistanceMs:s.exclusiveMs,
  }));
  const summary = [...new Set(scopes.map(s=>s.label))].map(label=>({label,count:all(label).length,inclusiveMs:total(label),exclusiveMs:sum(all(label).map(s=>s.exclusiveMs))})).sort((a,b)=>b.exclusiveMs-a.exclusiveMs);
  const forest = all(/^surface.prepare.forest/).map(s=>({label:s.label,ms:s.ms,components:scopes.filter(c=>c.parent===s.id).map(c=>({label:c.label,ms:c.ms})),publication:scopes.filter(c=>c.parent===s.id&&c.label==='surface.prepare_gpu_publication').flatMap(c=>scopes.filter(d=>d.parent===c.id).map(d=>({label:d.label,ms:d.ms})))}));
  const counters = startupLines.filter(l=>l.includes('[STARTUP_PROBE_COUNTS]')).map(l=>l.split('[STARTUP_PROBE_COUNTS] ')[1]);
  const cache = startupLines.flatMap(l=>{const m=l.match(/\[TREE\]\[COMPILE_CACHE\] reused=(\d+) rebuilt=(\d+)/);return m?[{reused:+m[1],rebuilt:+m[2]}]:[];});
  const replay = startupLines.filter(l=>l.includes('[STARTUP_PROBE_REPLAY]')).map(l=>l.split('[STARTUP_PROBE_REPLAY] ')[1]);
  return {name,readyMs,pipelineCache:log.match(/\[PIPELINE_CACHE\] status=(hit|miss)/)?.[1]??'unknown',userSeconds:+(log.match(/User time \(seconds\): ([\d.]+)/)?.[1]??0),systemSeconds:+(log.match(/System time \(seconds\): ([\d.]+)/)?.[1]??0),stages,partition,admission,forest,cache,counters,replay,summary};
};
const runs = readdirSync(`${base}raw`).filter(n=>n.endsWith('.log.gz')).map(parse);
const warm = runs.filter(r=>['profile-3.log.gz','profile-4.log.gz'].includes(r.name));
assert.equal(warm.length,2);
assert(warm.every(r=>r.pipelineCache==='hit' && r.readyMs>30000));
assert(warm.every(r=>r.forest.length===5));
assert(warm.every(r=>r.counters.some(c=>c.includes('distance_tests=1500328596'))));
assert(warm.every(r=>r.cache.reduce((n,c)=>n+c.reused,0)===9));
for (const name of ['replay-default.log.gz','replay-seed7.log.gz']) {
  const r = runs.find(r=>r.name===name);
  assert(r && r.replay.some(s=>s.includes('boundary_fixtures=4296 mismatches=0')));
  assert(r.replay.every(s=>s.endsWith('mismatches=0')));
}
const clean = runs.find(r=>r.name==='clean-2.log.gz');
assert(clean && clean.pipelineCache==='hit' && clean.summary.length===0);
const pooled = warm[0].partition.map(s=>({key:s.key,name:s.name,ms:median(warm.map(r=>r.partition.find(p=>p.key===s.key).ms))})).sort((a,b)=>b.ms-a.ms);
const result={schema:1,baseCommit:'4ce0dbcc470de2c10681b8457927ed0b22c92d1a',units:'milliseconds unless named otherwise',method:'warm-repeat median; disjoint top-level partition, nested scope wall time; not GPU timestamps',warmReadyMedianMs:median(warm.map(r=>r.readyMs)),warmPartitionMedian:pooled,runs};
writeFileSync(`${base}measurements.json`,JSON.stringify(result,null,2)+'\n');
console.log(`warm ready median ${(result.warmReadyMedianMs/1000).toFixed(3)} s`);
for(const p of pooled) console.log(`${(p.ms/1000).toFixed(4).padStart(8)} s  ${p.name}`);
for(const r of warm){console.log('\n'+r.name);console.log(r.admission[0]);for(const key of ['loading.step.terrain','loading.step.building','loading.step.colliders','terrain.collider_import','terrain.physical_advance','models.warmup_record_commands','courtyard.initialize_walls_and_soil','tree.generate.Ginkgo.Veteran','tree.canopy_descriptor','surface.scene_bvh','surface.physics_collision','surface.gpu_upload','surface.mesh_bind'])console.log(r.summary.find(s=>s.label===key));}
console.log(`checked ${runs.length} logs; nested accounting, totals, admission counts, pipeline state and forest publication counts passed`);
