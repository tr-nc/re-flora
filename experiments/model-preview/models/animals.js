import * as THREE from 'three';
import {animalCatalog,animalGeometry,animalPose} from './animal-geometry.mjs';
import {createKingfisherPlumage,resolveKingfisherPlumage} from './kingfisher-plumage.mjs';
import {disposeScene} from './resources.js';

export const animalDefinitions=animalCatalog.map(spec=>({
  id:spec.id,label:spec.label,
  defaults:{...spec.colors,modelScale:1,wingLength:1,wingMotion:1},
  preview:{resolution:spec.id==='kingfisher'?96:48,background:'#293c36'},
  controls:[
    {type:'note',label:spec.description+' 所有部件为不透明几何，翅膀不是 alpha 贴片；共用现有覆盖与补点。'},
    {key:'modelScale',label:'模型大小',min:.75,max:1.2,step:.01},
    {key:'wingLength',label:'翅膀长度',min:.65,max:1.15,step:.01},
    {key:'wingMotion',label:'拍翼幅度（拍翼片段）',min:.5,max:1.3,step:.01},
    ...Object.keys(spec.colors).map((key,i)=>({key,type:'color',label:spec.colorLabels?.[key]??['主色','深色 / 背部','浅色点缀','翅膀颜色','触角 / 喙足点缀'][i]})),
  ],
  async create(){
    const scene=new THREE.Scene(),root=new THREE.Group(),meshes=[],recipe=animalGeometry(spec.id),bird=spec.id==='kingfisher';
    const solidKeys=new Set(recipe.meshes.filter(part=>!part.appearance).map(part=>part.color));
    const colors={...spec.colors,eye:'#17101a',shine:'#fff7db',...(bird?{iris:'#64372b',billSeam:'#a8300c'}:{eye:'#211d23'})};
    const materials=Object.fromEntries(Object.entries(colors).filter(([key])=>solidKeys.has(key)).map(([key,color])=>[key,new THREE.MeshStandardMaterial({color,roughness:bird&&key==='eye'?.35:.9,side:THREE.DoubleSide,flatShading:!bird})]));
    const masks=bird?createKingfisherPlumage():{},maps={},plumageMaterials={};
    for(const [key,mask]of Object.entries(masks)){
      const texture=new THREE.DataTexture(resolveKingfisherPlumage(mask,spec.colors),mask.size,mask.size);
      texture.colorSpace=THREE.SRGBColorSpace;texture.generateMipmaps=true;
      texture.minFilter=THREE.LinearMipmapLinearFilter;texture.magFilter=THREE.LinearFilter;
      if(['head','body'].includes(key))texture.wrapS=THREE.RepeatWrapping;
      texture.needsUpdate=true;maps[key]=texture;
      const material=new THREE.MeshStandardMaterial({color:'#ffffff',map:texture,roughness:.88,side:THREE.DoubleSide});
      if(['head','body','wing'].includes(key)){
        const heights=Uint8Array.from(mask.shades,value=>Math.round(Math.max(0,Math.min(1,value))*255));
        const bump=new THREE.DataTexture(heights,mask.size,mask.size,THREE.RedFormat);
        bump.generateMipmaps=true;bump.minFilter=THREE.LinearMipmapLinearFilter;bump.magFilter=THREE.LinearFilter;
        bump.wrapS=texture.wrapS;bump.needsUpdate=true;material.bumpMap=bump;material.bumpScale=key==='body'?.012:.008;
      }
      plumageMaterials[key]=material;
    }
    scene.add(root);
    const groups={body:root,leftWing:new THREE.Group(),rightWing:new THREE.Group()};
    for(const key of ['leftWing','rightWing']){groups[key].position.fromArray(recipe.hinges[key]);root.add(groups[key]);}
    for(const part of recipe.meshes){
      const geometry=new THREE.BufferGeometry();geometry.setAttribute('position',new THREE.Float32BufferAttribute(part.positions,3));geometry.setIndex(part.indices);
      if(part.normals)geometry.setAttribute('normal',new THREE.Float32BufferAttribute(part.normals,3));else geometry.computeVertexNormals();
      if(part.uvs)geometry.setAttribute('uv',new THREE.Float32BufferAttribute(part.uvs,2));
      const mesh=new THREE.Mesh(geometry,part.appearance?plumageMaterials[part.appearance]:materials[part.color]);
      mesh.name=part.name;mesh.userData.animalGroup=part.group;groups[part.group].add(mesh);meshes.push(mesh);
    }
    // A neutral near-camera key keeps the photo's bright red / lemon / pink
    // readable; the previous side key made all three into dull ochre / mauve.
    const light=new THREE.DirectionalLight(bird?'#fffaf0':'#fff1d8',bird?2.8:2.1);
    light.position.set(...(bird?[6,4,5]:[-3,5,6]));scene.add(light,new THREE.AmbientLight(bird?'#ffffff':'#e9f3ff',bird?1.05:.8));
    const repairGroups=['body','leftWing','rightWing'].map((group,index)=>({id:index+1,label:['身体与尾足','左翅','右翅'][index],meshes:meshes.filter(mesh=>mesh.userData.animalGroup===group)}));
    let settings={...spec.colors,modelScale:1,wingLength:1,wingMotion:1},paletteKey=Object.values(spec.colors).join(':');
    return {
      scene,meshes,repairGroups,view:bird?{span:6,target:[0,.02,.18],offset:[10,.65,2.1],near:.1,far:40}:{span:4.6,target:[0,0,-.08],offset:[3,2,7],near:.1,far:40},
      clips:[{name:bird?'停栖姿态':'停留姿态',duration:bird?2:1},{name:'拍翼展示（非飞行控制）',duration:bird?2:1}],
      description:spec.description,
      apply(values){
        settings={...settings,...values};root.scale.setScalar(settings.modelScale);
        for(const key of ['leftWing','rightWing']){if(bird)groups[key].scale.setScalar(settings.wingLength);else groups[key].scale.x=settings.wingLength;}
        for(const key of Object.keys(spec.colors))materials[key]?.color.set(settings[key]);
        if(bird)materials.billSeam.color.copy(materials.accentColor.color).multiplyScalar(.38);
        const next=Object.keys(spec.colors).map(key=>settings[key]).join(':');
        if(next!==paletteKey){
          for(const [key,mask]of Object.entries(masks)){resolveKingfisherPlumage(mask,settings,maps[key].image.data);maps[key].needsUpdate=true;}
          paletteKey=next;
        }
        for(const group of repairGroups){const color=group.id===1?settings.bodyColor:settings.wingColor;group.fallbackColor=[1,3,5].map(i=>parseInt(color.slice(i,i+2),16));}
      },
      sample(time,clip){
        const pose=animalPose(spec.id,time,clip,settings.wingMotion);root.position.y=pose.rootY;root.rotation.x=pose.rootPitch;
        groups.leftWing.rotation.z=pose.leftWing;groups.rightWing.rotation.z=pose.rightWing;
      },
      preparePass(){},get shadows(){return false;},dispose(){disposeScene(scene);},
    };
  },
}));
