#!/usr/bin/env node
// Diagnostic silhouettes for the grass-only fixtures; not a general image-equivalence oracle.
import fs from 'node:fs';import path from 'node:path';import assert from 'node:assert/strict';import {spawnSync} from 'node:child_process';
const dir=path.resolve(process.argv[2]??'target/grass-workset/accepted-native');
const summary=JSON.parse(fs.readFileSync(path.join(dir,'summary.json'))),comparisons=[];
const maskDir=path.join(dir,'masks');fs.mkdirSync(maskDir,{recursive:true});
for(const before of summary.runs.filter(r=>r.job.enabled===0)) {
 const after=summary.runs.find(r=>r.job.name===before.job.name.replace('cull0','cull1'));assert(after);
 for(const image of before.images) {
  assert(after.images.some(i=>i.name===image.name));
  const masks=[];
  for(const run of [before,after]) {
   const file=path.join(dir,run.job.name,image.name),mask=path.join(maskDir,run.job.name+'-'+image.name);
   const p=spawnSync('magick',[file,'-fx','g>r && g>b ? 1:0',mask],{encoding:'utf8'});assert.equal(p.status,0,p.stderr);masks.push(mask);
  }
  const p=spawnSync('magick',['compare','-metric','AE',...masks,'null:'],{encoding:'utf8'});assert([0,1].includes(p.status),p.stderr);
  comparisons.push({before:before.job.name,after:after.job.name,image:image.name,identicalGrassSilhouette:p.status===0,metric:p.stderr.trim()});
 }
}
const result={classification:'green > red AND green > blue; diagnostic grass-only silhouette; does not establish identical lighting/depth values',comparisons};
fs.writeFileSync(path.join(dir,'silhouettes.json'),JSON.stringify(result,null,2));
console.log(JSON.stringify(result,null,2));
if(comparisons.some(c=>!c.identicalGrassSilhouette))process.exitCode=1;
