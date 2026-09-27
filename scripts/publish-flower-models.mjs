#!/usr/bin/env node
// One authoring recipe for the browser and native game. No runtime Node dependency.
import {readFile,writeFile} from 'node:fs/promises';
import {crc32} from 'node:zlib';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {flowerCatalog,flowerGeometry} from '../assets/models/flower-source.mjs';

export async function publishedFlowers(){
  const sources=await Promise.all(['../assets/models/flower-source.mjs','./publish-flower-models.mjs'].map(file=>readFile(new URL(file,import.meta.url))));
  const flowers=flowerCatalog.map(spec=>{
    const recipe=flowerGeometry(spec.id);
    return {id:spec.id,root:[0,-1.2,0],center:[0,.2,0],span:3.7,heads:recipe.heads,
      parts:recipe.parts.map(part=>({...part,color:[1,3,5].map(i=>parseInt(spec.defaults[part.material].slice(i,i+2),16))}))};
  });
  return JSON.stringify({source_crc32:crc32(Buffer.concat(sources)),flowers})+'\n';
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  if(process.argv.length!==2){
    console[process.argv[2]==='--help'?'log':'error']('Usage: node scripts/publish-flower-models.mjs\nRebuilds assets/models/flowers.json from the shared flower-source.mjs recipe.');
    if(process.argv[2]!=='--help')process.exitCode=2;
  }else{
    await writeFile(new URL('../assets/models/flowers.json',import.meta.url),await publishedFlowers());
    console.log('Published assets/models/flowers.json (8 original flower meshes).');
  }
}
