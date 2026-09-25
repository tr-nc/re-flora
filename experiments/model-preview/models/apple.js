import * as THREE from 'three';
import {appleGeometry} from '../../../assets/models/apple-source.mjs';
import {disposeScene} from './resources.js';

const defaults={skinColor:'#cb302f',stemColor:'#74502e',leafColor:'#4d913e'};
function makeGeometry(part){
  const geometry=new THREE.BufferGeometry();
  geometry.setAttribute('position',new THREE.Float32BufferAttribute(part.positions,3));
  geometry.setIndex(part.indices);geometry.computeVertexNormals();return geometry;
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
    const materials=[skin,wood,foliage];
    const meshes=appleGeometry().map(part=>{
      const mesh=new THREE.Mesh(makeGeometry(part),materials[part.material]);mesh.name=part.name;
      scene.add(mesh);return mesh;
    });
    const key=new THREE.DirectionalLight(0xffffff,2.1);key.position.set(-3,5,6);
    scene.add(key,new THREE.AmbientLight(0xffffff,.65));
    const repairGroups=meshes.map((mesh,i)=>({id:i+1,label:['果实','果柄','小叶'][i],meshes:[mesh]}));
    return {
      scene,meshes,repairGroups,clips:[],
      view:{span:3.1,target:[0,.12,0],offset:[2.4,1.5,6],near:.1,far:40},
      description:'共用苹果造型 · 果皮、果柄、小叶 · 两种配色预设',
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
