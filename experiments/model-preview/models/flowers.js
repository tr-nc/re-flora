import * as THREE from 'three';
import {flowerCatalog,flowerGeometry} from './flower-catalog.mjs';
import {normalizeFlowerShape} from '../../../assets/models/parametric-flower.mjs';
import {maskValue,resolvePalette} from '../../../assets/models/palette-mask.mjs';
import {disposeScene} from './resources.js';

const paletteKeys=['paletteA','paletteB','paletteC','paletteD'];
export const flowerDefinitions=flowerCatalog.map(spec=>({
  id:spec.id,label:spec.label,defaults:{...spec.defaults},
  controls:[
    {type:'note',label:'共用可调花头：所有花型都是同一机制的预设。正向聚拢、0 平展、负向下垂。六种正式花与游戏共用配方；网页编辑不自动发布或保存。'},
    {key:'petalCount',label:'花瓣数量',min:3,max:24,step:1},
    {key:'flowerSize',label:'花头大小',min:.65,max:1.3,step:.01},
    {key:'petalLength',label:'花瓣长度',min:.6,max:1.4,step:.01},
    {key:'petalWidth',label:'花瓣宽度',min:.08,max:.65,step:.01},
    {key:'tipSharpness',label:'瓣尖 · 0 圆 / 1 尖',min:0,max:1,step:.01},
    {key:'notch',label:'瓣尖缺口',min:0,max:.25,step:.01},
    {key:'opening',label:'花瓣姿态 · 下垂 ↔ 聚拢',min:-1,max:1,step:.01},
    {key:'centerShape',type:'select',label:'花心形状',options:[['flat','平花盘'],['dome','圆球凸起'],['cone','锥状凸起']]},
    {key:'centerRadius',label:'花心宽度',min:.04,max:.35,step:.01},
    {key:'centerHeight',label:'花心高度（凸起时）',min:0,max:.55,step:.01},
    {key:'tilt',label:'花头仰角',min:-15,max:85,step:1},
    {type:'note',label:'贴纸只决定 A/B/C/D 的混合权重，实际颜色由下方 palette 决定。四槽没有固定部位含义；蓝白渐变不需要新颜色分区参数。'},
    ...paletteKeys.map((key,i)=>({key,type:'color',label:`调色板 ${'ABCD'[i]}`})),
  ],
  colorPresets:[
    {name:'蓝白',colors:{paletteA:'#488cdf',paletteB:'#fbfcff',paletteC:'#f4ce67',paletteD:'#5c864d'}},
    {name:'莓粉',colors:{paletteA:'#a53d85',paletteB:'#ffd8e5',paletteC:'#eeb34d',paletteD:'#597d43'}},
    {name:'暮紫',colors:{paletteA:'#604ca2',paletteB:'#c8bdff',paletteC:'#f7dfb1',paletteD:'#46695c'}},
  ],
  preview:{resolution:32,background:'#293c36'},
  async create(){
    const scene=new THREE.Scene(),meshes=[],repairGroups=[],pixelParts=[];
    const texture=new THREE.DataTexture(new Uint8Array(4),1,1);texture.colorSpace=THREE.SRGBColorSpace;
    texture.minFilter=texture.magFilter=THREE.LinearFilter;texture.generateMipmaps=false;
    const material=new THREE.MeshStandardMaterial({map:texture,side:THREE.DoubleSide,roughness:.95});
    const light=new THREE.DirectionalLight('#fff5df',2.2);light.position.set(-3,5,6);
    scene.add(light,new THREE.AmbientLight('#f1f5ff',.85));
    let shapeKey='',paletteKey='',previousMask;
    return {
      scene,meshes,repairGroups,pixelParts,clips:[],view:{span:2,target:[0,0,0],offset:[.65,1,6],near:.1,far:40},
      description:`${spec.latin} · 共用参数化花头 / palette 权重贴纸 · 仅网页`,
      apply(settings){
        const nextPalette=JSON.stringify(paletteKeys.map(key=>settings[key]));
        if(previousMask!==settings.weightMap||paletteKey!==nextPalette){
          const mask=maskValue(settings.weightMap);
          texture.image={data:resolvePalette(mask,paletteKeys.map(key=>settings[key])),width:mask.width,height:mask.height};texture.needsUpdate=true;
          previousMask=settings.weightMap;paletteKey=nextPalette;
        }
        const nextKey=JSON.stringify(normalizeFlowerShape(settings));
        if(nextKey!==shapeKey){
          for(const mesh of meshes){scene.remove(mesh);mesh.geometry.dispose();}
          meshes.length=repairGroups.length=pixelParts.length=0;
          for(const part of flowerGeometry(spec.id,settings).parts){
            const geometry=new THREE.BufferGeometry();
            geometry.setAttribute('position',new THREE.Float32BufferAttribute(part.positions,3));
            geometry.setAttribute('uv',new THREE.Float32BufferAttribute(part.uvs,2));geometry.setIndex(part.indices);geometry.computeVertexNormals();
            const mesh=new THREE.Mesh(geometry,material);mesh.name=part.name;mesh.userData.head=0;scene.add(mesh);meshes.push(mesh);
          }
          const box=new THREE.Box3();for(const mesh of meshes)box.expandByObject(mesh);
          const sphere=box.getBoundingSphere(new THREE.Sphere());
          pixelParts.push({id:2,label:'完整花头',meshes:[...meshes],center:sphere.center,span:sphere.radius*2.12,anchor:[0,0,0]});
          repairGroups.push({id:2,label:'完整花头',meshes:[...meshes]});shapeKey=nextKey;
        }
        repairGroups[0].fallbackColor=[1,3,5].map(i=>parseInt(settings.paletteA.slice(i,i+2),16));
      },
      sample(){},preparePass(){},get shadows(){return false;},
      dispose(){disposeScene(scene);material.dispose();texture.dispose();},
    };
  },
}));
