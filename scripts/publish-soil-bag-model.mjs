#!/usr/bin/env node
// Two immutable snapshots of the shared recipe, not a second mesh/animation author.
import {readFile,writeFile} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {crc32} from 'node:zlib';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import * as THREE from '../experiments/butterfly-method-comparison/vendor/three-0.183.0/build/three.module.js';
import {soilBagClips,soilBagDefaults,soilBagGeometry,soilBagPose} from '../assets/models/soil-bag-source.mjs';
import {soilBagStickerArt} from '../assets/models/soil-bag-sticker-art.mjs';

export const soilBagSourceFiles=['assets/models/soil-bag-source.mjs','assets/models/soil-bag-sticker-art.mjs','scripts/publish-soil-bag-model.mjs'];
const rounded=value=>Number(value.toFixed(9));
const roundArray=values=>Array.from(values,rounded);
const root=new URL('../',import.meta.url);
export async function soilBagFingerprint(){
  return crc32(Buffer.concat(await Promise.all(soilBagSourceFiles.map(file=>readFile(new URL(file,root))))));
}
export function soilBagSnapshots(){
  return [soilBagPose(0,0),soilBagPose(soilBagClips[1].duration*.5,1)].map((pose,index)=>{
    const recipe=soilBagGeometry({opening:pose.opening,fullness:soilBagDefaults.fullness*(.75+.25*pose.fill),labelScale:soilBagDefaults.labelScale,fill:pose.fill});
    // Same BufferGeometry normals and tab transform as the browser adapter.
    const tab=new THREE.Matrix4().makeRotationZ(pose.tabAngle).setPosition(...recipe.tabHinge);
    const parts=recipe.parts.map(part=>{
      const mesh=new THREE.BufferGeometry();mesh.setAttribute('position',new THREE.Float32BufferAttribute(part.positions,3));mesh.setIndex(part.indices);mesh.computeVertexNormals();
      if(part.group==='tab')mesh.applyMatrix4(tab);
      const out={material:part.material,positions:roundArray(mesh.attributes.position.array),normals:roundArray(mesh.attributes.normal.array),uvs:part.uvs,indices:part.indices};mesh.dispose();return out;
    });
    return {id:index===0?'hover':'pour',angle:pose.angle,mouth:recipe.mouthLocal,parts};
  });
}
export function soilBagStickerSvg(){
  const art=soilBagStickerArt(soilBagDefaults.stickerDesign);
  const colors={ink:soilBagDefaults.inkColor,background:soilBagDefaults.labelColor};
  const commands={moveTo:'M',lineTo:'L',bezierCurveTo:'C',closePath:'Z'};
  const shapes=art.shapes.map(s=>{
    const fill=`fill="${colors[s.fill]}"`;
    if(s.kind==='path')return `<path ${fill} d="${s.commands.map(([name,...v])=>commands[name]+v.join(' ')).join(' ')}"/>`;
    if(s.kind==='ellipse')return `<ellipse ${fill} cx="${s.x}" cy="${s.y}" rx="${s.rx}" ry="${s.ry}" transform="rotate(${s.rotation*180/Math.PI} ${s.x} ${s.y})"/>`;
    return `<rect ${fill} x="${s.x}" y="${s.y}" width="${s.width}" height="${s.height}"/>`;
  }).join('');
  return `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="640" viewBox="0 0 ${art.viewBox.width} ${art.viewBox.height}"><rect width="80" height="100" fill="${colors.background}"/>${shapes}</svg>`;
}
export async function publishedSoilBag(textureCrc){
  const color=hex=>new THREE.Color(hex);
  const bag=color(soilBagDefaults.bagColor),label=color(soilBagDefaults.labelColor);
  return JSON.stringify({source_crc32:await soilBagFingerprint(),texture_crc32:textureCrc,texture_size:[512,640],colors:[bag,bag.clone().multiplyScalar(.76),label.clone().multiplyScalar(.43),color('#614331')].map(c=>roundArray(c.toArray())),states:soilBagSnapshots()})+'\n';
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const usage='Usage: node scripts/publish-soil-bag-model.mjs\nPublishes assets/models/soil-bag.json and soil-bag-sticker.png from shared sources.\nRequires ImageMagick (magick; override with MAGICK_EXECUTABLE). No runtime/build dependency.';
  if(process.argv.length!==2){console[process.argv[2]==='--help'?'log':'error'](usage);if(process.argv[2]!=='--help')process.exitCode=2;}
  else{
    // Rasterize continuous vectors at the ORIGINAL texture size. This is source
    // artwork antialiasing, never a coarse mosaic or the model pixel pipeline.
    const png=execFileSync(process.env.MAGICK_EXECUTABLE||'magick',['-background',soilBagDefaults.labelColor,'svg:-','-alpha','off','-strip','PNG24:-'],{input:soilBagStickerSvg(),maxBuffer:8*1024*1024});
    await writeFile(new URL('assets/models/soil-bag-sticker.png',root),png);
    await writeFile(new URL('assets/models/soil-bag.json',root),await publishedSoilBag(crc32(png)));
    console.log('Published assets/models/{soil-bag.json,soil-bag-sticker.png}');
  }
}
