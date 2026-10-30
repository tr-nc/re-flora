// Adapt the approved root-local browser mesh to the existing plant surface format.
import * as THREE from '../experiments/butterfly-method-comparison/vendor/three-0.183.0/build/three.module.js';
import {blackberryMesh,blackberryColors} from '../assets/models/blackberry-source.mjs';

// The walking body is 24 terrain voxels tall; the HTML reference person is 1.7 m.
// The shared plant renderer uses 10 voxels per authored unit.
export const BLACKBERRY_AUTHORING_SCALE=24/1.7/10;
export function publishedBlackberry() {
  const mesh=blackberryMesh(THREE);
  const colors=Object.entries(blackberryColors);
  const rgb=new Array(32*32*3);
  for(let y=0;y<32;y++)for(let x=0;x<32;x++) {
    const color=colors[Math.floor(y/8)*4+Math.floor(x/8)][1];
    for(let c=0;c<3;c++)rgb[(y*32+x)*3+c]=parseInt(color.slice(1+c*2,3+c*2),16);
  }
  const parts=mesh.parts.map(part=>{
    const slot=colors.findIndex(([key])=>key===part.material);
    const uv=[(slot%4+.5)/4,(Math.floor(slot/4)+.5)/4];
    const positions=[],uvs=[],indices=[],vertices=new Map();
    for(let i=0;i<part.positions.length;i+=3) {
      const p=part.positions.slice(i,i+3).map(v=>v*BLACKBERRY_AUTHORING_SCALE);
      const key=p.join(',');
      if(!vertices.has(key)) {vertices.set(key,positions.length/3);positions.push(...p);uvs.push(...uv);}
      indices.push(vertices.get(key));
    }
    return {head:0,material:'palette',positions,uvs,indices};
  });
  return {id:'blackberry',kind:'shrub',display_name:'Blackberry',stem_layers:0,cache_family:'blackberry',
    palette:['old','leaf2','red','ripe'].map(key=>[1,3,5].map(i=>parseInt(blackberryColors[key].slice(i,i+2),16))),
    color_texture:{width:32,height:32,rgb},socket_normal:[0,1,0],heads:[{id:0,anchor:[0,0,0]}],parts};
}
