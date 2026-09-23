// Optional WebGL integration checks, outside cargo test. Needs Playwright on
// NODE_PATH. Uses a private static server and headless Chrome, never user tabs.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const root=path.resolve(__dirname,'../..');
const artifacts=process.env.PREVIEW_ARTIFACT_DIR||path.resolve(root,'../target/model-preview-validation');
let server;
(async()=>{
 const {createPreviewServer}=await import('../../../scripts/serve-model-preview.mjs');
 server=createPreviewServer();
 await fs.mkdir(artifacts,{recursive:true});await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const base=`http://127.0.0.1:${server.address().port}`;
 const browser=await chromium.launch({executablePath:process.env.CHROME_EXECUTABLE||'/usr/bin/google-chrome',headless:true,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],failed=[],urls=[];
  page.on('pageerror',error=>errors.push(error.message));page.on('console',message=>{if(message.type()==='error')errors.push(message.text())});
  page.on('request',request=>urls.push(request.url()));page.on('requestfailed',request=>failed.push(`${request.url()}: ${request.failure()?.errorText}`));page.on('response',response=>{if(response.status()>=400)failed.push(response.url())});
  const stable=()=>page.waitForFunction(()=>readModelPreview().ready&&!readModelPreview().loading&&!readModelPreview().dirty&&!readModelPreview().failed);
  const state=()=>page.evaluate(()=>readModelPreview());
  const png=()=>page.locator('#pixel').evaluate(c=>c.toDataURL());
  const rgba=()=>page.evaluate(async()=>{
    const image=await createImageBitmap(document.querySelector('#pixel')),canvas=new OffscreenCanvas(image.width,image.height),ctx=canvas.getContext('2d');ctx.drawImage(image,0,0);image.close();return Array.from(ctx.getImageData(0,0,canvas.width,canvas.height).data);
  });
  async function select(model){await page.locator('#model').selectOption(model);await page.waitForFunction(id=>readModelPreview().model===id&&!readModelPreview().loading&&!readModelPreview().dirty,model);}
  async function repairCheck(){
    await page.locator('#repair').uncheck();await stable();const original=await rgba();
    await page.locator('#repair').check();await stable();const result=await rgba();let added=0;
    for(let i=0;i<original.length;i+=4){if(original[i+3])assert.deepEqual(result.slice(i,i+4),original.slice(i,i+4));else if(result[i+3])added++;}
    assert.equal(added,(await state()).repair.added);
    return added;
  }
  await page.goto(base+'/model-preview/');await stable();
  assert.equal((await state()).triangles,32);assert.equal(await page.locator('#play').isDisabled(),true);
  assert.equal(await repairCheck(),1);
  for(const n of [8,12,16,22,32,64,128]){
    await page.locator('#resolution').fill(String(n));await stable();assert.deepEqual((await state()).pixelBuffer,[n,n]);await repairCheck();
  }
  for(const id of ['source','pixel']){
    const before=(await state()).camera,b=await page.locator('#'+id).boundingBox();
    await page.mouse.move(b.x+b.width/2,b.y+b.height/2);await page.mouse.down();await page.mouse.move(b.x+b.width/2+65,b.y+b.height/2+20,{steps:8});await page.mouse.up();await stable();assert.notDeepEqual((await state()).camera,before);assert.equal((await state()).sharedCamera,true);
  }
  const zoom=(await state()).zoom;await page.mouse.wheel(0,-180);await page.waitForTimeout(100);await stable();assert.notEqual((await state()).zoom,zoom);
  await page.locator('#projection').selectOption('perspective');await stable();await repairCheck();
  for(const [key,value]of [['curl','-0.65'],['width','1.4'],['fold','0.7'],['season','1'],['veins','0'],['transmission','0']]){
    await page.locator('#model-'+key).fill(value);await stable();assert.equal((await state()).modelSettings[key],Number(value));
  }
  await page.locator('#reset-all').click();await stable();assert.equal((await state()).repairEnabled,false);
  await page.screenshot({path:path.join(artifacts,'leaf.png'),fullPage:true});
  await select('butterfly');assert.equal((await state()).triangles,156);assert.equal((await state()).clips[0].duration,1);
  assert.doesNotMatch(await page.locator('body').innerText(),/\bv\d+\b|\bVersion\s*\d+/i);
  // A zero-add frame is legitimate, but is NOT evidence that butterfly repair works.
  const connectedImage=await png();assert.equal(await repairCheck(),0);assert.equal(await png(),connectedImage);
  assert.match(await page.locator('#repair-info').textContent(),/无需补点.*八邻接连通/);
  await page.locator('#front').click();await page.locator('#phase').fill('100');await stable();
  assert.equal(await repairCheck(),2,'butterfly front / 12px / 0.100s must repair actual gaps');
  assert.match(await page.locator('#repair-info').textContent(),/已补 2 个像素/);
  await page.locator('#resolution').fill('22');await page.locator('#edge').click();await page.locator('#phase').fill('600');await stable();
  assert.equal(await repairCheck(),0);assert.ok((await state()).repair.groups.some(group=>group.after>1));
  assert.match(await page.locator('#repair-info').textContent(),/没有允许的几何连接路径/);
  await select('butterfly');
  for(const fps of [2,5,12,24,60]){
    await page.locator('#fps').fill(String(fps));await page.locator('#phase').fill('237');await stable();const s=await state();assert.equal(s.sourceTime,Math.floor(.237*fps)/fps);assert.equal(s.sourceTime,s.pixelTime);
  }
  await page.locator('#next-frame').click();await stable();assert.equal((await state()).pixelTime,15/60);
  await page.locator('#previous-frame').click();await stable();assert.equal((await state()).pixelTime,14/60);
  await page.locator('#model-color .color-trigger').click();await page.locator('#model-color .hex-input').fill('#f80');await stable();assert.equal((await state()).modelSettings.color,'#ff8800');
  await page.locator('#model-color .hex-input').fill('#zzzzzz');assert.equal((await state()).modelSettings.color,'#ff8800');await page.keyboard.press('Escape');
  await page.locator('#model-shadows').uncheck();await stable();const pixels=await rgba();
  for(let i=0;i<pixels.length;i+=4)if(pixels[i+3])assert.deepEqual(pixels.slice(i,i+4),[255,136,0,255]);
  await page.locator('#model-shadows').check();await stable();
  for(const phase of [0,100,250,500,750,950]){
    await page.locator('#phase').fill(String(phase));await stable();await repairCheck();const s=await state();assert.equal(s.repair.groups.length,2);assert.equal(s.sourceTime,s.pixelTime);
  }
  await page.locator('#resolution').fill('128');await stable();await repairCheck();
  await page.locator('#projection').selectOption('perspective');await stable();await repairCheck();
  await page.locator('#resolution').fill('32');await page.locator('#phase').fill('237');await stable();
  const savedImage=await png(),savedPreset=(await state()).preset;
  const exportPromise=page.waitForEvent('download');await page.locator('#export-preset').click();const exported=await exportPromise;const presetFile=path.join(artifacts,'preset.json');await exported.saveAs(presetFile);assert.deepEqual(JSON.parse(await fs.readFile(presetFile,'utf8')),savedPreset);
  await select('leaf');await page.locator('#preset-file').setInputFiles(presetFile);await stable();assert.equal((await state()).model,'butterfly');assert.equal(await png(),savedImage);
  await page.locator('#preset-file').setInputFiles({name:'bad.json',mimeType:'application/json',buffer:Buffer.from('{"version":999}')});await page.waitForFunction(()=>document.querySelector('#status').classList.contains('error'));assert.equal(await png(),savedImage);
  await page.locator('#preset-file').setInputFiles(presetFile);await stable();
  const beforeWire=await png();await page.locator('#wireframe').check();await stable();assert.equal(await png(),beforeWire);await page.locator('#wireframe').uncheck();
  await page.locator('#levels').fill('4');await stable();assert.notEqual(await png(),savedImage);await page.locator('#levels').fill('0');await stable();assert.equal(await png(),savedImage);
  const downloadPromise=page.waitForEvent('download');await page.locator('#download').click();const downloaded=await downloadPromise;assert.match(downloaded.suggestedFilename(),/butterfly-B-connectivity-32px/);const file=path.join(artifacts,'pixel.png');await downloaded.saveAs(file);const data=await fs.readFile(file);assert.equal(data.readUInt32BE(16),32);assert.equal(data.readUInt32BE(20),32);
  await page.locator('#play').click();const time=(await state()).pixelTime;await page.waitForTimeout(200);assert.notEqual((await state()).pixelTime,time);await page.locator('#play').click();await page.locator('#phase').fill('237');await stable();
  await page.screenshot({path:path.join(artifacts,'butterfly.png'),fullPage:true});
  // Stale asynchronous GLB loads must not replace the user's newer selection.
  await page.route('**/assets/models/butterfly.glb',async route=>{await new Promise(resolve=>setTimeout(resolve,150));await route.continue();});
  await select('leaf');await page.locator('#model').selectOption('butterfly');await page.locator('#model').selectOption('leaf');await stable();await page.waitForTimeout(250);assert.equal((await state()).model,'leaf');await page.unroute('**/assets/models/butterfly.glb');
  for(let i=0;i<4;i++){await select('butterfly');await select('leaf');}
  for(const variant of ['model','pixel','compare']){await page.locator('#next').click();await stable();assert.equal((await state()).variant,variant);}
  for(const viewport of [{width:1280,height:720},{width:390,height:844}]){
    await page.setViewportSize(viewport);await stable();assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);await page.screenshot({path:path.join(artifacts,`viewport-${viewport.width}.png`),fullPage:true});
  }
  // Migration redirects (only enabled after legacy pages have been retired).
  if(process.env.CHECK_REDIRECTS)for(const [url,model]of [['/leaf-prototype/','leaf'],['/butterfly-method-comparison/','butterfly']]){
    await page.waitForLoadState('networkidle');
    await page.goto(base+url);await stable();assert.equal((await state()).model,model);assert.ok(page.url().includes('/model-preview/'));
  }
  assert.deepEqual(errors,[]);assert.deepEqual(failed,[]);assert.ok(urls.every(url=>url.startsWith(base)),'no external network dependencies');
  console.log('PASS: model switching/races, both projections and controls, native buffers, 156-triangle grouped repair, original RGBA, shared animation/stepping, color/shadows, PNG, preset round-trip/rejection, layouts/mobile; no page/console/request errors.');
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(()=>server?.close());
