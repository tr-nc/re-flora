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
  page.on('request',request=>urls.push(request.url()));page.on('requestfailed',request=>{
    // Switching models may intentionally cancel an obsolete in-flight GLB.
    if(/\/assets\/models\/(?:leaf|butterfly)\.glb$/.test(request.url())&&request.failure()?.errorText==='net::ERR_ABORTED')return;
    failed.push(`${request.url()}: ${request.failure()?.errorText}`);
  });page.on('response',response=>{if(response.status()>=400)failed.push(response.url())});
  const stable=()=>page.waitForFunction(()=>readModelPreview().ready&&!readModelPreview().loading&&!readModelPreview().dirty&&!readModelPreview().failed);
  const state=()=>page.evaluate(()=>readModelPreview());
  const png=()=>page.locator('#pixel').evaluate(c=>c.toDataURL());
  const rgba=()=>page.evaluate(async()=>{
    const image=await createImageBitmap(document.querySelector('#pixel')),canvas=new OffscreenCanvas(image.width,image.height),ctx=canvas.getContext('2d');ctx.drawImage(image,0,0);image.close();return Array.from(ctx.getImageData(0,0,canvas.width,canvas.height).data);
  });
  async function select(model){await page.locator('#model').selectOption(model);await page.waitForFunction(id=>readModelPreview().model===id&&!readModelPreview().loading&&!readModelPreview().dirty,model);}
  async function repairCheck(){
    await stable();
    const original=await page.evaluate(()=>{const s=readModelPreview(true),n=s.resolution;return Array.from({length:n*n*4},(_,i)=>s.originalRgba[(n-1-Math.floor(i/(n*4)))*n*4+i%(n*4)]);});
    const result=await rgba(),levels=(await state()).levels;let added=0;
    for(let i=0;i<original.length;i+=4){
      if(original[i+3]){
        assert.equal(result[i+3],original[i+3],'color quantization must not erase an original sample');
        if(!levels)assert.deepEqual(result.slice(i,i+4),original.slice(i,i+4));
      }else if(result[i+3])added++;
    }
    assert.equal(added,(await state()).repair.added);
    return added;
  }
  await page.goto(base+'/model-preview/');await stable();
  assert.equal((await state()).triangles,32);assert.equal(await page.locator('#play').isDisabled(),true);
  assert.equal((await state()).levels,8,'leaf retains its eight-shade default in the single control');
  assert.equal(await page.locator('#model-steps').count(),0,'no second leaf-only light-quantization slider');
  assert.ok((await repairCheck())>=1);
  await page.locator('#front').click();await stable();
  const stemMissing=await page.evaluate(async()=>{
    const s=readModelPreview(true),{projectedCoverage}=await import('/model-preview/connectivity.mjs');
    const stem=projectedCoverage(s.projectedGroups[0].triangles.slice(-4),s.resolution);
    const image=await createImageBitmap(document.querySelector('#pixel'));
    const canvas=new OffscreenCanvas(s.resolution,s.resolution),ctx=canvas.getContext('2d');ctx.drawImage(image,0,0);image.close();
    const final=ctx.getImageData(0,0,s.resolution,s.resolution).data;
    return Array.from({length:s.resolution*s.resolution},(_,i)=>i).filter(i=>{
      const canvasIndex=(s.resolution-1-Math.floor(i/s.resolution))*s.resolution+i%s.resolution;
      return stem.has(i)&&!s.originalRgba[i*4+3]&&!final[canvasIndex*4+3];
    }).length;
  });
  assert.equal(stemMissing,0,'front 32px stem footprint must survive partial center sampling');
  const coveredFront=await png(),coveredAdded=(await state()).repair.added;
  await page.locator('#conservative-coverage').uncheck();await stable();
  assert.equal((await state()).conservativeCoverage,false);
  assert.match(await page.locator('#coverage-mode').innerText(),/旧八邻接/);
  assert.ok((await state()).repair.added<coveredAdded,'old bridge-only mode must omit conservative footprint pixels');
  assert.equal((await state()).repair.groups[0].preserved,0);
  assert.notEqual(await png(),coveredFront);
  await page.screenshot({path:path.join(artifacts,'leaf-bridge-only.png'),fullPage:true});
  await repairCheck();
  const oldDownload=page.waitForEvent('download');await page.locator('#download').click();
  assert.match((await oldDownload).suggestedFilename(),/leaf-bridge-only-32px/);
  await page.locator('#conservative-coverage').check();await stable();
  assert.equal((await state()).conservativeCoverage,true);
  assert.equal(await png(),coveredFront,'toggling back restores the conservative tile in place');
  await page.screenshot({path:path.join(artifacts,'leaf-coverage.png'),fullPage:true});
  await page.locator('#reset-view').click();await stable();
  for(const n of [8,12,16,22,32,64,128]){
    await page.locator('#resolution').fill(String(n));await stable();assert.deepEqual((await state()).pixelBuffer,[n,n]);await repairCheck();
  }
  for(const id of ['source','pixel']){
    const before=(await state()).camera,b=await page.locator('#'+id).boundingBox();
    await page.mouse.move(b.x+b.width/2,b.y+b.height/2);await page.mouse.down();await page.mouse.move(b.x+b.width/2+65,b.y+b.height/2+20,{steps:8});await page.mouse.up();await stable();assert.notDeepEqual((await state()).camera,before);assert.equal((await state()).linkedRotation,true);
  }
  const fixedImage=await png(),zoom=(await state()).zoom;
  await page.locator('#source').hover();await page.mouse.wheel(0,-180);await stable();
  assert.notEqual((await state()).zoom,zoom);assert.equal((await state()).pixelZoom,1);
  assert.equal(await png(),fixedImage,'source inspection zoom must not change game-framed pixels');
  const sourceZoom=(await state()).zoom;await page.locator('#pixel').hover();await page.mouse.wheel(0,-180);await stable();
  assert.equal((await state()).zoom,sourceZoom,'pixel canvas wheel does not zoom either camera');
  await page.locator('#projection').selectOption('perspective');await stable();await repairCheck();
  const perspectiveImage=await png(),sourcePosition=(await state()).camera;
  await page.locator('#source').hover();await page.mouse.wheel(0,-180);await stable();
  assert.notDeepEqual((await state()).camera,sourcePosition);
  assert.equal(await png(),perspectiveImage,'perspective inspection dolly must not change fixed tile framing');
  for(const [key,value]of [['curl','-0.65'],['width','1.4'],['length','1.15'],['fold','0.7'],['transmission','0']]){
    await page.locator('#model-'+key).fill(value);await stable();assert.equal((await state()).modelSettings[key],Number(value));
  }
  assert.equal(await page.locator('#model-season').count(),0);
  assert.equal(await page.locator('#model-veins').count(),0);
  const beforePreset=(await state()).modelSettings;
  await page.locator('[data-preset="秋日黄叶"]').click();await stable();
  const yellow=(await state()).modelSettings;
  assert.deepEqual(Object.keys(yellow).filter(key=>yellow[key]!==beforePreset[key]).sort(),['backTint','leafColor','stemTint','veinColor']);
  for(const key of ['leafColor','veinColor','stemTint','backTint'])assert.equal(await page.locator('#model-'+key).evaluate(picker=>picker.value),yellow[key]);
  const yellowImage=await png();
  await page.locator('[data-preset="盛夏绿叶"]').click();await stable();
  assert.notEqual(await png(),yellowImage,'color preset should recolor the visible leaf');
  await page.locator('#resolution').fill('32');await stable();
  await page.locator('#levels').fill('1');await stable();const summerShade=await rgba();
  await page.locator('[data-preset="秋日黄叶"]').click();await stable();
  assert.notDeepEqual(await rgba(),summerShade,'one-shade palette must follow newly selected leaf colors');
  for(const levels of ['1','2','3']){
    await page.locator('#levels').fill(levels);await stable();const image=await rgba();
    for(let i=0;i<image.length;i+=4)if(image[i+3])assert.ok(image[i]+image[i+1]+image[i+2]>0,`level ${levels} made a colored surface black`);
  }
  await page.locator('#levels').fill('2');await stable();
  await page.screenshot({path:path.join(artifacts,'leaf-dynamic-2.png'),fullPage:true});
  await page.locator('#levels').fill('0');await stable();
  for(const key of ['leafColor','veinColor','stemTint','backTint']){
    await page.locator('#model-'+key).evaluate(picker=>{picker.value='#FFFFFF';picker.dispatchEvent(new Event('input',{bubbles:true}));});
    await stable();assert.equal((await state()).modelSettings[key].toLowerCase(),'#ffffff');
  }
  await page.locator('#front').click();await stable();
  const whiteLeaf=await rgba(),colored=Array.from({length:whiteLeaf.length/4},(_,i)=>i).filter(i=>whiteLeaf[i*4+3]);
  assert.ok(colored.length>0);
  assert.ok(colored.every(i=>Math.abs(whiteLeaf[i*4]-whiteLeaf[i*4+1])<=2&&Math.abs(whiteLeaf[i*4+1]-whiteLeaf[i*4+2])<=2),'white base color must not retain the green palette');
  await page.locator('#model-veinColor').evaluate(picker=>{picker.value='#ff00ff';picker.dispatchEvent(new Event('input',{bubbles:true}));});
  await stable();assert.notDeepEqual(await rgba(),whiteLeaf,'vein picker should change rendered vein pixels without changing leaf base');
  await page.locator('#reset-all').click();await stable();assert.equal((await state()).repairEnabled,true);assert.equal((await state()).levels,8);assert.equal(await page.locator('#repair').count(),0);
  await page.screenshot({path:path.join(artifacts,'leaf.png'),fullPage:true});
  await select('butterfly');assert.equal((await state()).triangles,156);assert.equal((await state()).levels,8);assert.equal((await state()).clips[0].duration,1);
  assert.doesNotMatch(await page.locator('body').innerText(),/\bv\d+\b|\bVersion\s*\d+/i);
  const butterflyCovered=await repairCheck();assert.ok(butterflyCovered>0,'both wings have conservative coverage');
  await page.locator('#conservative-coverage').uncheck();await stable();
  assert.ok((await state()).repair.added<butterflyCovered,'butterfly also switches to bridge-only mode');
  await page.locator('#conservative-coverage').check();await stable();
  assert.equal((await state()).repair.added,butterflyCovered);
  await page.locator('#front').click();await page.locator('#phase').fill('100');await stable();
  assert.ok((await repairCheck())>0,'moving butterfly wings retain projected coverage');
  await page.locator('#resolution').fill('22');await page.locator('#edge').click();await page.locator('#phase').fill('600');await stable();
  await repairCheck();assert.equal((await state()).repair.groups.length,2);
  await select('butterfly');
  for(const fps of [2,5,12,24,60]){
    await page.locator('#fps').fill(String(fps));await page.locator('#phase').fill('237');await stable();const s=await state();assert.equal(s.sourceTime,Math.floor(.237*fps)/fps);assert.equal(s.sourceTime,s.pixelTime);
  }
  await page.locator('#next-frame').click();await stable();assert.equal((await state()).pixelTime,15/60);
  await page.locator('#previous-frame').click();await stable();assert.equal((await state()).pixelTime,14/60);
  await page.locator('#model-color .color-trigger').click();await page.locator('#model-color .hex-input').fill('#f80');await stable();assert.equal((await state()).modelSettings.color,'#ff8800');
  await page.locator('#model-color .hex-input').fill('#zzzzzz');assert.equal((await state()).modelSettings.color,'#ff8800');await page.keyboard.press('Escape');
  await page.locator('#levels').fill('1');await stable();
  for(let i=0,shade=await rgba();i<shade.length;i+=4)if(shade[i+3])assert.deepEqual(shade.slice(i,i+3),[255,136,0]);
  await page.locator('#model-color').evaluate(picker=>{picker.value='#0000ff';picker.dispatchEvent(new Event('input',{bubbles:true}));});await stable();
  for(let i=0,shade=await rgba();i<shade.length;i+=4)if(shade[i+3])assert.deepEqual(shade.slice(i,i+3),[0,0,255]);
  await page.locator('#model-color').evaluate(picker=>{picker.value='#ff8800';picker.dispatchEvent(new Event('input',{bubbles:true}));});await stable();
  await page.locator('#levels').fill('0');await stable();
  await page.locator('#model-shadows').uncheck();await stable();const pixels=await rgba();
  for(let i=0;i<pixels.length;i+=4)if(pixels[i+3])assert.deepEqual(pixels.slice(i,i+4),[255,136,0,255]);
  await page.locator('#model-shadows').check();await stable();
  for(const phase of [0,100,250,500,750,950]){
    await page.locator('#phase').fill(String(phase));await stable();await repairCheck();const s=await state();assert.equal(s.repair.groups.length,2);assert.equal(s.sourceTime,s.pixelTime);
  }
  await page.locator('#resolution').fill('128');await stable();await repairCheck();
  await page.locator('#projection').selectOption('perspective');await stable();await repairCheck();
  await page.locator('#resolution').fill('32');await page.locator('#phase').fill('237');await stable();
  const savedImage=await png();
  assert.equal(await page.locator('#export-preset,#import-preset,#preset-file').count(),0);
  const beforeWire=await png();await page.locator('#wireframe').check();await stable();assert.equal(await png(),beforeWire);await page.locator('#wireframe').uncheck();
  await page.locator('#levels').fill('4');await stable();assert.notEqual(await png(),savedImage);await page.locator('#levels').fill('0');await stable();assert.equal(await png(),savedImage);
  const downloadPromise=page.waitForEvent('download');await page.locator('#download').click();const downloaded=await downloadPromise;assert.match(downloaded.suggestedFilename(),/butterfly-coverage-32px/);const file=path.join(artifacts,'pixel.png');await downloaded.saveAs(file);const data=await fs.readFile(file);assert.equal(data.readUInt32BE(16),32);assert.equal(data.readUInt32BE(20),32);
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
  console.log('PASS: model switching/races, both projections and controls, native buffers, 156-triangle grouped repair, original RGBA, shared animation/stepping, color/shadows, PNG, layouts/mobile; no page/console/request errors.');
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(()=>server?.close());
