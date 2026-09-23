// Export an independent Three.js pose oracle for the Rust GLB consumer.
// Then: cargo test browser_model_pose_parity -- --ignored --nocapture
const {chromium}=require('playwright');
const fs=require('node:fs/promises');
const path=require('node:path');
(async()=>{
 const {createPreviewServer}=await import('../../../scripts/serve-model-preview.mjs');const server=createPreviewServer();
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 let browser;
 try{
  browser=await chromium.launch({executablePath:process.env.CHROME_EXECUTABLE||'/usr/bin/google-chrome',headless:true,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  const page=await browser.newPage();await page.goto(`http://127.0.0.1:${server.address().port}/model-preview/`);
  await page.waitForFunction(()=>window.readModelPreview?.().ready);
  const result=await page.evaluate(async()=>{
    const THREE=await import('three'),{GLTFLoader}=await import('three/addons/loaders/GLTFLoader.js');
    const result={};
    for(const name of ['butterfly']){
      const gltf=await new GLTFLoader().loadAsync(`/assets/models/${name}.glb`),mixer=new THREE.AnimationMixer(gltf.scene);
      if(gltf.animations[0])mixer.clipAction(gltf.animations[0]).play();
      result[name]=[];
      for(const time of [0,...Array.from({length:100},(_,i)=>(i+.37)/100),1]){
        mixer.setTime(time);gltf.scene.updateMatrixWorld(true);const positions=[];
        gltf.scene.traverse(mesh=>{if(!mesh.isMesh)return;const geometry=mesh.geometry,v=new THREE.Vector3();
          for(let i=0;i<(geometry.index?.count||geometry.attributes.position.count);i++){
            mesh.getVertexPosition(geometry.index?geometry.index.getX(i):i,v);positions.push(v.applyMatrix4(mesh.matrixWorld).toArray());
          }
        });result[name].push({time,positions});
      }
    }return result;
  });
  const output=path.resolve(__dirname,'../../../target/model-pose-reference.json');await fs.writeFile(output,JSON.stringify(result));
  console.log(`Three.js pose reference: ${output} (${result.butterfly.length} times)`);
 }finally{await browser?.close();server.close();}
})().catch(error=>{console.error(error);process.exitCode=1});
