#!/usr/bin/env node
// Check data dependencies in emitted GPU code, not merely Slang/CPU source.
import fs from 'node:fs';
import {spawnSync} from 'node:child_process';
const help=`Usage: node scripts/check-scene-depth-artifact.mjs FILE.spv [FILE.spv ...]
Checks that each scene_depth_tex write in optimized composition SPIR-V depends
on both compute_depth_tex and gfx_depth_tex. Accepts spirv-dis .spvasm output too.
Requires spirv-dis for binary input; build Release artifacts with cargo check
and cargo build --release. Find them under
 target/release/build/re-flora-vkn-*/out/precompiled-shaders/shader/tracer/
Check composition.comp.optimized.spv and composition_glass.comp.optimized.spv.
This is a dependency regression guard, not a complete SPIR-V semantic proof;
use native screenshots and Vulkan validation as well.
Exit 0: passed/help; 1: missing dependency/disassembly failure; 2: usage error.`;
const files=process.argv.slice(2);
if(files.includes('--help')){console.log(help);process.exit(0);}
if(!files.length||files.some(f=>f.startsWith('-'))){console.error(help);process.exit(2);}
try {
 for(const file of files){
  let text;
  if(file.endsWith('.spvasm'))text=fs.readFileSync(file,'utf8');
  else {const r=spawnSync('spirv-dis',[file],{encoding:'utf8'});if(r.status!==0)throw Error(`${file}: ${r.error??r.stderr}`);text=r.stdout;}
  const defs=new Map(),names=new Map(),writes=[];
  for(const line of text.split('\n')){
    const name=line.match(/OpName\s+(%\S+)\s+"([^"]+)"/);if(name)names.set(name[2],name[1]);
    const def=line.match(/^\s*(%\S+)\s*=\s*(.*)$/);if(def)defs.set(def[1],def[2].match(/%[\w.]+/g)??[]);
    const write=line.match(/OpImageWrite\s+(%\S+)\s+(%\S+)\s+(%\S+)/);if(write)writes.push({image:write[1],value:write[3]});
  }
  const reaches=(from,target)=>{const seen=new Set(),todo=[from];while(todo.length){const id=todo.pop();if(id===target)return true;if(seen.has(id))continue;seen.add(id);todo.push(...(defs.get(id)??[]));}return false;};
  const resource=n=>names.get(n)??`%${n}`;
  const selected=writes.filter(w=>reaches(w.image,resource('scene_depth_tex')));
  if(!selected.length)throw Error(`${file}: no scene_depth_tex writes found`);
  for(const w of selected)for(const source of ['compute_depth_tex','gfx_depth_tex']){
    if(!reaches(w.value,resource(source)))throw Error(`${file}: scene depth write loses ${source}; inspect optimized GPU code`);
  }
  console.log(`passed ${file}: ${selected.length} scene depth write(s) retain both producers`);
 }
}catch(error){console.error(`${error.message}\nRebuild and inspect spirv-dis output; do not accept CPU-only tests for this regression.`);process.exitCode=1;}
