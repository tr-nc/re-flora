// Opt-in end-to-end review through agent-browser, not a unit/native-GPU/performance test.
// Own ephemeral HTTP server and PID-scoped browser session; always release both.
import {spawn,execFileSync} from 'node:child_process';
import {writeFileSync,mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';

const repo=fileURLToPath(new URL('../../../',import.meta.url));
process.chdir(repo);
const artifacts=resolve(process.env.PREVIEW_ARTIFACT_DIR??'target/soil-bag-preview-review');
mkdirSync(artifacts,{recursive:true});
writeFileSync(resolve(artifacts,'invalid-image.txt'),'Not an image; local error-recovery fixture.\n');
const session=execFileSync('agent-browser',['session','id','--scope','worktree','--prefix',`soil-bag-test-${process.pid}`],{encoding:'utf8'}).trim();
const browserEnv={...process.env,AGENT_BROWSER_SESSION:session};
const server=spawn(process.execPath,['--input-type=module','-e',`
  import {createPreviewServer} from './scripts/serve-model-preview.mjs';
  const server=createPreviewServer();
  server.listen(0,'127.0.0.1',()=>console.log(server.address().port));
`],{cwd:repo,stdio:['ignore','pipe','inherit']});
try{
  const port=await new Promise((resolve,reject)=>{
    server.once('error',reject);
    server.once('exit',code=>reject(new Error(`preview server exited ${code}`)));
    server.stdout.once('data',data=>resolve(Number(String(data).trim())));
  });
  assert.ok(port>0);
  const origin=`http://127.0.0.1:${port}`;
const run=(...args)=>{const response=JSON.parse(execFileSync('agent-browser',['--json',...args],{encoding:'utf8',maxBuffer:16*1024*1024,env:browserEnv}));assert.equal(response.success,true,JSON.stringify(response));return response.data;};
const evaluate=script=>run('eval',script).result;
const wait=script=>run('wait','--fn',script);
const settle=()=>wait('readModelPreview().ready && !readModelPreview().dirty && !readModelPreview().loading');
const input=(id,value)=>{evaluate(`(()=>{const e=document.getElementById(${JSON.stringify(id)});e.value=${JSON.stringify(String(value))};e.dispatchEvent(new Event('input',{bubbles:true}));})()`);settle();};
const inspect=()=>evaluate(`(()=>{
 const p=readModelPreview(true),c=document.getElementById('pixel'),gl=c.getContext('webgl2'),bytes=new Uint8Array(c.width*c.height*4);gl.readPixels(0,0,c.width,c.height,gl.RGBA,gl.UNSIGNED_BYTE,bytes);
 let hash=2166136261,alpha=0,preserved=true,originalSamples=0;
 for(let i=0;i<bytes.length;i++){hash=Math.imul(hash^bytes[i],16777619)>>>0;if(i%4===3&&bytes[i])alpha++;if(p.originalRgba?.[i-i%4+3]){if(i%4===3)originalSamples++;if(bytes[i]!==p.originalRgba[i])preserved=false;}}
 return {model:p.model,settings:p.modelSettings,review:p.modelReview,time:p.time,pixelTime:p.pixelTime,sourceTime:p.sourceTime,playing:p.playing,failed:p.failed,size:p.pixelBuffer,linked:p.linkedRotation,memory:p.rendererMemory,parts:p.pixelPartCount,alpha,hash,originalSamples,preserved,label:document.querySelector('#model-info').textContent,overflow:document.documentElement.scrollWidth>innerWidth};
})()`);
const samples=[],checks=[];
const record=name=>{const value=inspect();assert.equal(value.failed,false);assert.equal(value.linked,true);assert.equal(value.sourceTime,value.pixelTime);assert.ok(value.alpha>0);if(value.parts===0){assert.equal(value.preserved,true,'shared pipeline must preserve every original sample');assert.ok(value.alpha<value.size[0]*value.size[1]);}samples.push({name,...value});return value;};
run('set','viewport','1440','1000');run('open',origin+'/model-preview/?model=soil-bag');settle();
run('click','#edge');settle();const roundSide=record('side-round');assert.equal(roundSide.settings.sideFlatness,0);
assert.deepEqual(evaluate('(()=>{const s=document.querySelector("#model-sideFlatness");return [s.min,s.max,s.step];})()'),['0','1','0.01']);
run('scroll','up','3000');run('screenshot',resolve(artifacts,'soil-bag-preview-side-round.png'));
input('model-sideFlatness',1);const flatSide=record('side-flat');assert.equal(flatSide.settings.sideFlatness,1);assert.notEqual(flatSide.hash,roundSide.hash);assert.deepEqual(flatSide.memory,roundSide.memory);
run('scroll','up','3000');run('screenshot',resolve(artifacts,'soil-bag-preview-side-flat.png'));
input('model-sideFlatness',0);assert.equal(record('side-round-restored').hash,roundSide.hash);run('click','#reset-view');settle();checks.push('declarative side-flatness slider changes real side geometry and pixel result, returns exactly to the original, and keeps resource counts');
const png=evaluate('(()=>{const c=document.createElement("canvas");c.width=240;c.height=320;const x=c.getContext("2d");x.fillStyle="#315ca4";x.beginPath();x.arc(120,130,80,0,Math.PI*2);x.fill();x.fillStyle="#fff";x.font="bold 56px sans-serif";x.textAlign="center";x.fillText("SOIL",120,151);x.fillStyle="#db7844";x.fillRect(50,238,140,22);return c.toDataURL("image/png");})()');
writeFileSync(resolve(artifacts,'upload-sticker.png'),Buffer.from(png.split(',')[1],'base64'));
run('click','button[data-preset="花园配方"]');settle();run('click','#model-bagColor button');const flower=record('flower-packaging');assert.equal(flower.settings.stickerDesign,'flower');
run('fill','#model-bagColor .hex-input','#58809c');run('click','button[aria-label="关闭颜色选择器"]');settle();const blue=record('bag-color-edit');assert.equal(blue.settings.bagColor,'#58809c');assert.notEqual(blue.hash,flower.hash);checks.push('actual HEX picker changes bag and pixel result');
run('select','#model-stickerDesign','layers');settle();const layers=record('builtin-sticker-swap');assert.notEqual(layers.hash,blue.hash);
run('upload','#model-customSticker',resolve(artifacts,'upload-sticker.png'));wait('readModelPreview().modelReview.customSticker && readModelPreview().modelSettings.stickerDesign==="custom" && !readModelPreview().dirty');const imported=record('uploaded-alpha-sticker');assert.notEqual(imported.hash,layers.hash);assert.equal(imported.alpha,layers.alpha);checks.push('custom PNG import changes label; alpha is flattened without changing pouch coverage');
run('scroll','up','3000');run('screenshot',resolve(artifacts,'soil-bag-preview-custom-sticker.png'));
run('click','button[data-preset="育苗基质"]');settle();const seedling=record('seedling-preset');assert.equal(seedling.settings.stickerDesign,'layers');assert.equal(seedling.settings.bagColor,'#bd956d');assert.equal(evaluate('document.querySelector("#model-fullness-value").textContent'),'0.85');checks.push('whole packaging preset updates saved-in-memory values and every displayed control');
run('upload','#model-customSticker',resolve(artifacts,'invalid-image.txt'));wait('document.querySelector("#status").classList.contains("error")');assert.equal(inspect().failed,false);assert.equal(evaluate('document.querySelector("#model-customSticker").disabled'),false);
run('upload','#model-customSticker',resolve(artifacts,'upload-sticker.png'));wait('!document.querySelector("#status").classList.contains("error") && readModelPreview().modelSettings.stickerDesign==="custom" && !readModelPreview().dirty');checks.push('bad image reports error, re-enables upload, and a valid retry recovers');
run('click','button[data-preset="林下营养土"]');settle();run('click','#play');wait('!readModelPreview().playing && readModelPreview().pixelTime===1.65 && !readModelPreview().dirty');const torn=record('tear-held-end');assert.equal(torn.review.openingCount,1);assert.equal(torn.review.flowVisible,false);assert.equal(torn.label,'已撕开 · 单一袋口');run('scroll','up','3000');run('screenshot',resolve(artifacts,'soil-bag-preview-opened.png'));
input('fps',7);run('click','#previous-frame');settle();assert.equal(inspect().pixelTime,11/7);run('click','#next-frame');settle();assert.equal(inspect().pixelTime,1.65);run('click','#next-frame');settle();assert.equal(inspect().pixelTime,1.65);checks.push('one-shot tear holds exact end; odd FPS previous/next does not skip or wrap');
run('click','#play');wait('readModelPreview().playing && readModelPreview().time<1');run('click','#play');settle();assert.ok(inspect().time<1);input('fps',60);
run('select','#clip','1');settle();const initialPour=record('pour-start-open');assert.equal(initialPour.review.openingCount,1);assert.equal(initialPour.review.flowVisible,false);
input('phase',1900);const pour=record('pour-gravity');assert.ok(pour.review.mouthNormal[1]<-.7);assert.equal(pour.review.flowVisible,true);assert.equal(pour.label,'袋口朝下 · 简化土流');run('scroll','up','3000');run('screenshot',resolve(artifacts,'soil-bag-preview-pouring.png'));
input('model-sideFlatness',1);const flatPour=record('side-flat-pouring');assert.equal(flatPour.review.flowVisible,true);assert.deepEqual(flatPour.review.mouthWorld,pour.review.mouthWorld);assert.deepEqual(flatPour.review.mouthNormal,pour.review.mouthNormal);input('model-sideFlatness',0);
input('phase',3800);const end=record('pour-held-end');assert.equal(end.review.openingCount,1);assert.equal(end.review.flowVisible,false);checks.push('pour starts opened; middle mouth points down and flows; final pose keeps opening and stops flow');
input('phase',1900);run('uncheck','#conservative-coverage');settle();const old=record('existing-shared-bridge-mode');run('check','#conservative-coverage');settle();const covered=record('existing-shared-coverage-mode');assert.ok(covered.alpha>=old.alpha);
for(const n of [16,32,64,128]){input('resolution',n);assert.deepEqual(record(`resolution-${n}`).size,[n,n]);}
run('select','#projection','perspective');settle();record('perspective');run('select','#projection','orthographic');run('click','#back');settle();record('back');run('click','#edge');settle();record('edge');run('click','#reset-view');settle();checks.push('both shared processing modes, four resolutions, projections and linked views render correctly');
for(let cycle=0;cycle<3;cycle++){
 run('select','#model','apple');wait('readModelPreview().model==="apple" && !readModelPreview().loading && !readModelPreview().dirty');const apple=record(`apple-${cycle}`);assert.equal(apple.parts,0);assert.equal(apple.playing,false);
 run('select','#model','butterfly');wait('readModelPreview().model==="butterfly" && !readModelPreview().loading && !readModelPreview().dirty');const butterfly=record(`butterfly-${cycle}`);assert.ok(evaluate('readModelPreview().clips.length')>0);input('phase',999);run('click','#next-frame');settle();assert.equal(inspect().pixelTime,0);
 run('select','#model','gillenia');wait('readModelPreview().model==="gillenia" && !readModelPreview().loading && !readModelPreview().dirty');assert.ok(record(`flower-${cycle}`).parts>0);
 run('select','#model','soil-bag');wait('readModelPreview().model==="soil-bag" && !readModelPreview().loading && !readModelPreview().dirty');const pouch=record(`reload-pouch-${cycle}`);assert.equal(pouch.review.customSticker,false);assert.deepEqual(pouch.memory,samples.find(s=>s.name==='reload-pouch-0')?.memory??pouch.memory);
}
checks.push('three apple/butterfly/flower/pouch cycles keep existing behavior and stable geometry/texture counts');
run('set','viewport','390','844');settle();const mobile=record('mobile');assert.equal(mobile.overflow,false);run('scroll','up','3000');run('screenshot','--full',resolve(artifacts,'soil-bag-preview-mobile.png'));
run('set','viewport','1440','1000');settle();const desktop=record('desktop');assert.equal(desktop.overflow,false);run('scroll','up','3000');run('screenshot',resolve(artifacts,'soil-bag-preview-closed.png'));checks.push('390px mobile and 1440px desktop have no horizontal overflow');
const errors=run('errors');assert.deepEqual(errors.errors??[],[],JSON.stringify(errors));
writeFileSync(resolve(artifacts,'soil-bag-preview-qa.json'),JSON.stringify({passed:true,checks,samples,errors},null,2));console.log(JSON.stringify({passed:true,checks,samples:samples.length},null,2));
}catch(error){
  try{
    console.error(execFileSync('agent-browser',['--json','eval','({url:location.href,status:document.querySelector("#status")?.textContent,preview:window.readModelPreview?.()})'],{env:browserEnv,encoding:'utf8'}));
    console.error(execFileSync('agent-browser',['--json','errors'],{env:browserEnv,encoding:'utf8'}));
    console.error(execFileSync('agent-browser',['--json','console'],{env:browserEnv,encoding:'utf8'}));
  }catch{/* Preserve the original failure even if the diagnostic page is unavailable. */}
  throw error;
}finally{
  try{execFileSync('agent-browser',['close'],{env:browserEnv,stdio:'ignore'});}finally{server.kill('SIGTERM');}
}
