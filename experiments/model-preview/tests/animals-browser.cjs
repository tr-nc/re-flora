// Web-only correctness and art evidence. Uses the existing optional Playwright
// setup, no new renderer, native asset publication, or gameplay test fixture.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
(async()=>{
  const {createPreviewServer}=await import('../../../scripts/serve-model-preview.mjs');
  const {animalCatalog}=await import('../models/animal-geometry.mjs');
  const output=process.env.PREVIEW_ARTIFACT_DIR||path.resolve(__dirname,'../../../target/animal-preview-review');
  await fs.mkdir(output,{recursive:true});
  const server=createPreviewServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const base=`http://127.0.0.1:${server.address().port}`;let browser;
  try{
    browser=await chromium.launch({executablePath:process.env.CHROME_EXECUTABLE||'/usr/bin/google-chrome',headless:true,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
    const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],requests=[],rows=[];
    page.on('pageerror',error=>errors.push(error.message));page.on('console',message=>{if(message.type()==='error')errors.push(message.text());});
    page.on('requestfailed',request=>errors.push(`${request.failure()?.errorText}: ${request.url()}`));page.on('request',request=>requests.push(request.url()));
    const stable=()=>page.waitForFunction(()=>readModelPreview().ready&&!readModelPreview().loading&&!readModelPreview().dirty&&!readModelPreview().failed);
    const state=()=>page.evaluate(()=>readModelPreview());
    const image=id=>page.locator('#'+id).evaluate(canvas=>canvas.toDataURL());
    const input=async(id,value)=>{await page.locator('#'+id).fill(String(value));await stable();};
    let poses=0,originalSamples=0;const centerMisses=[];
    async function verify(){
      const samples=await page.evaluate(()=>{
        const s=readModelPreview(true),canvas=document.querySelector('#pixel'),gl=canvas.getContext('webgl2'),rgba=new Uint8Array(s.resolution*s.resolution*4);gl.readPixels(0,0,s.resolution,s.resolution,gl.RGBA,gl.UNSIGNED_BYTE,rgba);
        if(s.sourceTime!==s.pixelTime)throw Error('split animation clocks');
        if(s.repair.groups.length!==3)throw Error('body and two wings must keep separate repair IDs');
        let count=0;
        for(let i=0;i<s.originalRgba.length;i+=4)if(s.originalRgba[i+3]){
          count++;for(let c=0;c<4;c++)if(rgba[i+c]!==s.originalRgba[i+c])throw Error('repair overwrote an existing material sample');
        }
        if(!rgba.some((v,i)=>i%4===3&&v>0))throw Error('empty final animal');
        if(!count&&(!s.conservativeCoverage||s.repair.added===0))throw Error('missing source without geometric coverage');
        return {count,model:s.model,resolution:s.resolution,projection:s.projection,added:s.repair.added};
      });poses++;originalSamples+=samples.count;if(!samples.count)centerMisses.push(samples);
    }
    await page.goto(base+'/model-preview/?model=bee');await stable();
    for(const spec of animalCatalog){
      await page.locator('#model').selectOption(spec.id);await stable();assert.equal((await state()).model,spec.id);
      assert.equal((await state()).clips.length,2);assert.equal((await state()).pixelPartCount,0);
      const source=await image('source'),rest=await image('pixel'),triangles=(await state()).triangles;
      await input('phase',300);assert.equal(await image('pixel'),rest,'rest clip must not invent motion');
      await page.locator('#clip').selectOption('1');await stable();
      await input('phase',100);const flight=await image('pixel');await input('phase',230);assert.notEqual(await image('pixel'),flight,'absolute wing sampling changes silhouette');
      await page.screenshot({path:path.join(output,`${spec.id}-flight.png`),fullPage:true});
      for(const projection of ['orthographic','perspective']){
        await page.locator('#projection').selectOption(projection);await stable();
        for(const view of ['front','back','edge']){
          await page.locator('#'+view).click();await stable();
          for(const size of [8,32,64,128]){await input('resolution',size);await verify();}
        }
      }
      await page.locator('#reset-all').click();await stable();assert.equal(await image('pixel'),rest);
      await page.locator('#wireframe').check();await stable();assert.equal(await image('pixel'),rest);
      await page.locator('#wireframe').uncheck();await stable();
      await page.locator('#conservative-coverage').uncheck();await stable();await verify();
      assert.ok((await state()).repair.groups.every(g=>g.preserved===0));
      await page.locator('#conservative-coverage').check();await stable();
      const memory=(await state()).rendererMemory;
      for(let i=0;i<4;i++)for(const value of [.75,1.2]){await input('model-modelScale',value);await input('model-wingLength',value===.75?.65:1.15);}
      assert.deepEqual((await state()).rendererMemory,memory);
      await page.locator('#model-bodyColor').evaluate(picker=>{picker.value='#e946a9';picker.dispatchEvent(new Event('input',{bubbles:true}));});await stable();assert.notEqual(await image('pixel'),rest);
      await page.locator('#reset-all').click();await stable();assert.equal(await image('pixel'),rest);
      await page.locator('#clip').selectOption('1');await input('phase',150);
      const before=(await state()).pixelTime;await page.locator('#next-frame').click();await stable();assert.notEqual((await state()).pixelTime,before);
      await page.locator('#play').click();await page.waitForFunction(()=>readModelPreview().pixelTime>.3);await page.locator('#play').click();await stable();assert.equal((await state()).playing,false);
      await page.locator('#reset-all').click();await stable();
      const wait=page.waitForEvent('download');await page.locator('#download').click();const file=await wait;
      await file.saveAs(path.join(output,file.suggestedFilename()));const png=await fs.readFile(path.join(output,file.suggestedFilename()));
      assert.equal(png.readUInt32BE(16),48);assert.equal(png.readUInt32BE(20),48);
      const corner=await page.locator('#pixel').evaluate(canvas=>{const gl=canvas.getContext('webgl2'),pixel=new Uint8Array(4);gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);return pixel[3];});assert.equal(corner,0);
      rows.push({id:spec.id,label:spec.label,source,pixel:rest,triangles});
      await page.screenshot({path:path.join(output,`${spec.id}-rest.png`),fullPage:true});
    }
    const bounds=await page.evaluate(async()=>{
      const THREE=await import('three'),{animalDefinitions}=await import('/model-preview/models/animals.js'),results=[];
      for(const definition of animalDefinitions){
        const asset=await definition.create();try{
          asset.apply({...definition.defaults,modelScale:1.2,wingLength:1.15,wingMotion:1.3});
          let radius=0;const point=new THREE.Vector3(),target=new THREE.Vector3(...asset.view.target);
          for(let frame=0;frame<32;frame++){
            asset.sample(frame/32*asset.clips[1].duration,1);asset.scene.updateMatrixWorld(true);
            for(const mesh of asset.meshes)for(let i=0;i<mesh.geometry.attributes.position.count;i++)radius=Math.max(radius,point.fromBufferAttribute(mesh.geometry.attributes.position,i).applyMatrix4(mesh.matrixWorld).distanceTo(target));
          }
          results.push({id:definition.id,radius,halfSpan:asset.view.span/2});
        }finally{asset.dispose();}
      }return results;
    });for(const b of bounds)assert.ok(b.radius<b.halfSpan,`animation out of fixed framing: ${JSON.stringify(b)}`);
    // Warm the shared flower depth target once; it is pipeline-owned and
    // intentionally survives asset switches, unlike per-model geometry/maps.
    await page.locator('#model').selectOption('custom-flower');await stable();
    await page.locator('#model').selectOption('bee');await stable();const memory=(await state()).rendererMemory;
    for(let i=0;i<3;i++)for(const id of ['custom-flower','sparrow','swallow','bee']){await page.locator('#model').selectOption(id);await stable();}
    assert.deepEqual((await state()).rendererMemory,memory,'switching animals and textured flowers releases GPU resources');
    await page.setViewportSize({width:390,height:844});await stable();assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);await page.screenshot({path:path.join(output,'mobile.png'),fullPage:true});
    assert.deepEqual(errors,[]);assert.ok(requests.every(url=>url.startsWith(base)));assert.ok(originalSamples>1000);
    const sheet=await browser.newPage({viewport:{width:1050,height:1100}});
    await sheet.setContent(`<style>body{background:#17231f;color:#e4e8d6;font:16px system-ui;padding:24px}section{display:grid;grid-template-columns:180px 1fr 1fr;align-items:center;border-top:1px solid #526449}img{width:330px;height:300px;object-fit:contain}.pixel{image-rendering:pixelated}</style><h1>Web-only animal studies</h1><p>Bee · round sparrow · fork-tailed swallow — shared 48² postprocessing</p>${rows.map(r=>`<section><div>${r.label}<p>${r.triangles} triangles</p></div><img src="${r.source}"><img class="pixel" src="${r.pixel}"></section>`).join('')}`);
    await sheet.screenshot({path:path.join(output,'contact-sheet.png'),fullPage:true});await sheet.close();
    await fs.writeFile(path.join(output,'summary.json'),JSON.stringify({models:rows.map(({source,pixel,...row})=>row),poses,originalSamples,centerMisses,bounds,errors},null,2)+'\n');
    console.log(`PASS: ${rows.length} animals, ${poses} poses, ${originalSamples} preserved source samples; animation, coverage A/B, controls, bounds, PNG, disposal and mobile. ${output}`);
  }finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
})().catch(error=>{console.error(error);process.exitCode=1;});
