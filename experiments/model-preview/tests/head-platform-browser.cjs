// Flower platform contract: complete head geometry + postprocessing, no assembly.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
  const {createPreviewServer}=await import('../../../scripts/serve-model-preview.mjs');
  const {flowerCatalog}=await import('../models/flower-catalog.mjs');
  const server=createPreviewServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  let browser;
  try{
    browser=await chromium.launch({executablePath:process.env.CHROME_EXECUTABLE||'/usr/bin/google-chrome',headless:true,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
    const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],requests=[];
    page.on('pageerror',error=>errors.push(error.message));
    page.on('request',request=>requests.push(request.url()));
    page.on('requestfailed',request=>errors.push(request.url()));
    const stable=()=>page.waitForFunction(()=>readModelPreview().ready&&!readModelPreview().loading&&!readModelPreview().dirty&&!readModelPreview().failed);
    const state=()=>page.evaluate(()=>readModelPreview());
    const image=()=>page.locator('#pixel').evaluate(canvas=>canvas.toDataURL());
    await page.goto(`http://127.0.0.1:${server.address().port}/model-preview/?model=star-strawberry`);await stable();
    assert.equal((await state()).model,'gillenia');
    const geometry=await page.evaluate(async()=>{
      const {flowerDefinitions}=await import('/model-preview/models/flowers.js');
      const results=[];
      for(const definition of flowerDefinitions){
        const asset=await definition.create();
        try{
          asset.apply(definition.defaults);
          results.push({id:definition.id,heads:asset.pixelParts.length,anchor:asset.pixelParts[0].anchor,
            allHead:asset.meshes.every(mesh=>mesh.userData.head===0&&!mesh.userData.stem),
            calyx:asset.pixelParts[0].meshes.filter(mesh=>mesh.name.includes('calyx')).length});
        }finally{asset.dispose();}
      }
      return results;
    });
    for(const model of geometry){assert.equal(model.heads,1);assert.ok(model.allHead,model.id);assert.equal(model.calyx,1);assert.deepEqual(model.anchor,[0,0,0]);}
    for(const spec of flowerCatalog){
      await page.locator('#model').selectOption(spec.id);await stable();
      for(const key of ['height','bend','leafSize','leafColor','voxelStems']){
        assert.equal(await page.locator('#model-'+key).count(),0);
        assert.equal(Object.hasOwn((await state()).modelSettings,key),false);
      }
      assert.equal((await state()).pixelPartCount,1);
      const pixel=await image();
      for(const projection of ['perspective','orthographic']){
        await page.locator('#projection').selectOption(projection);await stable();
        for(const view of ['back','edge','front']){await page.locator('#'+view).click();await stable();}
      }
      await page.locator('#reset-all').click();await stable();assert.equal(await image(),pixel);
    }
    const memory=(await state()).rendererMemory;
    for(let i=0;i<8;i++)for(const size of ['0.75','1']){await page.locator('#model-flowerSize').fill(size);await stable();}
    assert.deepEqual((await state()).rendererMemory,memory);
    await page.setViewportSize({width:390,height:844});await stable();
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
    assert.equal(requests.some(url=>/flower-(stem|topology)\.mjs/.test(url)),false);
    assert.deepEqual(errors,[]);
    console.log(`PASS: ${flowerCatalog.length} head-only models, calyx preservation, no assembly controls/imports, reset, projections, GPU disposal, mobile.`);
  }finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
})().catch(error=>{console.error(error);process.exitCode=1;});
