#!/usr/bin/env node
// Real Release A/B; same scene, all water/droplets/audio retained. No visual-approval claim.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync,mkdirSync,readdirSync} from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {metrics,sceneDimensions,options as commonOptions} from './benchmark-sprinkler-optics.mjs';

const help = `Usage: node scripts/benchmark-sprinkler-air.mjs [options]
Compare original mist with the APPROXIMATE volumetric-air renderer, not a lossless optimization.
Hidden, muted Release runs; real spray, droplets, irrigation and spatial audio stay active.
Alternates original/air order across repeats. Never saves GUI, camera or terrain.
Includes field clear/build, shared god rays, original god rays, temporal pass and final
mist/droplet compositing in one PER-FRAME optical.atmosphere sum. Reports actual cadence.

--binary PATH   Release + no_validation_layer binary (default target/release/re-flora)
--snapshot PATH Loaded garden (default target/garden-delivery/world/implementation/garden-final.rflterrain)
--output PATH   NEW/empty evidence directory (default target/sprinkler-air-review/measurement)
--counts LIST   Distinct values from 1,4,8,16 (default 1,16)
--layout MODE   spread|overlap (default spread)
--duty N        30|100 percent (default 30)
--seconds N     App seconds per run, finite >=20 (default 22)
--repeats N     Integer >=3 (default 3)
--help, -h      Show help without launching
Exit: 0 success/help, 1 runtime/evidence failure, 2 invalid arguments.

Requires Vulkan desktop access and the realized garden fixture. Records and enforces
actual monitor/internal dimensions, binary/input hashes, source publication and warmed
samples. Raw logs and warnings remain in the evidence; no slower repetition is deleted.
Example (serialize builds/apps and restore ordinary Release):
flock -o /tmp/re-flora-garden-delivery-heavy.lock bash -c 'cargo build --release --features no_validation_layer && node scripts/benchmark-sprinkler-air.mjs --counts 16 --output target/sprinkler-air-review/NEW; cargo build --release'
Failure recovery: inspect saved logs, fix the cause, then rerun with a NEW --output.
`;

export const AIR_TOTAL_SCOPES = ['sprinkler_mist.pass','sprinkler_air.field','god_ray.air','god_ray.pass','god_ray_temporal.pass'];
export function options(argv) {
  return commonOptions(['--counts','1,16','--output','target/sprinkler-air-review/measurement',...argv]);
}
const hash = file => createHash('sha256').update(readFileSync(file)).digest('hex');
const mean = values => values.reduce((a,b)=>a+b,0)/values.length;

