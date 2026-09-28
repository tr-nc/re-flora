// Fixed single-column stems: one terminal flower and one cube per height layer.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
(async()=>{
  const {createPreviewServer}=await import('../../../scripts/serve-model-preview.mjs');
  const {flowerCatalog}=await import('../models/flower-catalog.mjs');
  const server=createPreviewServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const artifacts=path.resolve(__dirname,'../../../target/stem-preview');await fs.mkdir(artifacts,{recursive:true});
  let browser;
  try{
    browser=await chromium.launch({executablePath:process.env.CHROME_EXECUTABLE||'/usr/bin/google-chrome',headless:true,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
    const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];
    page.on('pageerror',error=>errors.push(error.message));
    page.on('console',message=>{if(message.type()==='error')errors.push(message.text());});
    page.on('requestfailed',request=>errors.push(request.url()));
    page.on('response',response=>{if(response.status()>=400)errors.push(`${response.status()}: ${response.url()}`);});
    const stable=()=>page.waitForFunction(()=>readModelPreview().ready&&!readModelPreview().loading&&!readModelPreview().dirty&&!readModelPreview().failed);
    const state=()=>page.evaluate(()=>readModelPreview());
    const image=id=>page.locator('#'+id).evaluate(canvas=>canvas.toDataURL());
    await page.goto(`http://127.0.0.1:${server.address().port}/model-preview/?model=star-strawberry`);await stable();
    assert.equal((await state()).model,'gillenia','old misidentified bookmark resolves to the corrected plant');
    assert.equal(new URL(page.url()).searchParams.get('model'),'gillenia');
    assert.equal(await page.locator('#model').inputValue(),'gillenia');
    assert.equal(await page.locator('#model option[value="star-strawberry"]').count(),0);
    const geometry=await page.evaluate(async()=>{
      const {flowerDefinitions}=await import('/model-preview/models/flowers.js');
      const results=[];
      for(const definition of flowerDefinitions){
        const asset=await definition.create();
        const snapshot=()=>({
          meshes:asset.meshes.map(mesh=>({name:mesh.name,positions:Array.from(mesh.geometry.attributes.position.array),indices:Array.from(mesh.geometry.index.array)})),
          heads:asset.pixelParts.map(part=>({center:part.center.toArray(),span:part.span,anchor:part.anchor,calyxCount:part.meshes.filter(mesh=>mesh.name.includes('calyx')&&!mesh.userData.stem).length})),
          stem:asset.meshes.find(mesh=>mesh.userData.stem).userData.stem,
        });
        try{
          for(const shape of [{},{height:1.15,bend:-.25,leafSize:1.35,flowerSize:1.3}]){
            const settings={...definition.defaults,...shape};asset.apply(settings);const fixed=snapshot();
            asset.apply({...settings,voxelStems:false});const legacy=snapshot();
            results.push({id:definition.id,headCount:fixed.heads.length,calyxCount:fixed.heads[0].calyxCount,anchor:fixed.heads[0].anchor,unchanged:JSON.stringify(fixed)===JSON.stringify(legacy),stem:fixed.stem});
          }
        }finally{asset.dispose();}
      }
      return results;
    });
    for(const item of geometry){
      assert.ok(item.unchanged,item.id);assert.equal(item.stem.mode,'voxels');assert.equal(item.stem.cellSize,.05);
      assert.equal(item.headCount,1);assert.equal(item.calyxCount,1,'calyx stays in the pixelated head, not the voxel stalk');assert.deepEqual(item.anchor,item.stem.tip);
      assert.equal(item.stem.cells,item.stem.layerCenters.length);
      for(let i=1;i<item.stem.layerCenters.length;i++){
        const a=item.stem.layerCenters[i-1],b=item.stem.layerCenters[i];
        assert.ok(Math.abs(b[1]-a[1]-.05)<1e-8,'one cube per height');
        assert.ok(Math.abs(b[0]-a[0])<.05&&Math.abs(b[2]-a[2])<.05,'no disconnected layers');
      }
    }
    const rows=[];
    for(const spec of flowerCatalog){
      await page.locator('#model').selectOption(spec.id);await stable();
      assert.equal(await page.locator('#model-voxelStems').count(),0,'no stem A/B control');
      assert.equal(Object.hasOwn((await state()).modelSettings,'voxelStems'),false);
      assert.equal((await state()).stem.mode,'voxels');assert.equal((await state()).pixelPartCount,1);
      const pixel=await image('pixel');rows.push({label:spec.label,source:await image('source'),pixel});
      assert.deepEqual((await state()).pixelBuffer,[512,512]);
      for(const projection of ['perspective','orthographic']){
        await page.locator('#projection').selectOption(projection);await stable();
        for(const view of ['back','edge','front']){await page.locator('#'+view).click();await stable();assert.equal((await state()).stem.mode,'voxels');}
      }
      await page.locator('#reset-all').click();await stable();
      assert.equal((await state()).stem.mode,'voxels');assert.equal(await image('pixel'),pixel,'reset keeps the selected voxel stalk');
    }
    await page.locator('#model').selectOption('wild-geranium');await stable();
    const memory=(await state()).rendererMemory;
    for(let i=0;i<8;i++)for(const height of ['0.75','1']){await page.locator('#model-height').fill(height);await stable();}
    assert.deepEqual((await state()).rendererMemory,memory,'mesh replacements release GPU resources');
    await page.locator('#source').hover();await page.mouse.wheel(0,-600);await stable();
    await page.screenshot({path:path.join(artifacts,'closeup.png'),fullPage:true});
    await page.setViewportSize({width:390,height:844});await stable();
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
    for(const id of ['leaf','butterfly','apple']){await page.locator('#model').selectOption(id);await stable();assert.equal(await page.locator('#model-voxelStems').count(),0);}
    const sheet=await browser.newPage({viewport:{width:1000,height:2800}});
    await sheet.setContent(`<style>body{background:#293c36;color:#eee;font:16px system-ui}.row{display:grid;grid-template-columns:200px 360px 360px;align-items:center}img{width:340px;height:340px;image-rendering:pixelated}h1{font-size:22px}</style><h1>单列颗粒茎 · 每层一个方块（草边长 1/2）</h1><p>左：源模型；右：花头像素化 + 三维颗粒茎。花萼归花头；仅网页预览，非游戏光照或性能验收。</p>${rows.map(row=>`<div class="row"><strong>${row.label}</strong><img src="${row.source}"><img src="${row.pixel}"></div>`).join('')}`);
    await sheet.screenshot({path:path.join(artifacts,'single-stems.png'),fullPage:true});
    assert.deepEqual(errors,[]);
    console.log(`PASS: ${flowerCatalog.length} fixed voxel stems; one terminal flower with calyx, one cube per layer; no scope switch; perspective/orthographic views, reset, GPU disposal, mobile. Screenshots: ${artifacts}`);
  }finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
})().catch(error=>{console.error(error);process.exitCode=1;});
