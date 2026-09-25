#!/usr/bin/env node
// Deterministic GLBs derived from one leaf recipe; no Three/Blender runtime dependency.
import {readFile,writeFile} from 'node:fs/promises';
import {crc32} from 'node:zlib';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {leafGeometry,leafDefaults} from '../assets/models/leaf-source.mjs';

export const LEAF_VARIANT_COUNT=64;
const halton=(index,base)=>{
  let value=0,scale=1/base;
  while(index){value+=(index%base)*scale;index=Math.floor(index/base);scale/=base;}
  return value;
};
// Variant zero remains the approved leaf. Others cover the same five authoring
// controls without changing the stem palette, material, or leaf-flight physics.
export function leafVariantParameters(index){
  if(!Number.isInteger(index)||index<0||index>=LEAF_VARIANT_COUNT)throw new RangeError('invalid leaf variant');
  if(index===0)return {};
  return {
    width:.78+.44*halton(index,2),
    length:.82+.34*halton(index,3),
    widestPoint:.32+.38*halton(index,5),
    fold:.12+.4*halton(index,7),
    curl:-.45+1.35*halton(index,11),
  };
}

async function sourceFingerprint(){
  const recipe=await readFile(new URL('../assets/models/leaf-source.mjs',import.meta.url));
  const publisher=await readFile(new URL('./publish-leaf-model.mjs',import.meta.url));
  return crc32(Buffer.concat([recipe,publisher]));
}
function makeBuilder(){
  const parts=[],views=[],accessors=[];
  let offset=0;
  function attribute(array,type,componentType,bounds){
    const bytes=Buffer.from(array.buffer,array.byteOffset,array.byteLength);
    const view=views.length;views.push({buffer:0,byteOffset:offset,byteLength:bytes.length});parts.push(bytes);offset+=bytes.length;
    const padding=(4-offset%4)%4;parts.push(Buffer.alloc(padding));offset+=padding;
    const components={VEC3:3,VEC2:2,SCALAR:1}[type];
    accessors.push({bufferView:view,componentType,count:array.length/components,type,...bounds});return accessors.length-1;
  }
  function mesh(geometry){
    const {positions,normals,uvs,indices}=geometry;
    const min=[0,1,2].map(axis=>Math.min(...positions.filter((_,i)=>i%3===axis)));
    const max=[0,1,2].map(axis=>Math.max(...positions.filter((_,i)=>i%3===axis)));
    const attributes={POSITION:attribute(positions,'VEC3',5126,{min,max}),NORMAL:attribute(normals,'VEC3',5126),TEXCOORD_0:attribute(uvs,'VEC2',5126)};
    const index=attribute(indices,'SCALAR',5123);
    return {primitives:[{attributes,indices:index,material:0}]};
  }
  function finish(nodes,meshes,extras){
    const gltf={asset:{version:'2.0',generator:'Re:Flora shared leaf publisher'},extras,scene:0,
      scenes:[{nodes:nodes.map((_,i)=>i)}],nodes,meshes,
      materials:[{name:'Leaf',doubleSided:true,pbrMetallicRoughness:{baseColorFactor:[.105,.24,.026,1],metallicFactor:0,roughnessFactor:1}}],
      buffers:[{byteLength:offset}],bufferViews:views,accessors};
    const jsonBytes=Buffer.from(JSON.stringify(gltf)),json=Buffer.concat([jsonBytes,Buffer.alloc((4-jsonBytes.length%4)%4,32)]),bin=Buffer.concat(parts);
    const header=Buffer.alloc(20);header.writeUInt32LE(0x46546c67,0);header.writeUInt32LE(2,4);header.writeUInt32LE(28+json.length+bin.length,8);header.writeUInt32LE(json.length,12);header.writeUInt32LE(0x4e4f534a,16);
    const binHeader=Buffer.alloc(8);binHeader.writeUInt32LE(bin.length,0);binHeader.writeUInt32LE(0x004e4942,4);
    return Buffer.concat([header,json,binHeader,bin]);
  }
  return {mesh,finish};
}
export async function publishedLeaf(){
  const builder=makeBuilder();
  return builder.finish([{name:'Leaf',mesh:0}],[builder.mesh(leafGeometry())],
    {source_crc32:await sourceFingerprint(),defaults:leafDefaults});
}
export async function publishedLeafVariants(){
  const builder=makeBuilder(),nodes=[],meshes=[];
  for(let i=0;i<LEAF_VARIANT_COUNT;i++){
    meshes.push(builder.mesh(leafGeometry({...leafDefaults,...leafVariantParameters(i)})));
    nodes.push({name:`Leaf variant ${i}`,mesh:i});
  }
  return builder.finish(nodes,meshes,{source_crc32:await sourceFingerprint(),variant_count:LEAF_VARIANT_COUNT});
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  if(process.argv.length!==2){
    console[process.argv[2]==='--help'?'log':'error']('Usage: node scripts/publish-leaf-model.mjs\nRebuilds assets/models/leaf.glb and leaf-variants.glb from leaf-source.mjs.');
    if(process.argv[2]!=='--help')process.exitCode=2;
  }else{
    await writeFile(new URL('../assets/models/leaf.glb',import.meta.url),await publishedLeaf());
    await writeFile(new URL('../assets/models/leaf-variants.glb',import.meta.url),await publishedLeafVariants());
    console.log(`Published assets/models/leaf.glb and leaf-variants.glb (${LEAF_VARIANT_COUNT} shape variants).`);
  }
}
