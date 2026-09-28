// Preview-only A/B: authored low-poly stems versus half-grass-edge 3D cubes.
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
    const toggle=async value=>{await page.locator('#model-voxelStems').setChecked(value);await stable();};
    const image=id=>page.locator('#'+id).evaluate(canvas=>canvas.toDataURL());
    const heads=()=>page.evaluate(()=>readModelPreview(true).partTiles.map(tile=>({rgba:tile.rgba,depth:tile.depth})));
    await page.goto(`http://127.0.0.1:${server.address().port}/model-preview/?model=wild-geranium`);await stable();
    // Geometry-level contract: leaves, heads, head bounds and anchors are exactly
    // identical between modes, even after changing the shared authoring controls.
    const geometry=await page.evaluate(async()=>{
      const {flowerDefinitions}=await import('/model-preview/models/flowers.js');
      const results=[];
      for(const definition of flowerDefinitions){
        const asset=await definition.create();
        const snapshot=()=>({
          unchanged:asset.meshes.filter(mesh=>!mesh.userData.stem).map(mesh=>({name:mesh.name,positions:Array.from(mesh.geometry.attributes.position.array),indices:Array.from(mesh.geometry.index.array)})),
          heads:asset.pixelParts.map(part=>({center:part.center.toArray(),span:part.span,anchor:part.anchor})),
          stem:asset.meshes.find(mesh=>mesh.userData.stem).userData.stem,
        });
        try{
          for(const shape of [{},{height:1.15,bend:-.25,leafSize:1.35,flowerSize:1.3}]){
            const settings={...definition.defaults,...shape};asset.apply(settings);const a=snapshot();
            asset.apply({...settings,voxelStems:true});const b=snapshot();
            results.push({id:definition.id,unchanged:JSON.stringify(a.unchanged)===JSON.stringify(b.unchanged),heads:JSON.stringify(a.heads)===JSON.stringify(b.heads),a:a.stem,b:b.stem});
          }
        }finally{asset.dispose();}
      }
      return results;
    });
    for(const item of geometry){assert.ok(item.unchanged&&item.heads,item.id);assert.equal(item.a.mode,'mesh');assert.equal(item.b.mode,'voxels');assert.equal(item.b.cellSize,.05);assert.ok(item.b.cells>0);}
    const rows=[];
    for(const spec of flowerCatalog){
      await page.locator('#model').selectOption(spec.id);await stable();
      assert.equal((await state()).modelSettings.voxelStems,false);
      const a=await image('pixel'),aSource=await image('source'),aHeads=await heads();
      await toggle(true);const b=await image('pixel');
      assert.notEqual(a,b,`${spec.id}: visible stem candidate`);
      assert.notEqual(aSource,await image('source'));
      assert.deepEqual(await heads(),aHeads,'stem A/B never changes head tiles');
      assert.equal((await state()).stem.mode,'voxels');
      assert.deepEqual((await state()).pixelBuffer,[512,512]);
      rows.push({label:spec.label,a,b});
      for(const projection of ['perspective','orthographic']){
        await page.locator('#projection').selectOption(projection);await stable();
        for(const view of ['back','edge','front']){await page.locator('#'+view).click();await stable();assert.equal((await state()).stem.mode,'voxels');}
      }
      await page.locator('#reset-view').click();await stable();
      await toggle(false);assert.equal(await image('pixel'),a,'A -> B -> A is byte-identical');
      await toggle(true);await page.locator('#reset-all').click();await stable();
      assert.equal((await state()).modelSettings.voxelStems,false);assert.equal(await image('pixel'),a);
    }
    await page.locator('#model').selectOption('wild-geranium');await stable();await toggle(true);
    const memory=(await state()).rendererMemory;
    for(let i=0;i<8;i++){await toggle(false);await toggle(true);}
    assert.deepEqual((await state()).rendererMemory,memory,'mesh replacements release GPU resources');
    await page.locator('#source').hover();await page.mouse.wheel(0,-600);await stable();
    await page.screenshot({path:path.join(artifacts,'closeup-b.png'),fullPage:true});
    await toggle(false);await page.screenshot({path:path.join(artifacts,'closeup-a.png'),fullPage:true});
    await toggle(true);await page.setViewportSize({width:390,height:844});await stable();
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
    for(const id of ['leaf','butterfly','apple']){await page.locator('#model').selectOption(id);await stable();assert.equal(await page.locator('#model-voxelStems').count(),0);}
    const sheet=await browser.newPage({viewport:{width:1000,height:2800}});
    await sheet.setContent(`<style>body{background:#293c36;color:#eee;font:16px system-ui}.row{display:grid;grid-template-columns:200px 360px 360px;align-items:center}img{width:340px;height:340px;image-rendering:pixelated}h1{font-size:22px}</style><h1>茎和分枝：A 原低模 / B 三维颗粒（草边长 1/2）</h1><p>叶片与花头不变；仅网页实验，非游戏光照或性能验收。</p>${rows.map(row=>`<div class="row"><strong>${row.label}</strong><img src="${row.a}"><img src="${row.b}"></div>`).join('')}`);
    await sheet.screenshot({path:path.join(artifacts,'stem-ab.png'),fullPage:true});
    assert.deepEqual(errors,[]);
    console.log(`PASS: ${flowerCatalog.length} stem A/Bs; unchanged leaf/head geometry, head tiles and anchors; perspective/orthographic views, reset, GPU disposal, mobile. Screenshots: ${artifacts}`);
  }finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
})().catch(error=>{console.error(error);process.exitCode=1;});
