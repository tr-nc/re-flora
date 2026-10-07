import * as THREE from 'three';
import {animalCatalog,animalGeometry,animalPose} from './animal-geometry.mjs';
import {disposeScene} from './resources.js';

export const animalDefinitions=animalCatalog.map(spec=>({
  id:spec.id,label:spec.label,
  defaults:{...spec.colors,modelScale:1,wingLength:1,wingMotion:1},
  preview:{resolution:spec.id==='kingfisher'?64:48,background:'#293c36'},
  controls:[
    {type:'note',label:spec.description+' 所有部件为不透明几何，翅膀不是 alpha 贴片；共用现有覆盖与补点。'},
    {key:'modelScale',label:'模型大小',min:.75,max:1.2,step:.01},
    {key:'wingLength',label:'翅膀长度',min:.65,max:1.15,step:.01},
    {key:'wingMotion',label:'拍翼幅度（拍翼片段）',min:.5,max:1.3,step:.01},
    ...Object.keys(spec.colors).map((key,i)=>({key,type:'color',label:spec.colorLabels?.[key]??['主色','深色 / 背部','浅色点缀','翅膀颜色','触角 / 喙足点缀'][i]})),
  ],
  async create(){
    const scene=new THREE.Scene(),root=new THREE.Group(),meshes=[],blends=[],recipe=animalGeometry(spec.id);
    const bird=spec.id==='kingfisher';
    const materials=Object.fromEntries(Object.entries({...spec.colors,eye:'#211d23',shine:'#fff7db'}).map(([key,color])=>[key,new THREE.MeshStandardMaterial({color,roughness:bird&&key==='eye'?.25:.9,side:THREE.DoubleSide,flatShading:!bird})]));
    scene.add(root);
    const groups={body:root,leftWing:new THREE.Group(),rightWing:new THREE.Group()};
    for(const key of ['leftWing','rightWing']){groups[key].position.fromArray(recipe.hinges[key]);root.add(groups[key]);}
    for(const part of recipe.meshes){
      const geometry=new THREE.BufferGeometry();geometry.setAttribute('position',new THREE.Float32BufferAttribute(part.positions,3));geometry.setIndex(part.indices);
      if(part.normals)geometry.setAttribute('normal',new THREE.Float32BufferAttribute(part.normals,3));else geometry.computeVertexNormals();
      let material=materials[part.color];
      if(part.colorBlend){
        material=material.clone();material.color.set('#ffffff');material.vertexColors=true;
        const colors=new THREE.Float32BufferAttribute(new Float32Array(part.positions.length),3);geometry.setAttribute('color',colors);
        blends.push({colors,base:part.color,...part.colorBlend});
      }
      const mesh=new THREE.Mesh(geometry,material);mesh.name=part.name;mesh.userData.animalGroup=part.group;groups[part.group].add(mesh);meshes.push(mesh);
    }
    const light=new THREE.DirectionalLight('#fff1d8',bird?1.8:2.1);light.position.set(...(bird?[4,6,5]:[-3,5,6]));scene.add(light,new THREE.AmbientLight('#e9f3ff',.8));
    const repairGroups=['body','leftWing','rightWing'].map((group,index)=>({id:index+1,label:['身体与尾足','左翅','右翅'][index],meshes:meshes.filter(mesh=>mesh.userData.animalGroup===group)}));
    let settings={...spec.colors,modelScale:1,wingLength:1,wingMotion:1};
    return {
      scene,meshes,repairGroups,view:bird?{span:3.6,target:[0,.12,.20],offset:[7,1.5,4],near:.1,far:40}:{span:4.6,target:[0,0,-.08],offset:[3,2,7],near:.1,far:40},
      clips:[{name:spec.id==='bee'?'停留姿态':'停栖姿态',duration:spec.id==='bee'?1:2},{name:'拍翼展示（非飞行控制）',duration:spec.id==='bee'?1:2}],
      description:spec.description,
      apply(values){
        settings={...settings,...values};
        root.scale.setScalar(settings.modelScale);
        for(const key of ['leftWing','rightWing']){
          if(bird)groups[key].scale.setScalar(settings.wingLength);else groups[key].scale.x=settings.wingLength;
        }
        for(const key of Object.keys(spec.colors))materials[key].color.set(settings[key]);
        for(const blend of blends){
          const color=new THREE.Color();
          for(let i=0;i<blend.weights.length;i++){
            color.copy(materials[blend.base].color).lerp(materials[blend.key].color,blend.weights[i]);blend.colors.setXYZ(i,color.r,color.g,color.b);
          }
          blend.colors.needsUpdate=true;
        }
        for(const group of repairGroups){const color=group.id===1?settings.bodyColor:settings.wingColor;group.fallbackColor=[1,3,5].map(i=>parseInt(color.slice(i,i+2),16));}
      },
      sample(time,clip){
        const pose=animalPose(spec.id,time,clip,settings.wingMotion);
        root.position.y=pose.rootY;root.rotation.x=pose.rootPitch;
        groups.leftWing.rotation.z=pose.leftWing;groups.rightWing.rotation.z=pose.rightWing;
      },
      preparePass(){},get shadows(){return false;},dispose(){disposeScene(scene);},
    };
  },
}));
