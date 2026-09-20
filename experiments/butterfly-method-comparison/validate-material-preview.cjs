// Optional real-browser checks for the experimental v5 page; not cargo tests.
// Requires Playwright and Python + Pillow. PREVIEW_URL, PREVIEW_ARTIFACT_DIR,
// CHROME_EXECUTABLE and PYTHON override defaults.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

(async () => {
  const out = process.env.PREVIEW_ARTIFACT_DIR || 'target/butterfly-resume/v5-browser';
  fs.mkdirSync(out, {recursive:true});
  const browser = await chromium.launch({executablePath:process.env.CHROME_EXECUTABLE || '/usr/bin/google-chrome',
    headless:true,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  try {
    const page = await browser.newPage({viewport:{width:1280,height:1000},deviceScaleFactor:2});
    const errors=[], failures=[];
    page.on('pageerror',e=>errors.push(e.message));
    page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
    page.on('requestfailed',r=>failures.push(r.url()));
    page.on('response',r=>{if(r.status()>=400)failures.push(`${r.status()} ${r.url()}`);});
    await page.goto(process.env.PREVIEW_URL || 'http://127.0.0.1:8793/comparison-v5.html');
    await page.waitForFunction(async()=>{const s=(await import('./material-preview.js')).readPreviewState();return s.ready||s.failed;});
    const state=()=>page.evaluate(async()=>(await import('./material-preview.js')).readPreviewState());
    assert.equal((await state()).ready,true);
    const settle=()=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    const fill=async(id,value)=>{await page.locator(id).fill(String(value));await settle();};
    const check=async(id,value)=>{await page.locator(id).setChecked(value);await settle();};
    const color=async(id,value)=>{
      assert.equal(await page.locator(id).getAttribute('type'),'color');
      await page.locator(id).evaluate((el,value)=>{el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}));},value);
      await settle();
    };
    const pixels=()=>page.locator('#pixel-canvas').evaluate(canvas=>{
      const gl=canvas.getContext('webgl2'), bytes=new Uint8Array(canvas.width*canvas.height*4);
      gl.readPixels(0,0,canvas.width,canvas.height,gl.RGBA,gl.UNSIGNED_BYTE,bytes);
      return {size:canvas.width,bytes:Array.from(bytes)};
    });
    const pixelAt=(image,x,y)=>x<0||y<0||x>=image.size||y>=image.size?[0,0,0,0]:image.bytes.slice((y*image.size+x)*4,(y*image.size+x)*4+4);
    const near=(image,x,y)=>Array.from({length:9},(_,i)=>pixelAt(image,x+i%3-1,y+Math.floor(i/3)-1)[3]>127);
    const different=(a,b)=>a.bytes.some((v,i)=>v!==b.bytes[i]);
    const opaqueColors=image=>new Set(Array.from({length:image.size**2},(_,i)=>image.bytes.slice(i*4,i*4+4)).filter(p=>p[3]>127).map(p=>p.slice(0,3).join(',')));
    await page.locator('#play').click();await settle();
    assert.equal((await state()).playing,false);
    assert.equal(await page.locator('#reset').count(),0);
    assert.equal(await page.getByRole('button',{name:'恢复视角'}).count(),0);
    await fill('#phase',0);await fill('#resolution',24);
    await check('#shadows',false);await check('#outline-enabled',false);
    const base=await pixels();
    assert.equal(opaqueColors(base).size,1,'Same-colored, unlit surfaces must have no upper patch');
    const rgb=[...opaqueColors(base)][0].split(',').map(Number);
    rgb.forEach((v,i)=>assert.ok(Math.abs(v-[80,190,220][i])<=1,'Linear/sRGB round trip'));
    await color('#background','#ff00ff');
    assert.deepEqual(await pixels(),base,'Canvas background must never be baked into pixels');
    await check('#outline-enabled',true);await color('#outline-color','#ff0000');await fill('#outline-alpha',.5);
    const outer=await pixels();let ringCount=0;
    for(let y=0;y<base.size;y++)for(let x=0;x<base.size;x++){
      const original=pixelAt(base,x,y), actual=pixelAt(outer,x,y);
      if(original[3]>127)assert.deepEqual(actual,original,'Outside outline changed an existing surface pixel');
      else if(near(base,x,y).some(Boolean)){
        ringCount++;assert.ok(Math.abs(actual[3]-128)<=1,'Outside alpha must be 0.5');
        assert.ok(Math.abs(actual[0]-128)<=1&&actual[1]===0&&actual[2]===0,'Premultiplied red outline');
      }else assert.deepEqual(actual,[0,0,0,0]);
    }
    assert.ok(ringCount>0);
    await fill('#outline-alpha',0);assert.deepEqual(await pixels(),base,'Zero alpha is exactly identity');
    await fill('#outline-alpha',1);await check('#outline-outside',false);
    const inner=await pixels();let innerCount=0;
    for(let y=0;y<base.size;y++)for(let x=0;x<base.size;x++){
      const original=pixelAt(base,x,y), actual=pixelAt(inner,x,y);
      assert.equal(actual[3],original[3],'Inner outline cannot grow or erase coverage');
      if(original[3]>127&&near(base,x,y).some(a=>!a)){innerCount++;assert.deepEqual(actual,[255,0,0,255]);}
      else assert.deepEqual(actual,original);
    }
    assert.ok(innerCount>0);
    await fill('#outline-alpha',.5);
    const innerHalf=await pixels();assert.ok(different(innerHalf,base)&&different(innerHalf,inner));
    innerHalf.bytes.forEach((v,i)=>{if(i%4===3)assert.equal(v,base.bytes[i]);});
    await check('#outline-enabled',false);
    assert.deepEqual(await pixels(),base,'Disabling outline exactly restores source');
    for(let size=8;size<=64;size++){
      await fill('#resolution',size);
      const s=await state();assert.deepEqual(s.pixelSize,[size,size]);
      const display=await page.locator('#pixel-canvas').evaluate(c=>c.getBoundingClientRect().width);
      assert.equal(display%size,0);
      assert.equal(s.sharedCamera,true);assert.equal(s.sourceTime,s.pixelTime);
    }
    await fill('#resolution',64);
    await color('#upper-color','#ff0000');await color('#lower-color','#0000ff');
    let s=await state();
    assert.equal(s.surfaceMaterials.find(m=>m.name==='Wing upper').emissive,'ff0000');
    assert.equal(s.surfaceMaterials.find(m=>m.name==='Wing lower').emissive,'0000ff');
    const above=await pixels();
    assert.ok(opaqueColors(above).has('255,0,0'),'Upper surface must render the upper picker color');
    await page.locator('#pixel-canvas').focus();
    for(let i=0;i<20;i++)await page.keyboard.press('ArrowDown');
    await settle();
    const below=await pixels();
    const blueCount=im=>Array.from({length:im.size**2},(_,i)=>im.bytes.slice(i*4,i*4+4)).filter(p=>p[0]===0&&p[2]===255&&p[3]===255).length;
    assert.ok(blueCount(below)>blueCount(above),'Underside must expose independently colored lower material');
    assert.ok((await state()).cameraPosition[1]<0);
    for(let i=0;i<20;i++)await page.keyboard.press('ArrowUp');await settle();
    await color('#upper-color','#50bedc');await color('#lower-color','#50bedc');
    const unlit=await pixels();assert.equal(opaqueColors(unlit).size,1);
    await check('#shadows',true);
    const lit=await pixels();assert.ok(opaqueColors(lit).size>2,'Same colors must still have real geometric shading');
    assert.ok(different(unlit,lit));
    s=await state();assert.equal(s.keyShadowEnabled,true);assert.equal(s.sourceShadows,true);assert.equal(s.pixelShadows,true);assert.deepEqual(s.shadowMapSize,[1024,1024]);
    await check('#shadows',false);assert.deepEqual(await pixels(),unlit);
    await check('#shadows',true);
    // Alternating drags must continue to drive both cameras after postprocessing.
    async function drag(id,dx,dy){
      const canvas=page.locator(id);await canvas.scrollIntoViewIfNeeded();
      const before=(await state()).cameraPosition;
      const b=await canvas.boundingBox();const x=b.x+b.width*.5,y=b.y+b.height*.7;
      const hit=await page.evaluate(({x,y})=>({tag:document.elementFromPoint(x,y)?.outerHTML.slice(0,160),controls:document.querySelector('.controls-panel').getBoundingClientRect().toJSON()}),{x,y});
      assert.ok(hit.tag?.includes(id.slice(1)),JSON.stringify({id,b,x,y,hit}));
      await page.mouse.move(x,y);await page.mouse.down();await page.mouse.move(x+dx,y+dy,{steps:10});await page.mouse.up();await settle();
      const after=await state();assert.notDeepEqual(after.cameraPosition,before);assert.equal(after.sharedCamera,true);assert.equal(after.sourceTime,after.pixelTime);
    }
    await drag('#source-canvas',52,-35);await drag('#pixel-canvas',-42,30);
    await page.locator('#projection').selectOption('perspective');await settle();assert.equal((await state()).projection,'perspective');
    await page.locator('#projection').selectOption('orthographic');await settle();
    await page.locator('#sampling').selectOption('stepped');await fill('#phase',673);assert.equal((await state()).sampledTime,.6);
    await page.locator('#sampling').selectOption('continuous');await fill('#phase',237);assert.equal((await state()).sampledTime,.237);
    await fill('#resolution',32);await check('#outline-enabled',true);await check('#outline-outside',true);await fill('#outline-alpha',.5);
    const downloadPromise=page.waitForEvent('download');await page.locator('#download').click();
    const download=await downloadPromise;const file=path.join(out,'outline-alpha.png');await download.saveAs(file);
    const png=fs.readFileSync(file);assert.equal(png.readUInt32BE(16),32);assert.equal(png.readUInt32BE(20),32);
    execFileSync(process.env.PYTHON || '/usr/bin/python3', ['-c',
      `from PIL import Image\nimport sys\nim=Image.open(sys.argv[1]).convert('RGBA')\nassert set(im.getchannel('A').getdata())=={0,128,255}\nring=[p for p in im.getdata() if p[3]==128]\nassert ring and all(p[:3]==(255,0,0) for p in ring), set(ring)`, file]);
    await color('#outline-color','#17272e');await fill('#outline-alpha',.85);await fill('#resolution',16);await color('#background','#253039');
    await page.evaluate(()=>scrollTo(0,0));await settle();
    await page.screenshot({path:path.join(out,'desktop.png'),fullPage:true});
    await color('#background','#e5dfcc');await page.screenshot({path:path.join(out,'paper.png'),fullPage:true});
    await page.setViewportSize({width:390,height:844});await settle();
    assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
    await page.evaluate(()=>scrollTo(0,0));await page.screenshot({path:path.join(out,'mobile.png'),fullPage:true});
    await page.locator('#play').click();const t=(await state()).sampledTime;
    await page.waitForFunction(async old=>(await import('./material-preview.js')).readPreviewState().sampledTime!==old,t);
    await page.locator('#play').click();await settle();
    assert.deepEqual(errors,[]);assert.deepEqual(failures,[]);
    const result={pass:true,checks:['No reset-view button; background color picker excluded from PNG',
      'GPU outline versus CPU 8-neighbor oracle: outside dilation and inner overlay',
      'Outline alpha 0/0.5/1; premultiplied outside alpha; inner coverage preserved',
      'All 57 native resolutions from 8 through 64 at DPR=2',
      'Uniform upper color with lighting off; distinct upper/lower colors visible from above/below',
      'Same-color lit surfaces produce shading; both renderers enable real shadow maps; unlit restores exact colors',
      'Bidirectional orbit; projection; continuous and stepped animation; PNG alpha 0/128/255 and unpremultiplied red verified with Pillow',
      'Desktop and 390px mobile layouts'],consoleErrors:errors,failedRequests:failures};
    fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result,null,2));
  } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
