#!/usr/bin/env node
// Explicit native validation, not a benchmark. Reference disables only fine culling.
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
const [binaryArg,outArg]=process.argv.slice(2);
assert(binaryArg&&outArg,'node docs/evidence/grass-visible-workset/validate.mjs BINARY OUTPUT');
const binary=path.resolve(binaryArg),out=path.resolve(outArg),root=process.cwd();
const hash=b=>createHash('sha256').update(b).digest('hex');
const config=fs.readFileSync('config/gui.toml'), configHash=hash(config);
const jobs=[];
for(const pose of [1,0]) for(const scene of pose?['near','far','low','inside']:['near','far'])
 for(const enabled of [0,1]) jobs.push({name:`${scene}-pose${pose}-cull${enabled}`,scene,pose,enabled});
for(const enabled of [0,1]) {
 jobs.push({name:`growth-resize-cull${enabled}`,scene:'wide',pose:1,enabled,lifecycle:true});
 jobs.push({name:`moving-cull${enabled}`,scene:'orbit',pose:1,enabled,moving:true});
}
jobs.push({name:'mow-replant',enabled:1,edits:true});
const summary={binary,binaryHash:hash(fs.readFileSync(binary)),configHash,runs:[]};
fs.mkdirSync(out,{recursive:true});
try {
 for(const job of jobs) {
  const dir=path.join(out,job.name),runtime=path.join(out,'runtime');
  fs.mkdirSync(dir,{recursive:true});fs.rmSync(runtime,{recursive:true,force:true});fs.mkdirSync(runtime,{recursive:true});
  fs.cpSync('config',path.join(runtime,'config'),{recursive:true});fs.symlinkSync(path.join(root,'assets'),path.join(runtime,'assets'));
  const env={...process.env};for(const key of Object.keys(env))if(key.startsWith('RE_FLORA_'))delete env[key];delete env.WAYLAND_DISPLAY;
  Object.assign(env,{RUST_LOG:'info',RE_FLORA_ROOT:runtime,RE_FLORA_GRASS_VISIBILITY:String(job.enabled),RE_FLORA_GRASS_VISIBILITY_AUDIT:'1',RE_FLORA_STEM_PIXEL_PROFILE:'1'});
  const args=['--hidden','--mute','--windowed','--perf'];
  if(job.edits) {
   env.RE_FLORA_GARDEN_GRASS_AUDIT='1';args.push('--camera-snapshot','player-default','--auto-exit','2');
  } else {
   Object.assign(env,{RE_FLORA_GRASS_STEM_CAPTURE:dir,RE_FLORA_GRASS_STEM_REVIEW:`${job.scene}-both-b`,RE_FLORA_GRASS_STEM_GRID:'3',RE_FLORA_STEM_SAMPLE_FRAMES:(job.lifecycle||job.moving)?'240':'120',RE_FLORA_GRASS_BAND_POSE_REUSE:String(job.pose)});
   if(job.lifecycle)env.RE_FLORA_STEM_PIXEL_LIFECYCLE='1';
   args.push('--authored-flora-bench');
  }
  console.log('START '+job.name);
  const run=spawnSync(binary,args,{env,encoding:'utf8',maxBuffer:256*1024*1024});
  const log=(run.stdout??'')+(run.stderr??'');fs.writeFileSync(path.join(dir,'run.log'),log);
  assert.equal(run.status,0,`${job.name}: ${run.error??'native failure'}`);
  assert.doesNotMatch(log,/\bERROR\b|VUID-|SYNC-HAZARD|panicked at/);
  assert.match(log,/phase=complete failures=0/);
  const validated=[...log.matchAll(/GRASS_VISIBILITY_VALIDATED\] source=(\d+) visible=(\d+) bands=(\d+)/g)].map(m=>({source:+m[1],visible:+m[2],bands:+m[3]}));
  assert(validated.length>10,'expected checks after completed frame-slot fences');
  if(!job.enabled)assert(validated.every(v=>v.source===v.visible));
  if(job.edits)assert.match(log,/mow_replant=identical.*cross_chunk=true/);
  if(job.moving) {
   const poses=[...log.matchAll(/GRASS_VISIBILITY_CAMERA\] frame=\d+ eye=(.*)/g)].map(m=>m[1]);
   assert(new Set(poses).size>=4,'camera must really move');
   assert.match(log,/GRASS_VISIBILITY_EDIT\].*action=mow/);assert.match(log,/GRASS_VISIBILITY_EDIT\].*action=replant/);
  }
  if(job.lifecycle)for(const frame of [52,132,212])assert.match(log,new RegExp(`STEM_PIXEL_LIFECYCLE\\] frame=${frame} resize=`));
  const images=fs.readdirSync(dir).filter(n=>n.endsWith('.png')).map(name=>({name,hash:hash(fs.readFileSync(path.join(dir,name)))}));
  if(!job.edits)assert(images.length>0,'missing review captures');
  summary.runs.push({job,args,validatedFrames:validated.length,first:validated[0],last:validated.at(-1),images});
  fs.writeFileSync(path.join(out,'summary.json'),JSON.stringify(summary,null,2));
  assert.equal(hash(fs.readFileSync('config/gui.toml')),configHash,'personal config changed');
  console.log('PASS '+job.name+' '+JSON.stringify(summary.runs.at(-1).last));
 }
 fs.writeFileSync(path.join(out,'README.txt'),'Native re-flora captures; original runtime screenshots, not imported artwork or AI output. Source: this repository and the binary hash in summary.json. Generated with docs/evidence/grass-visible-workset/validate.mjs; exact jobs/arguments and image SHA256 in summary.json. Unmodified originals are retained in each named job folder. These captures retain the project assets\' existing rights; no new redistribution license is asserted. No resizing/cropping applied.\n');
} finally {fs.rmSync(path.join(out,'runtime'),{recursive:true,force:true});}
