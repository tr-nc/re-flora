// Independent WebGL center masks + JS repair oracle for the native planner.
const {chromium}=require('playwright');const fs=require('node:fs/promises');const path=require('node:path');
(async()=>{
 const {createPreviewServer}=await import('../../../scripts/serve-model-preview.mjs');const server=createPreviewServer();await new Promise(r=>server.listen(0,'127.0.0.1',r));let browser;
 try{
  browser=await chromium.launch({executablePath:process.env.CHROME_EXECUTABLE||'/usr/bin/google-chrome',headless:true,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  const page=await browser.newPage();await page.goto(`http://127.0.0.1:${server.address().port}/model-preview/`);
  const stable=()=>page.waitForFunction(()=>window.readModelPreview?.().ready&&!readModelPreview().loading&&!readModelPreview().dirty);await stable();const fixtures=[];
  for(const model of ['leaf','butterfly']){
   await page.locator('#model').selectOption(model);await stable();
   for(const n of [8,16,22,32,64]){await page.locator('#resolution').fill(String(n));
    for(const view of ['reset-view','front','back','edge']){await page.locator('#'+view).click();
     for(const phase of model==='leaf'?[0]:[0,100,237,400,600,800]){
      if(model==='butterfly')await page.locator('#phase').fill(String(phase));await stable();
      fixtures.push(await page.evaluate(()=>{
       const s=readModelPreview(true),n=s.resolution,canvas=new OffscreenCanvas(n,n),ctx=canvas.getContext('2d');ctx.drawImage(document.querySelector('#pixel'),0,0);const rgba=ctx.getImageData(0,0,n,n).data,additions=[];
       for(let p=0;p<n*n;p++)if(!s.originalRgba[p*4+3]&&rgba[((n-1-Math.floor(p/n))*n+p%n)*4+3])additions.push(p);
       return {size:n,owners:s.owners.map((id,i)=>s.originalRgba[i*4+3]?id:0),groups:s.projectedGroups,additions};
      }));
     }
    }
   }
  }
  const out=path.resolve(__dirname,'../../../target/model-repair-reference.json');await fs.writeFile(out,JSON.stringify(fixtures));console.log(`${fixtures.length} WebGL/JS repair fixtures: ${out}`);
 }finally{await browser?.close();server.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
