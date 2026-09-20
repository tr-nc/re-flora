// Manual browser validation for the compact debug page. Requires Playwright.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');

(async()=>{
  const out=process.env.PREVIEW_ARTIFACT_DIR || 'target/butterfly-resume/v6-browser';
  fs.mkdirSync(out,{recursive:true});
  const browser=await chromium.launch({executablePath:process.env.CHROME_EXECUTABLE || '/usr/bin/google-chrome',
    headless:true,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  try{
    const page=await browser.newPage({viewport:{width:1280,height:720},deviceScaleFactor:2});
    const errors=[],failures=[],requests=[];
    page.on('pageerror',e=>errors.push(e.message));
    page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
    page.on('request',r=>requests.push(r.url()));
    page.on('requestfailed',r=>failures.push(r.url()));
    page.on('response',r=>{if(r.status()>=400)failures.push(`${r.status()} ${r.url()}`);});
    await page.goto(process.env.PREVIEW_URL || 'http://127.0.0.1:8793/comparison-v6.html');
    await page.waitForFunction(async()=>{const s=(await import('./debug-preview.js')).readPreviewState();return s.ready||s.failed;});
    const state=()=>page.evaluate(async()=>(await import('./debug-preview.js')).readPreviewState());
    const settle=()=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    const fill=async(id,value)=>{await page.locator(id).fill(String(value));await settle();};
    assert.equal((await state()).ready,true);
    await page.locator('#play').click();await settle();
    assert.equal((await state()).playing,false);
    for(const selector of ['a','footer','#reset','#sampling','[id^="outline"]','#upper-color','#lower-color','input[type="color"]'])
      assert.equal(await page.locator(selector).count(),0,selector);
    assert.equal(await page.locator('debug-color-picker').count(),2);
    assert.equal(requests.some(url=>url.includes('pixel-outline.js')),false);
    assert.equal((await state()).settings.fps,60);

    async function assertFits(){
      const layout=await page.evaluate(()=>({w:innerWidth,h:innerHeight,sw:document.documentElement.scrollWidth,sh:document.documentElement.scrollHeight,
        rects:[...document.querySelectorAll('.board canvas')].map(c=>c.getBoundingClientRect().toJSON())}));
      assert.ok(layout.sw<=layout.w && layout.sh<=layout.h,JSON.stringify(layout));
      const [left,right]=layout.rects;
      assert.ok(left.right<=right.left && Math.abs(left.top-right.top)<1,'Previews must be side by side');
      for(const r of layout.rects)assert.ok(r.left>=0&&r.top>=0&&r.right<=layout.w&&r.bottom<=layout.h,'Entire canvas must fit');
      assert.equal(left.width,right.width);
    }
    await assertFits();
    await fill('#phase',723);
    for(let fps=2;fps<=60;fps++){
      await fill('#fps',fps);
      const s=await state(), expected=Math.floor(.723*fps)/fps;
      assert.equal(s.sampledTime,expected);assert.equal(s.sourceTime,expected);assert.equal(s.pixelTime,expected);
      assert.equal(s.sharedCamera,true);
    }
    for(let size=8;size<=64;size++){
      await fill('#resolution',size);assert.deepEqual((await state()).pixelSize,[size,size]);await assertFits();
    }
    await fill('#resolution',24);
    const picker=page.locator('#wing-color');
    await picker.getByRole('button',{name:'选择蝴蝶颜色'}).click();
    assert.equal(await page.locator('.picker-popover:popover-open').count(),1);
    await picker.locator('.hex-input').fill('#e37a21');await settle();
    assert.equal((await state()).settings.color,'#e37a21');
    assert.ok((await state()).surfaceMaterials.every(m=>m.color==='e37a21'),'All surfaces must share one color');
    await picker.locator('.hex-input').fill('oops');await settle();
    assert.equal(await picker.locator('.hex-input').getAttribute('aria-invalid'),'true');
    assert.equal((await state()).settings.color,'#e37a21','Invalid text must not change material');
    await picker.locator('.hex-input').fill('#abc');await settle();
    assert.equal((await state()).settings.color,'#aabbcc');
    await picker.locator('.picker-hue').fill('120');
    await picker.locator('.picker-saturation').fill('100');
    await picker.locator('.picker-value').fill('100');await settle();
    assert.equal((await state()).settings.color,'#00ff00');
    const square=await picker.locator('.picker-square').boundingBox();
    await page.mouse.click(square.x+square.width/2,square.y+square.height/2);await settle();
    assert.equal((await state()).settings.color,'#408040','SV square must select continuous values');
    await page.screenshot({path:path.join(out,'custom-picker.png')});
    await page.keyboard.press('Escape');await settle();
    assert.equal(await page.locator('.picker-popover:popover-open').count(),0);
    await page.locator('#shadows').uncheck();await settle();
    assert.ok((await state()).surfaceMaterials.every(m=>m.emissive==='408040'));
    const pixels=()=>page.locator('#pixel-canvas').evaluate(c=>{
      const gl=c.getContext('webgl2'), data=new Uint8Array(c.width*c.height*4);
      gl.readPixels(0,0,c.width,c.height,gl.RGBA,gl.UNSIGNED_BYTE,data);return Array.from(data);
    });
    const flat=await pixels(), colors=new Set();
    for(let i=0;i<flat.length;i+=4){assert.ok(flat[i+3]===0||flat[i+3]===255);if(flat[i+3])colors.add(flat.slice(i,i+3).join(','));}
    assert.equal(colors.size,1,'No outline or per-surface color difference');
    await page.locator('#background').getByRole('button',{name:'选择背景颜色'}).click();
    await page.locator('#background .hex-input').fill('#f2e8d2');await settle();
    assert.deepEqual(await pixels(),flat,'Background stays out of the RGBA render');
    assert.equal(await page.locator('.panel').first().evaluate(el=>getComputedStyle(el).backgroundColor),'rgb(242, 232, 210)');
    await page.keyboard.press('Escape');
    await page.locator('#shadows').check();await settle();assert.notDeepEqual(await pixels(),flat);

    async function drag(id,dx,dy){
      const b=await page.locator(id).boundingBox();const before=(await state()).cameraPosition;
      await page.mouse.move(b.x+b.width*.5,b.y+b.height*.5);await page.mouse.down();
      await page.mouse.move(b.x+b.width*.5+dx,b.y+b.height*.5+dy,{steps:8});await page.mouse.up();await settle();
      const s=await state();assert.notDeepEqual(s.cameraPosition,before);assert.equal(s.sharedCamera,true);assert.equal(s.sourceTime,s.pixelTime);
    }
    await drag('#source-canvas',65,30);await drag('#pixel-canvas',-45,-20);
    await page.locator('#projection').selectOption('perspective');await settle();assert.equal((await state()).projection,'perspective');
    await page.locator('#projection').selectOption('orthographic');await settle();
    await page.screenshot({path:path.join(out,'desktop.png')});
    for(const [w,h] of [[1024,600],[390,844],[844,390]]){
      await page.setViewportSize({width:w,height:h});await settle();await assertFits();
      await page.locator('#background').getByRole('button',{name:'选择背景颜色'}).click();await settle();
      const panel=await page.locator('.picker-popover:popover-open').boundingBox();
      assert.ok(panel.x>=0&&panel.y>=0&&panel.x+panel.width<=w&&panel.y+panel.height<=h,JSON.stringify(panel));
      await page.keyboard.press('Escape');await settle();
      await page.screenshot({path:path.join(out,`${w}x${h}.png`)});
    }
    await page.setViewportSize({width:1280,height:720});await settle();
    const downloadPromise=page.waitForEvent('download');await page.locator('#download').click();
    const file=path.join(out,'frame.png');await (await downloadPromise).saveAs(file);
    const png=fs.readFileSync(file);assert.equal(png.readUInt32BE(16),24);assert.equal(png.readUInt32BE(20),24);
    await page.locator('#play').click();const t=(await state()).sampledTime;
    await page.waitForFunction(async old=>(await import('./debug-preview.js')).readPreviewState().sampledTime!==old,t);
    await page.locator('#play').click();await settle();
    assert.deepEqual(errors,[]);assert.deepEqual(failures,[]);
    const result={pass:true,checks:['No outline pipeline/UI, separate surface colors, history links, footer, or sampling dropdown',
      '2–60 FPS: all 59 fixed sampling rates checked; 8–64px: all 57 native buffers checked',
      'Custom HSV square/hue/saturation/value and HEX; invalid input guarded; no native preset dialog',
      'One color applied to both surfaces; background excluded from PNG; shadow toggle retained',
      'Shared camera/animation, bidirectional drag, projection and PNG export',
      'Side-by-side canvases entirely fit 1280x720, 1024x600, 390x844, 844x390 with no page scrolling',
      'Color popovers fit viewport and dismiss with Escape'],consoleErrors:errors,failedRequests:failures};
    fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result,null,2));
  }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
