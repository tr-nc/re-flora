// Headless integration and reproducible art-review sheet. Same optional
// Playwright / CHROME_EXECUTABLE setup as browser.cjs; no game or external URLs.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const artifacts=process.env.PREVIEW_ARTIFACT_DIR||path.resolve(__dirname,'../../../target/flower-study/validation');
(async()=>{
  const {createPreviewServer}=await import('../../../scripts/serve-model-preview.mjs');
  const {flowerCatalog}=await import('../models/flower-catalog.mjs');
  const server=createPreviewServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const base=`http://127.0.0.1:${server.address().port}`;
  let browser;
  try{
    await fs.mkdir(artifacts,{recursive:true});
    browser=await chromium.launch({executablePath:process.env.CHROME_EXECUTABLE||'/usr/bin/google-chrome',headless:true,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
    const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],requests=[],rows=[];
    page.on('pageerror',error=>errors.push(error.message));page.on('console',message=>{if(message.type()==='error')errors.push(message.text());});
    page.on('response',response=>{if(response.status()>=400)errors.push(`${response.status()}: ${response.url()}`);});
    page.on('requestfailed',request=>errors.push(request.url()));page.on('request',request=>requests.push(request.url()));
    const stable=()=>page.waitForFunction(()=>readModelPreview().ready&&!readModelPreview().loading&&!readModelPreview().dirty&&!readModelPreview().failed);
    const state=()=>page.evaluate(()=>readModelPreview());
    const image=id=>page.locator('#'+id).evaluate(canvas=>canvas.toDataURL());
    const input=async(id,value)=>{await page.locator('#'+id).fill(String(value));await stable();};
    const select=async id=>{await page.locator('#model').selectOption(id);await stable();assert.equal((await state()).model,id);};
    await page.goto(base+'/model-preview/?model=wild-geranium');await stable();
    assert.equal(await page.locator('#model option').count(),flowerCatalog.length+3);
    let poses=0,maxDepthError=0,depthSamples=0;
    async function verifyTiles(){
      const report=await page.evaluate(()=>{
        const s=readModelPreview(true),gl=document.querySelector('#pixel').getContext('webgl2');
        const subpixelScale=2**gl.getParameter(gl.SUBPIXEL_BITS);
        let colored=0,depthError=0,worst=null,checked=0;
        for(const tile of s.partTiles){
          const n=tile.size;
          if(tile.rgba.length!==n*n*4||tile.depth.length!==n*n)throw Error('wrong head tile size');
          for(let i=0;i<n*n;i++){
            if(!tile.rgba[i*4+3])continue;
            colored++;
            if(!Number.isFinite(tile.depth[i])||tile.depth[i]<0||tile.depth[i]>1)throw Error('invalid composite depth');
            if(tile.original[i*4+3]){
              for(let c=0;c<4;c++)if(tile.rgba[i*4+c]!==tile.original[i*4+c])throw Error('repair overwrote original head RGBA');
              // Independent center-sample depth oracle. SwiftShader reports
              // four subpixel bits: interpolation uses snapped raster vertices,
              // not the unsnapped CPU projection (notable at 8px).
              const x=i%n+.5,y=Math.floor(i/n)+.5;let nearest=Infinity,boundary=false;
              for(const group of tile.projectedGroups)for(const triangle of group.triangles){
                const [a,b,c]=triangle.map(p=>[Math.round(p[0]*subpixelScale)/subpixelScale,Math.round(p[1]*subpixelScale)/subpixelScale,p[2]]);
                const area=(b[1]-c[1])*(a[0]-c[0])+(c[0]-b[0])*(a[1]-c[1]);
                if(Math.abs(area)<1e-10)continue;
                const u=((b[1]-c[1])*(x-c[0])+(c[0]-b[0])*(y-c[1]))/area;
                const v=((c[1]-a[1])*(x-c[0])+(a[0]-c[0])*(y-c[1]))/area,w=1-u-v;
                const minimum=Math.min(u,v,w);
                if(minimum>=-1e-7){
                  boundary ||= minimum<1e-7; // top-left edge ownership is not an interior sample
                  nearest=Math.min(nearest,(a[2]*u+b[2]*v+c[2]*w)*.5+.5);
                }
              }
              if(!boundary&&Number.isFinite(nearest)){
                checked++;
                if(Math.abs(tile.depth[i]-nearest)>depthError){depthError=Math.abs(tile.depth[i]-nearest);worst={id:s.model,projection:s.projection,camera:s.camera,n,tile:tile.id,x,y,actual:tile.depth[i],nearest};}
              }
            }
          }
        }
        return {colored,depthError,worst,checked};
      });
      assert.ok(report.colored>0);assert.ok(report.depthError<.0001,`GPU depth error ${report.depthError}: ${JSON.stringify(report.worst)}`);
      maxDepthError=Math.max(maxDepthError,report.depthError);depthSamples+=report.checked;poses++;
    }
    for(const spec of flowerCatalog){
      await select(spec.id);assert.equal(await page.locator('#flower-heads-only').count(),0);
      assert.equal(await page.locator('#play').isDisabled(),true);
      assert.equal((await state()).pixelPartCount,1,'one complete flower model');
      const defaults=(await state()).modelSettings;
      const source=await image('source');
      assert.deepEqual((await state()).pixelBuffer,[512,512]);assert.equal((await state()).partTiles.length,1);
      const heads=await image('pixel');
      await verifyTiles();rows.push({name:spec.label,latin:spec.latin,source,heads,triangles:(await state()).triangles});
      await page.screenshot({path:path.join(artifacts,`${spec.id}-heads.png`),fullPage:true});
      await page.locator('#wireframe').check();await stable();assert.equal(await image('pixel'),heads,'source wireframe does not contaminate head tiles');
      await page.locator('#wireframe').uncheck();await stable();
      for(const projection of ['orthographic','perspective']){
        await page.locator('#projection').selectOption(projection);
        for(const view of ['front','back','edge']){
          await page.locator('#'+view).click();
          for(const n of [8,32,64]){
            await input('resolution',n);
            assert.deepEqual((await state()).pixelBuffer,[512,512]);
            await verifyTiles();
          }
        }
      }
      await page.locator('#projection').selectOption('orthographic');await page.locator('#reset-view').click();await input('resolution',128);await verifyTiles();
      await input('resolution',32);
      const fixed=await image('pixel');await page.locator('#source').hover();await page.mouse.wheel(0,-180);await stable();
      assert.equal(await image('pixel'),fixed,'inspection zoom leaves part framing unchanged');
      for(const [key,value]of Object.entries({flowerSize:1.3,opening:.6,tilt:-15})){
        const before=await image('source');await input('model-'+key,value);assert.equal((await state()).modelSettings[key],value);assert.notEqual(await image('source'),before,`${spec.id} ${key}`);
      }
      const beforeColor=await image('pixel');
      await page.locator('#model-petalColor').evaluate(picker=>{picker.value='#ffd369';picker.dispatchEvent(new Event('input',{bubbles:true}));});await stable();assert.notEqual(await image('pixel'),beforeColor);
      await page.locator('#conservative-coverage').uncheck();await stable();assert.ok((await state()).repair.groups.every(group=>group.preserved===0));
      await page.locator('#conservative-coverage').check();await stable();await verifyTiles();
      // Upper shape limits together exercise conservative, rotation-safe framing.
      for(const [key,value]of Object.entries({opening:1.35,tilt:85}))await input('model-'+key,value);
      await verifyTiles();
      await page.locator('#reset-all').click();await stable();assert.deepEqual((await state()).modelSettings,defaults);assert.equal(await image('pixel'),heads);
      {
        const wait=page.waitForEvent('download');await page.locator('#download').click();const file=await wait;
        assert.ok(file.suggestedFilename().includes('heads-512px-composite'));
        const output=path.join(artifacts,file.suggestedFilename());await file.saveAs(output);
        const png=await fs.readFile(output);assert.equal(png.readUInt32BE(16),512);assert.equal(png.readUInt32BE(20),512);
        const alpha=await page.locator('#pixel').evaluate(async canvas=>{const bitmap=await createImageBitmap(canvas),copy=new OffscreenCanvas(canvas.width,canvas.height),ctx=copy.getContext('2d');ctx.drawImage(bitmap,0,0);bitmap.close();return ctx.getImageData(0,0,1,1).data[3];});assert.equal(alpha,0);
      }
    }
    // Shared compositor must depth-test flowers against BOTH near and far stems,
    // and other flowers; a flat sprite-depth / painter-order solution fails here.
    const occlusion=await page.evaluate(async()=>{
      const THREE=await import('three'),{PreviewPipeline}=await import('/model-preview/pipeline.js');
      const source=document.createElement('canvas'),pixel=document.createElement('canvas'),pipeline=new PreviewPipeline(source,pixel);
      const scene=new THREE.Scene(),head=new THREE.Mesh(new THREE.PlaneGeometry(1,1),new THREE.MeshBasicMaterial({color:'#ff5577',side:THREE.DoubleSide}));
      const stem=new THREE.Mesh(new THREE.PlaneGeometry(.18,1.6),new THREE.MeshBasicMaterial({color:'#33bb66',side:THREE.DoubleSide}));
      scene.add(head,stem);
      const asset={scene,meshes:[stem,head],repairGroups:[{id:1,label:'Stem',meshes:[stem]},{id:2,label:'Head',meshes:[head]}],pixelParts:[{id:2,meshes:[head],center:new THREE.Vector3(),span:1.55}],shadows:false,sample(){},preparePass(){}};
      const results=[];pipeline.resize(256,32);
      try{
        for(const perspective of [false,true]){
          const camera=perspective?new THREE.PerspectiveCamera(30,1,.1,20):new THREE.OrthographicCamera(-1,1,1,-1,.1,20);camera.position.z=5;
          for(const z of [.3,-.3]){
            stem.position.z=z;pipeline.render(asset,camera,camera,{time:0,clip:0});
            const gl=pipeline.pixel.getContext(),data=new Uint8Array(4);gl.readPixels(256,256,1,1,gl.RGBA,gl.UNSIGNED_BYTE,data);results.push({perspective,z,color:Array.from(data)});
          }
          // Promote the overlapping stem-shaped mesh to a second flower tile.
          asset.pixelParts.push({id:1,meshes:[stem],center:new THREE.Vector3(),span:1.8});
          for(const z of [.3,-.3]){
            stem.position.z=z;pipeline.render(asset,camera,camera,{time:0,clip:0});
            const gl=pipeline.pixel.getContext(),data=new Uint8Array(4);gl.readPixels(256,256,1,1,gl.RGBA,gl.UNSIGNED_BYTE,data);results.push({perspective,z,color:Array.from(data)});
          }
          asset.pixelParts.pop();pipeline.releaseAsset();
        }
      }finally{pipeline.dispose();for(const mesh of [head,stem]){mesh.geometry.dispose();mesh.material.dispose();}}
      return results;
    });
    for(const sample of occlusion)assert.deepEqual(sample.color,sample.z>0?[51,187,102,255]:[255,85,119,255],JSON.stringify(sample));
    await select('wild-geranium');const memory=(await state()).rendererMemory;
    for(let i=0;i<6;i++){await select('forget-me-not');await input('resolution',64);await select('wild-geranium');}
    assert.deepEqual((await state()).rendererMemory,memory,'GPU geometry/texture counts stabilize after repeated model/resolution changes');
    for(const model of ['leaf','butterfly','apple']){await select(model);assert.equal(await page.locator('#flower-comparison').isVisible(),false);assert.equal((await state()).partTiles,undefined);}
    await select('forget-me-not');
    for(const width of [1280,390]){await page.setViewportSize({width,height:width===390?844:720});await stable();assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);await page.screenshot({path:path.join(artifacts,`heads-viewport-${width}.png`),fullPage:true});}
    assert.deepEqual(errors,[]);assert.ok(requests.every(url=>url.startsWith(base)),'preview has no external dependencies');
    assert.ok(depthSamples>1000,'depth oracle must exercise substantial interior samples');
    // Original output only: no third-party reference images embedded in the sheet.
    const sheet=await browser.newPage({viewport:{width:1200,height:2540},deviceScaleFactor:1});
    await sheet.setContent(`<style>*{box-sizing:border-box}body{margin:0;padding:28px;background:#17231f;color:#e4e8d6;font:14px system-ui}header{padding:4px 12px 20px}h1{margin:0 0 8px;font-size:24px}p{margin:4px 0;color:#a8b6a8}.row{display:grid;grid-template-columns:230px repeat(2,1fr);height:282px;align-items:center;border-top:1px solid #3c5045;gap:10px}.name{padding:12px}img{width:270px;height:270px;object-fit:contain}.pixel{image-rendering:pixelated}.labels{display:grid;grid-template-columns:230px repeat(2,1fr);gap:10px;color:#cdd7b9;margin-bottom:8px}.labels span{text-align:center}small{color:#a7b5a8}</style><header><h1>Re:Flora / 十种低模花草</h1><p>完整花头模型与 32² 后处理（512² 透明合成）</p><p>仅网页预览，不是游戏性能验收。</p></header><div class="labels"><span>植物 / 三角形数</span><span>原始低模</span><span>花头后处理</span></div>${rows.map(row=>`<div class="row"><div class="name"><strong>${row.name}</strong><p>${row.latin}</p><small>${row.triangles} triangles</small></div><img src="${row.source}"><img class="pixel" src="${row.heads}"></div>`).join('')}`);
    await sheet.screenshot({path:path.join(artifacts,'flower-contact-sheet.png'),fullPage:true});await sheet.close();
    await fs.writeFile(path.join(artifacts,'summary.json'),JSON.stringify({models:rows.map(({name,triangles})=>({name,triangles})),headTilePoses:poses,maxDepthError,depthSamples,occlusionFixtures:occlusion.length,errors},null,2)+'\n');
    console.log(`PASS: ${flowerCatalog.length} flowers; ${poses} head-tile poses; original RGBA and GPU depth (max error ${maxDepthError}); ${occlusion.length} depth-occlusion fixtures; all parameters, PNGs, heads-only reset, resource disposal and mobile. Artifacts: ${artifacts}`);
  }finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
})().catch(error=>{console.error(error);process.exitCode=1;});
