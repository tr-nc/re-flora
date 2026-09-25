#!/usr/bin/env node
// Runtime transitions are deliberately separate from performance measurements.
import {spawnSync} from 'node:child_process';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
const help='Usage: node scripts/validate-apple-model.mjs [--seconds 12] [--stage-one]\n--stage-one also cycles all four lighting/view combinations with 64 rotating leaves and 21 butterflies.\nRuns hidden/muted Release: attached and fallen apple A/B at 8/32/64px, plus native resize lifecycle.\nDoes not edit saved GUI settings. Artifacts: target/apple-model-review/. Requires Vulkan/display.';
const args=process.argv.slice(2);
if(args.length===1&&['--help','-h'].includes(args[0])){console.log(help);process.exit(0);}
const stageOne=args.includes('--stage-one');if(stageOne)args.splice(args.indexOf('--stage-one'),1);
if(args.length&&(args.length!==2||args[0]!=='--seconds'||!Number.isFinite(+args[1])||+args[1]<6)){
 console.error(`Expected --seconds >= 6.\n${help}`);process.exit(2);
}
process.chdir(fileURLToPath(new URL('../',import.meta.url)));
const dir=stageOne?'target/model-stage-one-review':'target/apple-model-review';await mkdir(dir,{recursive:true});
const before=await readFile('config/gui.toml');
const result=spawnSync('cargo',['run','--release','--','--hidden','--mute','--windowed',
 '--resize-lifecycle-test','--screenshot','player-default',`${dir}/scene.png`,
 '--screenshot-delay','5','--auto-exit',args[1]||'12'],{
 encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,RE_FLORA_APPLE_MODEL_REVIEW:'ab',...(stageOne?{
  RE_FLORA_MODEL_PIXEL_PREVIEW_REVIEW:'1',RE_FLORA_MODEL_PIXEL_STRESS_LEAVES:'64'}:{})}});
const text=(result.stdout||'')+(result.stderr||'');await writeFile(`${dir}/validation.log`,text);
assert.equal(result.status,0,`App failed: ${result.error||''}; inspect ${dir}/validation.log`);
assert.doesNotMatch(text,/\bERROR\b|VUID-|panicked at/);
assert.deepEqual(await readFile('config/gui.toml'),before,'Review changed saved settings');
for(const dropped of ['false','true'])for(const resolution of [8,32,64]){
 assert.match(text,new RegExp(`APPLE_PIXEL_REVIEW\\] phase=\\d+ dropped=${dropped} enabled=true resolution=${resolution}`),
  'Incomplete runtime sweep; increase --seconds');
}
assert.match(text,/APPLE_PIXEL_REVIEW\] phase=10 dropped=true enabled=false/);
assert.match(text,/\[COLLISION\]\[FRUIT\] dropped tree=/,'Fixture did not exercise actual falling fruit');
assert.match(text,/\[APPLE_MODEL_AB\] mode=new/);
assert.match(text,/\[APPLE_MODEL_AB\] mode=original/);
assert.match(text,/Application exited successfully/);
assert.match(text,/RESIZE_LIFECYCLE\] phase=requests_complete count=5/);
const resized=[...text.matchAll(/RESIZE_LIFECYCLE\] phase=frame frame_generation=(\d+) swapchain_generation=(\d+) tracer_generation=(\d+)/g)];
assert.ok(resized.length>0,'No frames after resize');
assert.ok(resized.every(([,frame,swapchain,tracer])=>frame===swapchain&&frame===tracer),'Resize published inconsistent generations');
assert.ok((await readFile(`${dir}/scene.png`)).length>100);
if(stageOne){
 for(const light of [false,true])for(const views of [false,true])assert.ok(text.includes(
  `[MODEL_PIXEL_PREVIEW] single_light=${light} discrete_views=${views} views=128 live_tiles=true`),'Missing live preview combination');
 assert.ok(text.includes('[MODEL_PIXEL_STRESS] leaves=64 butterflies=21'),'Missing animated particle fixture');
 console.log('PASS: stage-one live toggles in all four combinations, with rotating leaves and butterfly animation.');
}
console.log(`PASS: attached/fallen runtime A/B and 8/32/64px, actual fruit drops, no Vulkan errors or saved config changes.\nLog: ${dir}/validation.log\nScreenshot: ${dir}/scene.png`);
