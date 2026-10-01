#!/usr/bin/env node
// Saved Original voxel scale: real Release draw / source rebuild, not a benchmark.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import fs from 'node:fs';
import {fileURLToPath} from 'node:url';
const help=`Usage: node scripts/validate-original-stem-voxels.mjs [--seconds <positive-number>] [--cache-review]
Hidden muted Release sweep of Original cubes at saved voxel scales .2 / 2 / 4.
Checks nominal grass-relative sizes, all six flowers, rebuilt source, resize,
clean shutdown and unchanged GUI. Default 60s; retry 90s if incomplete.
Optional --cache-review adds the strict bake oracle. Scale 4 / 32px / 256 views
currently fails it on baseline eafbb32d too; this diagnostic exits 1, not a pass.
Artifacts: target/original-stem-voxels-review/{scene.png,validation.log,summary.json}.
Exit: 0 pass/help, 1 validation failure, 2 usage error.
Example: env -u WAYLAND_DISPLAY node scripts/validate-original-stem-voxels.mjs`;
const args=process.argv.slice(2);
if(args.length===1&&['-h','--help'].includes(args[0])){console.log(help);process.exit(0);}
const cacheReview=args.includes('--cache-review');
const options=args.filter(a=>a!=='--cache-review');
const seconds=options.length===0?60:Number(options[1]);
if(args.filter(a=>a==='--cache-review').length>1||!(options.length===0||(options.length===2&&options[0]==='--seconds'))||!Number.isFinite(seconds)||seconds<=0){console.error(help);process.exit(2);}
process.chdir(fileURLToPath(new URL('../',import.meta.url)));
const dir='target/original-stem-voxels-review';fs.mkdirSync(dir,{recursive:true});
const before=fs.readFileSync('config/gui.toml');
const speciesCount=JSON.parse(fs.readFileSync('assets/models/flowers.json')).flowers.length;
try{
  fs.rmSync(`${dir}/scene.png`,{force:true});
  const env={...process.env,RE_FLORA_FLOWER_MODEL_REVIEW:'stem-original-size'};
  delete env.RE_FLORA_MODEL_CACHE_REVIEW;
  if(cacheReview)env.RE_FLORA_MODEL_CACHE_REVIEW='1';
  const run=spawnSync('cargo',['run','--release','--','--hidden','--mute','--windowed','--resize-lifecycle-test','--screenshot','player-default',`${dir}/scene.png`,'--screenshot-delay','4','--auto-exit',String(seconds)],{encoding:'utf8',maxBuffer:64*1024*1024,env});
  const text=`${run.stdout||''}\n${run.stderr||''}`;fs.writeFileSync(`${dir}/validation.log`,text);
  assert.equal(run.status,0,`${run.error||run.signal||dir+'/validation.log'}`);
  assert.doesNotMatch(text,/\bERROR\b|VUID-|panicked at|Validation Error|Validation Warning/);
  assert.match(text,/\[SHUTDOWN\] phase=complete failures=0/);
  const phases=[...text.matchAll(/\[STEM_ORIGINAL_VOXEL_PHASE\] phase=(\d+) mode=0 voxel_scale=([\d.]+) nominal_world_edge=([\d.e+-]+) saved=false/g)];
  assert.deepEqual(phases.map(p=>+p[1]),[0,1,2],'Incomplete sweep; retry --seconds 90');
  assert.deepEqual(phases.map(p=>+p[2]),[.2,2,4]);
  const results=phases.map((p,i)=>{
    const segment=text.slice(p.index,phases[i+1]?.index),species=[...new Set([...segment.matchAll(/\[FLOWER_DRAW\] species=([\w-]+)/g)].map(m=>m[1]))];
    assert.equal(species.length,speciesCount,`phase ${i} actual Original draws`);
    assert.ok(Math.abs(+p[3]-(1/256)*(+p[2])/2)<1e-9);
    assert.match(segment,/\[MODEL_CACHE_BUILD\] kind=3\b/,'saved edge change must rebuild authored source');
    if(cacheReview)assert.match(segment,/\[MODEL_CACHE_BAKE_CHECK\] kind=3 checked=\d+ mismatches=0/);
    return {phase:i,voxelScale:+p[2],nominalWorldEdge:+p[3],species};
  });
  assert.equal((text.match(/\[FLOWER_REVIEW_PLANT\]/g)||[]).length,speciesCount);
  assert.match(text,/\[RESIZE_LIFECYCLE\] phase=published/);
  assert.ok(fs.readFileSync(`${dir}/scene.png`).length>100);
  assert.deepEqual(fs.readFileSync('config/gui.toml'),before);
  fs.writeFileSync(`${dir}/summary.json`,JSON.stringify({configUnchanged:true,performanceEvidence:false,cacheReview,results},null,2)+'\n');
  console.log(`PASS: Original .2/2/4 voxel scales, ${speciesCount} species, live source rebuilds, resize, unchanged saved config.\n${dir}/summary.json`);
}catch(error){console.error(`${error.message}\nInspect ${dir}/validation.log`);process.exitCode=1;}
finally{if(!fs.readFileSync('config/gui.toml').equals(before)){console.error('GUI settings changed; inspect config/gui.toml');process.exitCode=1;}}
