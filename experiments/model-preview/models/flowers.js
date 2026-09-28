import * as THREE from 'three';
import {flowerCatalog,flowerGeometry} from './flower-catalog.mjs';
import {disposeScene} from './resources.js';
import {completeFlowerHead} from '../../../assets/models/flower-head.mjs';

const colorKeys=['petalColor','innerColor','centerColor','stemColor'];
export const flowerDefinitions=flowerCatalog.map(spec=>({
  id:spec.id,label:spec.label,defaults:Object.fromEntries(Object.entries(spec.defaults).filter(([key])=>!['height','bend','leafSize','leafColor'].includes(key))),
  controls:[
    {type:'note',label:'仅展示完整花头（花瓣、花心、花萼）及后处理；茎生成、整株拼装和风动由游戏负责。网页参数不保存。'},
    {key:'flowerSize',label:'花头大小',min:.65,max:1.3,step:.01},
    {key:'opening',label:spec.id==='coneflower'?'花瓣下垂':spec.kind==='radial'?'花瓣起伏':'花冠张开',min:.6,max:1.35,step:.01},
    {key:'tilt',label:'花头仰角',min:-15,max:85,step:1},
    ...colorKeys.map((key,i)=>({key,type:'color',label:['花瓣颜色','瓣根 / 喉部颜色','花心颜色','花萼颜色'][i]})),
  ],
  preview:{resolution:32,background:'#293c36'},
  async create(){
    const scene=new THREE.Scene(),meshes=[],repairGroups=[],pixelParts=[];
    const materials=Object.fromEntries(colorKeys.map(key=>[key,new THREE.MeshStandardMaterial({side:THREE.DoubleSide,roughness:.95,flatShading:true})]));
    const light=new THREE.DirectionalLight('#fff5df',2.2);light.position.set(-3,5,6);
    scene.add(light,new THREE.AmbientLight('#f1f5ff',.85));
    let shapeKey='';
    return {
      scene,meshes,repairGroups,pixelParts,clips:[],
      view:{span:2,target:[0,0,0],offset:[.65,1,6],near:.1,far:40},
      description:`${spec.latin} · 完整花头模型与后处理预览`,
      apply(settings){
        for(const key of colorKeys)materials[key].color.set(settings[key]);
        const nextKey=JSON.stringify(Object.fromEntries(Object.entries(settings).filter(([key])=>!colorKeys.includes(key))));
        if(nextKey!==shapeKey){
          for(const mesh of meshes){scene.remove(mesh);mesh.geometry.dispose();}
          meshes.length=repairGroups.length=pixelParts.length=0;
          const recipe=completeFlowerHead(flowerGeometry(spec.id,settings));
          for(const part of recipe.parts){
            const geometry=new THREE.BufferGeometry();
            geometry.setAttribute('position',new THREE.Float32BufferAttribute(part.positions,3));geometry.setIndex(part.indices);geometry.computeVertexNormals();
            const mesh=new THREE.Mesh(geometry,materials[part.material]);mesh.name=part.name;mesh.userData.head=part.head;
            scene.add(mesh);meshes.push(mesh);
          }
          for(const head of recipe.heads){
            const members=meshes.filter(mesh=>mesh.userData.head===head.id);
            const box=new THREE.Box3();for(const mesh of members)box.expandByObject(mesh);
            const sphere=box.getBoundingSphere(new THREE.Sphere());
            pixelParts.push({id:head.id+2,label:head.label,meshes:members,center:sphere.center,span:sphere.radius*2.12,anchor:head.anchor});
            repairGroups.push({id:head.id+2,label:head.label,meshes:members});
          }
          shapeKey=nextKey;
        }
        for(const group of repairGroups){
          const hex=settings.petalColor;
          group.fallbackColor=[1,3,5].map(i=>parseInt(hex.slice(i,i+2),16));
        }
      },
      sample(){},preparePass(){},get shadows(){return false;},
      dispose(){disposeScene(scene);for(const material of Object.values(materials))material.dispose();},
    };
  },
}));
