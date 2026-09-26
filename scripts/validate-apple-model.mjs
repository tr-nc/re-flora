#!/usr/bin/env node
// Runtime transitions are deliberately separate from performance measurements.
import {spawnSync} from 'node:child_process';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
const help='Usage: node scripts/validate-apple-model.mjs [--seconds 12] [--stage-one] [--cache]\n--cache implies --stage-one and defaults to 16 seconds: bit-exact GPU storage/generator oracle, isolated invalidation, >128 MiB multi-block storage and replacement. Artifacts: target/model-cache-review/.\n--stage-one also cycles rotating orthographic pixels at 8/16/37/128/512 views with fixed per-object lighting and 64 rotating leaves and 21 butterflies.\nRuns hidden/muted Release: attached and fallen pixel apples at 8/32/64px, plus native resize lifecycle.\nDoes not edit saved GUI settings. Artifacts: target/apple-model-review/. Requires Vulkan/display.';
const args=process.argv.slice(2);
if(args.length===1&&['--help','-h'].includes(args[0])){console.log(help);process.exit(0);}
const cache=args.includes('--cache');if(cache)args.splice(args.indexOf('--cache'),1);
const stageFlag=args.includes('--stage-one');if(stageFlag)args.splice(args.indexOf('--stage-one'),1);
const stageOne=stageFlag||cache;
if(args.length&&(args.length!==2||args[0]!=='--seconds'||!Number.isFinite(+args[1])||+args[1]<6)){
 console.error(`Expected --seconds >= 6.\n${help}`);process.exit(2);
}
process.chdir(fileURLToPath(new URL('../',import.meta.url)));
const dir=cache?'target/model-cache-review':stageOne?'target/model-stage-one-review':'target/apple-model-review';await mkdir(dir,{recursive:true});
const before=await readFile('config/gui.toml');
const result=spawnSync('cargo',['run','--release','--','--hidden','--mute','--windowed',
 '--resize-lifecycle-test','--screenshot','player-default',`${dir}/scene.png`,
 '--screenshot-delay','5','--auto-exit',args[1]||(cache?'16':'12')],{
 encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,RE_FLORA_APPLE_MODEL_REVIEW:'resolution',...(cache?{RE_FLORA_MODEL_CACHE_REVIEW:'1'}:{}),...(stageOne?{
  RE_FLORA_MODEL_PIXEL_PREVIEW_REVIEW:'1',RE_FLORA_MODEL_PIXEL_STRESS_LEAVES:'64'}:{})}});
const text=(result.stdout||'')+(result.stderr||'');await writeFile(`${dir}/validation.log`,text);
assert.equal(result.status,0,`App failed: ${result.error||''}; inspect ${dir}/validation.log`);
assert.doesNotMatch(text,/\bERROR\b|VUID-|panicked at/);
assert.deepEqual(await readFile('config/gui.toml'),before,'Review changed saved settings');
for(const dropped of ['false','true'])for(const resolution of [8,32,64]){
 assert.match(text,new RegExp(`APPLE_PIXEL_REVIEW\\] phase=\\d+ dropped=${dropped} resolution=${resolution}`),
  'Incomplete runtime sweep; increase --seconds');
}
assert.match(text,/APPLE_PIXEL_REVIEW\] phase=10 dropped=true resolution=32/);
assert.match(text,/\[COLLISION\]\[FRUIT\] dropped tree=/,'Fixture did not exercise actual falling fruit');
assert.match(text,/\[APPLE_MODEL\] mode=pixel/);
assert.doesNotMatch(text,/\[APPLE_MODEL_AB\]/);
assert.match(text,/Application exited successfully/);
assert.match(text,/RESIZE_LIFECYCLE\] phase=requests_complete count=5/);
const resized=[...text.matchAll(/RESIZE_LIFECYCLE\] phase=frame frame_generation=(\d+) swapchain_generation=(\d+) tracer_generation=(\d+)/g)];
assert.ok(resized.length>0,'No frames after resize');
assert.ok(resized.every(([,frame,swapchain,tracer])=>frame===swapchain&&frame===tracer),'Resize published inconsistent generations');
assert.ok((await readFile(`${dir}/scene.png`)).length>100);
if(stageOne){
 for(const views of [8,16,37,128,512])assert.ok(text.includes(
  `[MODEL_PIXEL_PREVIEW] single_light=true views=${views} continuous_oracle=false orthographic=true rotating_pixels=true shared_surfaces=true`),'Missing shared surfaces, view count or permanent per-object lighting');
 assert.ok(text.includes('[MODEL_PIXEL_STRESS] leaves=64 butterflies=21'),'Missing animated particle fixture');
 for(const [leaf,butterfly] of [[8,64],[16,8],[64,16]])assert.ok(text.includes(
  `[MODEL_PIXEL_ORTHO_REVIEW] leaf_pixels=${leaf} butterfly_pixels=${butterfly}`),'Independent particle resolutions were not exercised');
 console.log('PASS: rotating orthographic pixels at 8/16/37/128/512 views with permanent per-object lighting, with rotating leaves and butterfly animation.');
}
if(cache){
 const checks=[...text.matchAll(/MODEL_CACHE_ORACLE\] leaf=(\d+) apple=(\d+) butterfly=(\d+) mismatches=(\d+)\/(\d+)\/(\d+)/g)];
 assert.ok(checks.length>0,'No GPU storage oracle results');
 for(let kind=1;kind<=3;kind++)assert.ok(checks.some(r=>+r[kind]>0),`No cached texels checked for kind ${kind-1}`);
 assert.ok(checks.every(r=>r.slice(4).every(n=>+n===0)),'Stored surface bits differ from the sole generator');
 const steps=text.split('[MODEL_CACHE_REVIEW] ').slice(1);
 assert.equal(steps.length,7,'Incomplete invalidation sweep; retry --cache --seconds 20');
 for(const [step,expected] of [[1,[]],[2,[0]],[3,[1]],[4,[2]],[5,[0,1,2]],[6,[0,1,2]]]){
  assert.match(steps[step],new RegExp(`^step=${step} `));
  const updated=[...steps[step].matchAll(/MODEL_CACHE_BUILD\] kind=(\d)/g)].map(r=>+r[1]).sort();
  assert.deepEqual(updated,expected,`Unexpected cache rebuilds in step ${step}`);
 }
 assert.match(steps[5],/MODEL_CACHE_BUILD\] kind=0 views=32 resolution=64 shapes=64 bytes=268435456 blocks=4/);
 assert.match(steps[5],/MODEL_CACHE_BUILD\] kind=2 views=32 resolution=64 shapes=32 bytes=134217728 blocks=2/);
 assert.match(steps[5],/MODEL_CACHE_ORACLE\] leaf=[1-9]\d* apple=[1-9]\d* butterfly=[1-9]\d* mismatches=0\/0\/0/);
 assert.doesNotMatch(text,/MODEL_CACHE_FALLBACK|cache_requested=/);
 assert.match(steps[6],/MODEL_CACHE_ORACLE\] leaf=[1-9]\d* apple=[1-9]\d* butterfly=[1-9]\d* mismatches=0\/0\/0/);
 console.log('PASS: bit-identical stored surfaces for all kinds; reuse, per-kind rebuilds, 256 MiB storage across four blocks and safe replacement.');
}
console.log(`PASS: attached/fallen pixel rendering and live 8/32/64px, actual fruit drops, no Vulkan errors or saved config changes.\nLog: ${dir}/validation.log\nScreenshot: ${dir}/scene.png`);
