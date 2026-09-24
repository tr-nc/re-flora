import * as THREE from 'three';
import {GLTFLoader} from 'three/addons/loaders/GLTFLoader.js';
import {disposeScene} from './resources.js';

export const butterflyDefinition={
  id:'butterfly',label:'蝴蝶',
  defaults:{color:'#50bedc',shadows:true},
  controls:[{key:'color',label:'蝴蝶颜色',type:'color'},{key:'shadows',label:'光照 / 自阴影',type:'checkbox'}],
  preview:{resolution:12,background:'#253039'},
  async create(){
    const gltf=await new GLTFLoader().loadAsync(new URL('../../../assets/models/butterfly.glb',import.meta.url).href);
    const scene=new THREE.Scene();scene.add(gltf.scene);
    try{
      const meshes=[],materials=new Set();
      gltf.scene.traverse(object=>{
        if(/head|thorax|abdomen|body/i.test(object.name))throw new Error('批准的蝴蝶源不应包含身体节点');
        if(!object.isMesh)return;
        object.castShadow=object.receiveShadow=true;meshes.push(object);
        for(const material of Array.isArray(object.material)?object.material:[object.material]){
          if(!['Wing upper','Wing lower'].includes(material.name))throw new Error('未识别的翼面材质');
          materials.add(material);
        }
      });
      const key=new THREE.DirectionalLight(0xffffff,2.4),fill=new THREE.AmbientLight(0xffffff,.65);
      key.position.set(3.8,5,2.8);key.shadow.mapSize.set(1024,1024);
      Object.assign(key.shadow.camera,{left:-2,right:2,top:2,bottom:-2,near:.1,far:15});
      key.shadow.camera.updateProjectionMatrix();key.shadow.bias=-.0003;key.shadow.normalBias=.015;
      scene.add(key,key.target,fill);
      const mixer=new THREE.AnimationMixer(gltf.scene);
      const actions=gltf.animations.map(clip=>mixer.clipAction(clip));let selected=-1,shadows=true;
      const repairGroups=['L_','R_'].map((prefix,index)=>({id:index+1,label:index?'右翼':'左翼',meshes:meshes.filter(mesh=>mesh.name.startsWith(prefix))}));
      if(!gltf.animations.length||repairGroups.some(group=>!group.meshes.length))throw new Error('蝴蝶源缺少动画或左右翼');
      const distance=3.4/(2*Math.tan(THREE.MathUtils.degToRad(35/2)));
      return {
        scene,meshes,repairGroups,
        clips:gltf.animations.map(clip=>({name:clip.name,duration:clip.duration})),
        view:{span:3.4,target:[0,0,0],offset:[-.3535533906,.8660254038,-.3535533906].map(v=>v*distance)},
        description:'双翼模型 · 原始动画 / 材质 · 左右翼独立补点',
        apply(settings){
          shadows=settings.shadows;
          for(const group of repairGroups)group.fallbackColor=[1,3,5].map(i=>parseInt(settings.color.slice(i,i+2),16));
          for(const material of materials){
            material.color.set(shadows?settings.color:'#000000');material.emissive.set(shadows?'#000000':settings.color);
            material.emissiveIntensity=1;material.metalness=0;material.roughness=1;
          }
          key.intensity=shadows?2.4:0;fill.intensity=shadows?.65:0;key.castShadow=shadows;
        },
        sample(time,clip=0){
          if(selected!==clip){mixer.stopAllAction();actions[clip].reset().play();selected=clip;}
          mixer.setTime(time);
        },
        preparePass(){},
        get shadows(){return shadows;},
        dispose(){mixer.stopAllAction();mixer.uncacheRoot(gltf.scene);disposeScene(scene);},
      };
    }catch(error){disposeScene(scene);throw error;}
  },
};
