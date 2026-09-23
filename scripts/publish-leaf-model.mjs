#!/usr/bin/env node
// Deterministic published GLB; browser and Rust both read it. No Three/Blender dependency.
import {readFile,writeFile} from 'node:fs/promises';
import {crc32} from 'node:zlib';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {leafGeometry,leafDefaults} from '../assets/models/leaf-source.mjs';
export async function publishedLeaf(){
  const {positions,normals,uvs,indices}=leafGeometry(),parts=[],views=[],accessors=[];
  let offset=0;
  function attribute(array,type,componentType,bounds){
    const bytes=Buffer.from(array.buffer,array.byteOffset,array.byteLength);
    const view=views.length;views.push({buffer:0,byteOffset:offset,byteLength:bytes.length});parts.push(bytes);offset+=bytes.length;
    const padding=(4-offset%4)%4;parts.push(Buffer.alloc(padding));offset+=padding;
    const components={VEC3:3,VEC2:2,SCALAR:1}[type];
    accessors.push({bufferView:view,componentType,count:array.length/components,type,...bounds});return accessors.length-1;
  }
  const min=[0,1,2].map(axis=>Math.min(...positions.filter((_,i)=>i%3===axis))),max=[0,1,2].map(axis=>Math.max(...positions.filter((_,i)=>i%3===axis)));
  const attributes={POSITION:attribute(positions,'VEC3',5126,{min,max}),NORMAL:attribute(normals,'VEC3',5126),TEXCOORD_0:attribute(uvs,'VEC2',5126)};
  const index=attribute(indices,'SCALAR',5123);
  const recipe=await readFile(new URL('../assets/models/leaf-source.mjs',import.meta.url));
  const publisher=await readFile(new URL('./publish-leaf-model.mjs',import.meta.url));
  const gltf={asset:{version:'2.0',generator:'Re:Flora shared leaf publisher'},extras:{source_crc32:crc32(Buffer.concat([recipe,publisher])),defaults:leafDefaults},scene:0,scenes:[{nodes:[0]}],nodes:[{name:'Leaf',mesh:0}],meshes:[{primitives:[{attributes,indices:index,material:0}]}],materials:[{name:'Leaf',doubleSided:true,pbrMetallicRoughness:{baseColorFactor:[.105,.24,.026,1],metallicFactor:0,roughnessFactor:1}}],buffers:[{byteLength:offset}],bufferViews:views,accessors};
  const jsonBytes=Buffer.from(JSON.stringify(gltf)),json=Buffer.concat([jsonBytes,Buffer.alloc((4-jsonBytes.length%4)%4,32)]),bin=Buffer.concat(parts);
  const header=Buffer.alloc(20);header.writeUInt32LE(0x46546c67,0);header.writeUInt32LE(2,4);header.writeUInt32LE(28+json.length+bin.length,8);header.writeUInt32LE(json.length,12);header.writeUInt32LE(0x4e4f534a,16);
  const binHeader=Buffer.alloc(8);binHeader.writeUInt32LE(bin.length,0);binHeader.writeUInt32LE(0x004e4942,4);
  return Buffer.concat([header,json,binHeader,bin]);
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  if(process.argv.length!==2){
    console[process.argv[2]==='--help'?'log':'error']('Usage: node scripts/publish-leaf-model.mjs\nRebuilds assets/models/leaf.glb from leaf-source.mjs. Both preview and game consume that file.');
    if(process.argv[2]!=='--help')process.exitCode=2;
  }else{
    await writeFile(new URL('../assets/models/leaf.glb',import.meta.url),await publishedLeaf());console.log('Published assets/models/leaf.glb (32 triangles). Reload preview and rebuild game.');
  }
}
