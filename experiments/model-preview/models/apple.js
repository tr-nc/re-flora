import * as THREE from 'three';
import {disposeScene} from './resources.js';

const defaults={skinColor:'#cb302f',stemColor:'#74502e',leafColor:'#4d913e'};

// A shallow five-lobed shoulder surrounds the recessed stem socket. The bottom
// has a smaller indentation, so this reads as an apple rather than a sphere.
function bodyGeometry(){
  const segments=24,rings=16,positions=[],indices=[];
  positions.push(0,.86-.25,0);
  for(let ring=1;ring<rings;ring++){
    const theta=Math.PI*ring/rings,vertical=Math.cos(theta),r=Math.pow(Math.sin(theta),.82);
    for(let segment=0;segment<segments;segment++){
      const angle=2*Math.PI*segment/segments,lobes=Math.cos(5*angle);
      const radius=r*(1+.09*vertical+.045*lobes*Math.max(vertical,0)**4);
      const shoulder=.045*lobes*r*Math.max(vertical,0)**4;
      const topDent=.25*Math.exp(-((r/.38)**2))*Math.max(vertical,0)**2;
      const bottomDent=.11*Math.exp(-((r/.3)**2))*Math.max(-vertical,0)**2;
      positions.push(radius*Math.cos(angle),.86*vertical+shoulder-topDent+bottomDent,radius*Math.sin(angle));
    }
  }
  const bottom=positions.length/3;positions.push(0,-.86+.11,0);
  for(let segment=0;segment<segments;segment++){
    const next=(segment+1)%segments;
    indices.push(0,1+next,1+segment);
    for(let ring=0;ring<rings-2;ring++){
      const a=1+ring*segments+segment,b=1+ring*segments+next;
      indices.push(a,b,a+segments,b,b+segments,a+segments);
    }
    indices.push(bottom,bottom-segments+segment,bottom-segments+next);
  }
  const geometry=new THREE.BufferGeometry();
  geometry.setAttribute('position',new THREE.Float32BufferAttribute(positions,3));
  geometry.setIndex(indices);geometry.computeVertexNormals();
  return geometry;
}

function leafGeometry(){
  const positions=[],indices=[],steps=5;
  for(let i=0;i<=steps;i++){
    const t=i/steps,width=.14*Math.sin(Math.PI*t);
    const x=.055+.62*t,y=.91+.21*Math.sin(Math.PI*t)+.08*t,z=.04+.24*t;
    positions.push(x,y,z-width,x,y,z+width);
    if(i<steps){const a=2*i;indices.push(a,a+1,a+2,a+1,a+3,a+2);}
  }
  const geometry=new THREE.BufferGeometry();geometry.setAttribute('position',new THREE.Float32BufferAttribute(positions,3));
  geometry.setIndex(indices);geometry.computeVertexNormals();return geometry;
}

export const appleDefinition={
  id:'apple',label:'苹果',defaults,
  controls:[
    {key:'skinColor',label:'果皮颜色',type:'color'},
    {key:'stemColor',label:'果柄颜色',type:'color'},
    {key:'leafColor',label:'小叶颜色',type:'color'},
  ],
  colorPresets:[
    {name:'红苹果',colors:{skinColor:'#cb302f',stemColor:'#74502e',leafColor:'#4d913e'}},
    {name:'青苹果',colors:{skinColor:'#9cbb36',stemColor:'#715332',leafColor:'#468a36'}},
  ],
  preview:{resolution:32,background:'#303b3a'},
  async create(){
    const scene=new THREE.Scene(),skin=new THREE.MeshStandardMaterial({roughness:.62,metalness:0}),
      wood=new THREE.MeshStandardMaterial({roughness:.94}),
      foliage=new THREE.MeshStandardMaterial({side:THREE.DoubleSide,roughness:.85});
    const body=new THREE.Mesh(bodyGeometry(),skin);body.name='Apple';
    const stem=new THREE.Mesh(new THREE.CylinderGeometry(.027,.046,.43,10),wood);
    stem.name='Stem';stem.position.set(.025,.81,0);stem.rotation.z=-.13;
    const leaf=new THREE.Mesh(leafGeometry(),foliage);leaf.name='Apple leaf';
    const key=new THREE.DirectionalLight(0xffffff,2.1);key.position.set(-3,5,6);
    scene.add(body,stem,leaf,key,new THREE.AmbientLight(0xffffff,.65));
    const meshes=[body,stem,leaf];
    const repairGroups=meshes.map((mesh,i)=>({id:i+1,label:['果实','果柄','小叶'][i],meshes:[mesh]}));
    return {
      scene,meshes,repairGroups,clips:[],
      view:{span:3.1,target:[0,.12,0],offset:[2.4,1.5,6],near:.1,far:40},
      description:'预览专用苹果 · 果皮、果柄、小叶 · 两种配色预设（不进入游戏）',
      apply(values){
        for(const [material,key,group] of [[skin,'skinColor',0],[wood,'stemColor',1],[foliage,'leafColor',2]]){
          material.color.set(values[key]);
          repairGroups[group].fallbackColor=[1,3,5].map(i=>parseInt(values[key].slice(i,i+2),16));
        }
      },
      sample(){},preparePass(){},get shadows(){return false;},
      dispose(){disposeScene(scene);},
    };
  },
};
