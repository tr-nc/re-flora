// Optional manual browser validation, outside cargo test. Requires Playwright.
// PREVIEW_URL, PREVIEW_ARTIFACT_DIR and CHROME_EXECUTABLE can override defaults.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

(async () => {
  const out = process.env.PREVIEW_ARTIFACT_DIR || 'target/butterfly-resume/v4-browser';
  fs.mkdirSync(out, {recursive: true});
  const browser = await chromium.launch({
    executablePath: process.env.CHROME_EXECUTABLE || '/usr/bin/google-chrome',
    headless: true, args: ['--no-sandbox', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'],
  });
  try {
    const context = await browser.newContext({viewport: {width: 1280, height: 900}, deviceScaleFactor: 2});
    const page = await context.newPage();
    const errors = [], failedRequests = [], urls = [];
    page.on('pageerror', e => errors.push(e.message));
    page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
    page.on('request', r => urls.push(r.url()));
    page.on('requestfailed', r => failedRequests.push(r.url()));
    page.on('response', r => { if (r.status() >= 400) failedRequests.push(`${r.status()} ${r.url()}`); });
    const url = process.env.PREVIEW_URL || 'http://127.0.0.1:8793/comparison-v4.html';
    await page.goto(url);
    const state = () => page.evaluate(async () => (await import('./free-view-preview.js')).readPreviewState());
    await page.waitForFunction(async () => (await import('./free-view-preview.js')).readPreviewState().ready);
    await page.locator('#play').click();
    assert.equal((await state()).playing, false);
    await page.locator('#phase').fill('237');
    await page.waitForFunction(async () => (await import('./free-view-preview.js')).readPreviewState().sampledTime === .237);
    let s = await state();
    assert.equal(s.sharedCamera, true);
    assert.deepEqual(s.pixelSize, [12, 12]);
    assert.ok(s.sourceSize[0] >= 384);
    assert.ok(s.meshNames.length > 0);
    assert.ok(s.meshNames.every(n => !/head|thorax|abdomen|body/i.test(n)));
    const pixels = () => page.locator('#pixel-canvas').evaluate(c => c.toDataURL());
    const assertSynchronized = async () => {
      const current = await state();
      assert.equal(current.sharedCamera, true);
      assert.equal(current.sourceTime, current.pixelTime);
      assert.equal(current.sourceTime, current.sampledTime);
    };
    async function drag(id, dx, dy) {
      const canvas = page.locator(id);
      await canvas.scrollIntoViewIfNeeded();
      const before = await state(), beforePixels = await pixels();
      const b = await canvas.boundingBox();
      const x = b.x+b.width*.45, y = b.y+b.height*.45;
      await page.mouse.move(x,y); await page.mouse.down();
      await page.mouse.move(x+dx,y+dy,{steps:12}); await page.mouse.up();
      await page.waitForFunction(old => document.querySelector('#pixel-canvas').toDataURL() !== old, beforePixels);
      assert.notDeepEqual((await state()).cameraPosition, before.cameraPosition);
      await assertSynchronized();
    }
    await drag('#source-canvas', 82, 35);
    await drag('#pixel-canvas', -67, 27);
    await drag('#source-canvas', 45, -62);
    await page.locator('#pixel-canvas').focus();
    const beforeKey = (await state()).cameraPosition;
    await page.keyboard.press('ArrowRight');
    assert.notDeepEqual((await state()).cameraPosition, beforeKey);
    await assertSynchronized();
    for (const resolution of [12,16,24,32,48]) {
      await page.locator('#resolution').selectOption(String(resolution));
      await page.waitForFunction(n => document.querySelector('#pixel-canvas').width === n, resolution);
      s = await state(); assert.deepEqual(s.pixelSize, [resolution,resolution]);
      const width = await page.locator('#pixel-canvas').evaluate(c => c.getBoundingClientRect().width);
      assert.equal(width % resolution, 0);
      await assertSynchronized();
    }
    await page.locator('#projection').selectOption('perspective');
    assert.equal((await state()).projection, 'perspective');
    await drag('#pixel-canvas', 52, -30);
    await page.locator('#projection').selectOption('orthographic');
    await page.locator('#sampling').selectOption('stepped');
    await page.locator('#phase').fill('673');
    await page.waitForFunction(async () => (await import('./free-view-preview.js')).readPreviewState().sampledTime === .6);
    await assertSynchronized();
    const downloadPromise = page.waitForEvent('download');
    await page.locator('#download').click();
    const download = await downloadPromise;
    const file = path.join(out,'download.png'); await download.saveAs(file);
    const data = fs.readFileSync(file);
    assert.equal(data.readUInt32BE(16),48); assert.equal(data.readUInt32BE(20),48);
    await page.locator('#resolution').selectOption('12');
    await page.locator('#reset').click(); await page.locator('#phase').fill('0');
    await page.waitForFunction(async () => (await import('./free-view-preview.js')).readPreviewState().sampledTime === 0);
    const phaseZeroPixels = await pixels();
    await page.locator('#phase').fill('410');
    await page.waitForFunction(old => document.querySelector('#pixel-canvas').toDataURL() !== old, phaseZeroPixels);
    await assertSynchronized();
    await page.locator('#phase').fill('0');
    await page.waitForFunction(old => document.querySelector('#pixel-canvas').toDataURL() === old, phaseZeroPixels);
    await page.screenshot({path:path.join(out,'desktop.png'),fullPage:true});
    await page.locator('#background').selectOption('#e5dfcc');
    await page.screenshot({path:path.join(out,'paper.png'),fullPage:true});
    await page.setViewportSize({width:390,height:844});
    await page.locator('#background').selectOption('#253039');
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.screenshot({path:path.join(out,'mobile.png'),fullPage:true});
    await drag('#pixel-canvas', 45, 40);
    await page.locator('#sampling').selectOption('continuous');
    await page.locator('#play').click();
    const t = (await state()).sampledTime;
    await page.waitForFunction(async old => (await import('./free-view-preview.js')).readPreviewState().sampledTime !== old, t);
    await page.locator('#play').click();
    await assertSynchronized();
    assert.deepEqual(errors, []); assert.deepEqual(failedRequests, []);
    assert.equal(urls.filter(u => /atlas|\.png(?:$|\?)/.test(u)).length,0);
    assert.ok(urls.every(u => u.startsWith(new URL(url).origin)));
    const reduced = await browser.newPage({reducedMotion:'reduce'});
    await reduced.goto(url);
    await reduced.waitForFunction(async () => (await import('./free-view-preview.js')).readPreviewState().ready);
    assert.equal(await reduced.locator('#play').innerText(),'播放');
    // Explicit failure state rather than silently freezing on a lost context.
    await reduced.locator('#pixel-canvas').evaluate(c => c.getContext('webgl2').getExtension('WEBGL_lose_context').loseContext());
    await reduced.waitForFunction(() => document.querySelector('#status').classList.contains('error'));
    assert.equal(await reduced.locator('#download').isDisabled(),true);
    await reduced.close();
    const result = {pass:true,checks:['source drag -> pixel view; pixel drag -> source view; alternating drags',
      'shared camera and animation time; arrow-key orbit', '12/16/24/32/48 native buffers independent of DPR=2',
      'perspective/orthographic; continuous/5fps; arbitrary 0.237s phase; pose pixels change and return',
      'download PNG 48x48; integer nearest-neighbor display', '390px layout and drag',
      'no atlas/image requests; all requests local', 'reduced-motion default; context-loss error handling'],
      consoleErrors:errors,failedRequests};
    fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(result,null,2)+'\n');
    console.log(JSON.stringify(result,null,2));
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode=1; });
