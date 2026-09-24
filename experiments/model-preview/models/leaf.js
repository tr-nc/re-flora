import * as THREE from 'three';
import {disposeScene} from './resources.js';
import {GLTFLoader} from 'three/addons/loaders/GLTFLoader.js';
import {leafDefaults,leafGeometry} from '../../../assets/models/leaf-source.mjs';

export const leafDefinition={
  id:'leaf',label:'落叶',
  defaults:leafDefaults,
  controls:[
    {key:'width',label:'叶片宽度',min:.55,max:1.45,step:.01},
    {key:'widestPoint',label:'最宽处位置（叶柄 0 → 叶尖 1）',min:0,max:1,step:.01},
    {key:'fold',label:'主脉折起',min:0,max:.8,step:.01},
    {key:'curl',label:'叶尖卷曲',min:-1.8,max:1.8,step:.01},
    {key:'season',label:'叶色：青绿 → 秋黄',min:0,max:1,step:.01},
    {key:'veins',label:'叶脉强度',min:0,max:1,step:.01},
    {key:'light',label:'光源方位',min:-180,max:180,step:1},
    {key:'transmission',label:'薄叶透光',min:0,max:1,step:.01},
    {key:'steps',label:'叶片受光色阶（0 = 连续）',min:0,max:12,step:1},
  ],
  preview:{resolution:32,background:'#253426'},
  async create(){
    const gltf=await new GLTFLoader().loadAsync(new URL('../../../assets/models/leaf.glb',import.meta.url).href);
    const scene=new THREE.Scene(),settings={...this.defaults};
    const uniforms={season:{value:.25},veins:{value:.5},transmission:{value:.45},steps:{value:0},lightDirection:{value:new THREE.Vector3()}};
    const material=new THREE.ShaderMaterial({
      side:THREE.DoubleSide,uniforms,
      vertexShader:`
        varying vec2 leafUV; varying vec3 worldNormal; varying vec3 worldPosition;
        void main(){
          leafUV=uv; worldNormal=normalize(mat3(modelMatrix)*normal);
          worldPosition=(modelMatrix*vec4(position,1.)).xyz;
          gl_Position=projectionMatrix*modelViewMatrix*vec4(position,1.);
        }`,
      fragmentShader:`
        uniform float season,veins,transmission,steps; uniform vec3 lightDirection;
        varying vec2 leafUV; varying vec3 worldNormal; varying vec3 worldPosition;
        float hash(vec2 p){return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453);}
        float noise(vec2 p){
          vec2 i=floor(p),f=fract(p);f=f*f*(3.-2.*f);
          return mix(mix(hash(i),hash(i+vec2(1,0)),f.x),mix(hash(i+vec2(0,1)),hash(i+vec2(1,1)),f.x),f.y);
        }
        void main(){
          bool stem=leafUV.x>1.5;vec2 uv=leafUV;
          float lateral=abs(uv.x-.5)*2.;float mottling=noise(uv*vec2(13.,19.));
          vec3 green=vec3(.105,.24,.026),ochre=vec3(.57,.225,.023);
          vec3 base=mix(green,ochre,clamp(season+(mottling-.5)*.15,0.,1.));
          base*=.87+.19*mottling;
          float branchPhase=uv.y*7.-lateral*1.45;
          float branchDistance=abs(fract(branchPhase+.5)-.5);
          float branchAA=max(fwidth(branchPhase)*.65,.012);
          float branches=(1.-smoothstep(.015,.015+branchAA,branchDistance))
            *smoothstep(.03,.12,lateral)*(1.-smoothstep(.73,1.,lateral));
          float midrib=1.-smoothstep(.012,.012+max(fwidth(uv.x),.003),abs(uv.x-.5));
          base=mix(base,base*1.45+vec3(.025,.025,.003),veins*max(midrib,branches*.6));
          base*=1.-.16*smoothstep(.83,1.,lateral);
          if(!gl_FrontFacing)base=mix(base,vec3(.32,.36,.13),.30);
          if(stem)base=mix(vec3(.19,.23,.055),vec3(.29,.13,.033),season);
          vec3 n=normalize(worldNormal)*(gl_FrontFacing?1.:-1.);
          vec3 l=normalize(lightDirection);
          float diffuse=max(dot(n,l),0.);
          float through=max(dot(-n,l),0.)*transmission*(stem?.15:.75);
          float illumination=.38+.76*diffuse+through;
          if(steps>.5)illumination=floor(illumination*steps+.5)/steps;
          vec3 color=base*illumination;
          vec3 viewDirection=normalize(cameraPosition-worldPosition);
          float sheen=pow(max(dot(n,normalize(l+viewDirection)),0.),24.)*.035*diffuse;
          color+=vec3(sheen);color+=base*vec3(.14,.07,0.)*through;
          gl_FragColor=vec4(color,1.);
          #include <colorspace_fragment>
        }`,
    });
    const mesh=gltf.scene.getObjectByName('Leaf');mesh.material.dispose();mesh.material=material;scene.add(gltf.scene);
    return {
      scene,meshes:[mesh],repairGroups:[{id:1,label:'叶片与叶柄',meshes:[mesh]}],clips:[],
      view:{span:3.4,target:[0,-.06,.05],offset:[2.3,1.1,6],axisDistance:6,near:.1,far:40},
      description:'共享正式叶片 · 32 面 · 造型滑杆仅临时预览，发布后才同步游戏',
      apply(values){
        const rebuild=['width','widestPoint','fold','curl'].some(key=>settings[key]!==values[key]);
        Object.assign(settings,values);
        if(rebuild){mesh.geometry.dispose();mesh.geometry=makeGeometry(settings);}
        for(const key of ['season','veins','transmission'])uniforms[key].value=settings[key];
        const angle=THREE.MathUtils.degToRad(settings.light);
        uniforms.lightDirection.value.set(Math.sin(angle),.65,Math.cos(angle)).normalize();
      },
      sample(){},
      preparePass(pass){uniforms.steps.value=pass==='source'?0:settings.steps;},
      get shadows(){return false;},
      dispose(){disposeScene(scene);},
    };
  },
};

function makeGeometry(state){
  const data=leafGeometry(state),geometry=new THREE.BufferGeometry();
  geometry.setAttribute('position',new THREE.BufferAttribute(data.positions,3));
  geometry.setAttribute('normal',new THREE.BufferAttribute(data.normals,3));
  geometry.setAttribute('uv',new THREE.BufferAttribute(data.uvs,2));
  geometry.setIndex(new THREE.BufferAttribute(data.indices,1));return geometry;
}
