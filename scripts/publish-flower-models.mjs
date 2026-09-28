#!/usr/bin/env node
// One authoring recipe for the browser and native game. No runtime Node dependency.
import {readFile,writeFile} from 'node:fs/promises';
import {crc32} from 'node:zlib';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {flowerCatalog,flowerGeometry} from '../assets/models/flower-source.mjs';
import {completeFlowerHead} from '../assets/models/flower-head.mjs';

export async function publishedFlowers(){
  const sources=await Promise.all(['../assets/models/flower-source.mjs','../assets/models/flower-head.mjs','./publish-flower-models.mjs'].map(file=>readFile(new URL(file,import.meta.url))));
  const flowers=flowerCatalog.map(spec=>{
    const recipe=completeFlowerHead(flowerGeometry(spec.cacheTemplate??spec.id));
    const palette=['petalColor','innerColor','centerColor','stemColor'].map(key=>[1,3,5].map(i=>parseInt(spec.defaults[key].slice(i,i+2),16)));
    return {id:spec.id,display_name:spec.displayName,stem_layers:spec.stemLayers,cache_family:spec.cacheFamily??spec.id,palette,heads:recipe.heads,
      parts:recipe.parts.map(part=>({...part,color:[1,3,5].map(i=>parseInt(spec.defaults[part.material].slice(i,i+2),16))}))};
  });
  // Canonical authoring precision avoids cross-platform libm last-bit noise
  // while retaining much more precision than the runtime f32 representation.
  return JSON.stringify({source_crc32:crc32(Buffer.concat(sources)),flowers},(_key,value)=>typeof value==='number'?Number(value.toFixed(9)):value)+'\n';
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  if(process.argv.length!==2){
    console[process.argv[2]==='--help'?'log':'error']('Usage: node scripts/publish-flower-models.mjs\nRebuilds assets/models/flowers.json from the shared flower-source.mjs recipe.');
    if(process.argv[2]!=='--help')process.exitCode=2;
  }else{
    await writeFile(new URL('../assets/models/flowers.json',import.meta.url),await publishedFlowers());
    console.log(`Published assets/models/flowers.json (${flowerCatalog.length} complete flower-head meshes).`);
  }
}
