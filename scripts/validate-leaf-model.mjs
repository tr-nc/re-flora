#!/usr/bin/env node
// Explicit Vulkan check; not part of fast Cargo tests. No saved setting edits.
import {spawnSync} from 'node:child_process';
import {readFile,mkdir,rm,readdir} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
const help='Usage: node scripts/validate-leaf-model.mjs [--seconds 10]\nRuns a hidden, muted Release fixture through A -> B at 8/16/64px -> A -> B.\nRequires a Vulkan-capable desktop session. Artifacts: target/leaf-model-review/.\nIncrease --seconds if the live sweep is incomplete. Does not save Debug settings.';
const args=process.argv.slice(2);
if(args.length===1&&['--help','-h'].includes(args[0])){console.log(help);}else if(args.length&&(args.length!==2||args[0]!=='--seconds'||!Number.isFinite(+args[1])||+args[1]<=0)){
  console.error(`Expected a positive --seconds value.\n${help}`);process.exitCode=2;
}else{
 const root=fileURLToPath(new URL('../',import.meta.url));process.chdir(root);
 const seconds=Number(args[1]||10),output='target/leaf-model-review',log=path.join(output,'validation.log');
 await mkdir(output,{recursive:true});await rm(path.join(output,'tiles'),{recursive:true,force:true});
 const before=await readFile('config/gui.toml');
 const result=spawnSync('cargo',['run','--release','--','--hidden','--mute','--screenshot','player-default',path.join(output,'scene.png'),'--screenshot-delay','1.5','--auto-exit',String(seconds)],{encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,RE_FLORA_FALLEN_LEAF_REVIEW:'fixture',RE_FLORA_LEAF_MODEL_REVIEW:'ab'}});
 const text=(result.stdout||'')+(result.stderr||'');
 const {writeFile}=await import('node:fs/promises');await writeFile(log,text);
 assert.equal(result.status,0,`App/build failed: ${result.error||''}; inspect ${log}`);
 assert.doesNotMatch(text,/\bERROR\b|VUID-|panicked at/,`inspect ${log}`);
 assert.deepEqual(await readFile('config/gui.toml'),before,'Review changed saved settings');
 assert.ok(text.includes('failures=0'));
 const repairs=Array.from(text.matchAll(/MODEL-REPAIR-CHECK\] original_samples=(\d+) original_changed=0 added=(\d+) planned=\d+ components=(\d+)->(\d+)/g),m=>m.slice(1).map(Number));
 assert.ok(repairs.length>=8,'Missing rotating-pose repair checks');
 assert.ok(repairs.some(([samples,added,before,after])=>samples>0&&added>0&&after<before),'No actual GPU gap was repaired');
 // Conservatively visible parts may introduce a component absent from center sampling.
 const modes=Array.from(text.matchAll(/LEAF-MODEL\] mode=([AB]) pixels=(\d+)x/g),m=>`${m[1]}${m[2]}`);
 assert.deepEqual(modes.slice(0,6),['A16','B8','B16','B64','A16','B16'],`Incomplete live sweep; rerun with a larger --seconds. ${log}`);
 for(const [mode,scale] of [['A','2'],['B','0.25'],['B','4']])assert.match(text,new RegExp(`LEAF-MODEL\\] mode=${mode}[^\\n]* render_scale=${scale.replace('.','\\.')}\\b`),`Missing ${mode} size ${scale}; increase --seconds`);
 const files=await readdir(path.join(output,'tiles'));
 for(const n of [8,16,64]){
   assert.match(text,new RegExp(`LEAF-MODEL-CHECK\\] mode=B resolution=${n} active=8 checked_hits=[1-9]\\d*`));
   const images=files.filter(name=>name.startsWith(`${n}px-`));assert.equal(images.length,8);
   for(const image of images){const data=await readFile(path.join(output,'tiles',image));assert.equal(data.readUInt32BE(16),n);assert.equal(data.readUInt32BE(20),n);}
 }
 assert.ok((await readFile(path.join(output,'scene.png'))).length>100);
 console.log(`PASS: live A/B, 8/16/64px, rotating published flight poses, conservative coverage and eight-neighbour repair, original RGBA/depth unchanged, model-shaded seeds and GPU/CPU depths, no Vulkan errors or saved-setting changes.\nLog: ${log}\nScreenshot: ${output}/scene.png`);
}