function run(o) {
  const files=['config/gui.toml','config/camera_snapshots.toml',o.snapshot];
  const hashes=()=>Object.fromEntries(files.map(file=>[file,hash(file)]));
  const inputs=hashes(),binaryHash=hash(o.binary),results=[];
  mkdirSync(o.output,{recursive:true});
  assert.equal(readdirSync(o.output).length,0,'Evidence directory must be NEW/empty');
  let scene;
  const save = () => {
    const analysis = [...new Set(results.map(r=>r.count))].map(count=>{
      const rows=results.filter(r=>r.count===count);
      const complete=[...new Set(rows.map(r=>r.repeat))].filter(r=>rows.filter(row=>row.repeat===r).length===2);
      return {count,complete_repeats:complete.length,modes:Object.fromEntries(['original','air'].map(mode=>{
        const runs=rows.filter(r=>r.mode===mode&&complete.includes(r.repeat));
        if(!runs.length)return [mode,null];
        const keys=[...new Set(runs.flatMap(r=>Object.keys(r.gpu.scopes)))];
        return [mode,{gpu:Object.fromEntries(keys.map(k=>[k,Object.fromEntries(['mean','p50','p90'].map(stat=>[stat,mean(runs.map(r=>r.gpu.scopes[k]?.[stat]??0))]))])),
          actual_fps:mean(runs.map(r=>r.cpu.cadence_fps)),samples:runs.map(r=>r.gpu.samples)}];
      }))};
    });
    writeFileSync(path.join(o.output,'summary.json'),JSON.stringify({options:o,inputs,binary_sha256:binaryHash,scene,results,analysis},null,2)+'\n');
  };
  for(let repeat=0;repeat<+o.repeats;repeat++) {
    const counts=o.counts.split(',').map(Number);
    if(repeat%2)counts.reverse();
    for(const count of counts)for(const mode of repeat%2?['air','original']:['original','air']) {
      const name=`${o.layout}-d${o.duty}-n${count}-r${repeat}-${mode}`;
      assert.deepEqual(hashes(),inputs);assert.equal(hash(o.binary),binaryHash);
      const env={...process.env};
      for(const k of Object.keys(env))if(k.startsWith('RE_FLORA_'))delete env[k];
      Object.assign(env,{RUST_LOG:'info',RE_FLORA_GARDEN_PERF:'1',RE_FLORA_SPRINKLER_REVIEW:'rotary-perf',
        RE_FLORA_SPRINKLER_PERF:`${count},${o.layout},${o.duty}`,RE_FLORA_SPRINKLER_AIR_REVIEW:mode});
      delete env.VK_INSTANCE_LAYERS;delete env.VK_LAYER_ENABLES;delete env.VK_LOADER_LAYERS_DISABLE;
      if(env.DISPLAY){delete env.WAYLAND_DISPLAY;delete env.WAYLAND_SOCKET;}
      const child=spawnSync(path.resolve(o.binary),['--hidden','--mute','--perf','--terrain-load',o.snapshot,
        '--camera-snapshot','garden-arrival','--auto-exit',o.seconds],{env,encoding:'utf8',maxBuffer:128*1024*1024});
      const text=(child.stdout??'')+(child.stderr??'');
      writeFileSync(path.join(o.output,`${name}.log`),text);
      assert.equal(child.status,0,`${name} exited ${child.status}: ${child.error??''}`);
      assert.ok(!/\bERROR\b|VUID-|SYNC-HAZARD|panicked at|Insert instance layer.*KHRONOS_validation/.test(text),`${name}: inspect saved errors/validation`);
      const dimensions=sceneDimensions(text);scene??=dimensions;assert.deepEqual(dimensions,scene);
      assert.match(text,/phase=complete failures=0/);
      assert.ok(text.includes(`[SPRINKLER_AIR_REVIEW] mode=${mode} saved=false water=unchanged droplets=unchanged`));
      assert.ok(text.includes(`[SPRINKLER_PERF_PUBLICATION] instances=${count} all_instances_live=true`));
      const gpu=metrics(text,'GPU',{'optical.atmosphere':AIR_TOTAL_SCOPES});
      assert.equal('sprinkler_air.field' in gpu.scopes,mode==='air');
      assert.equal('god_ray.air' in gpu.scopes,mode==='air');
      assert.equal('god_ray.pass' in gpu.scopes,mode==='original');
      results.push({name,count,mode,repeat,gpu,cpu:metrics(text,'CPU'),
        warnings:text.split('\n').filter(line=>/\bWARN\b/.test(line))});
      assert.deepEqual(hashes(),inputs);assert.equal(hash(o.binary),binaryHash);
      if(results.at(-1).warnings.length)console.warn(`${name}: ${results.at(-1).warnings.length} warnings retained; inspect the log before acceptance`);
      save();console.log(`${name}: optical+atmosphere=${gpu.scopes['optical.atmosphere'].mean.toFixed(3)}ms GPU=${gpu.scopes['frame.render'].mean.toFixed(3)}ms`);
    }
  }
}

if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  let o;
  try { o=options(process.argv.slice(2)); }
  catch(e){console.error(`${e.message}\n${help}`);process.exit(2);}
  if(o.help){console.log(help);process.exit(0);}
  try {run(o);}
  catch(e){console.error(`${e.message}\nInspect ${o.output}/*.log; fix the cause and rerun with a NEW --output.\n${help}`);process.exitCode=1;}
}
