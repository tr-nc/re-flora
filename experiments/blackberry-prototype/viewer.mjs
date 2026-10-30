import * as THREE from 'three';
import {OrbitControls} from 'three/addons/controls/OrbitControls.js';
import {defaults,random} from './model.mjs';
import {blackberryMesh,blackberryColors} from '../../assets/models/blackberry-source.mjs';

const $=id=>document.getElementById(id);
const canvas=$('scene'),viewport=$('viewport');
const state={...defaults,stage:'fruit',skeleton:false,wind:!matchMedia('(prefers-reduced-motion: reduce)').matches,scale:true,pixel:false};
let renderer;
try { renderer=new THREE.WebGLRenderer({canvas,antialias:true,alpha:true}); }
catch(error) { $('error').hidden=false; $('error').textContent='无法创建 WebGL 画布。请启用浏览器硬件加速后重试；下方调研笔记仍可阅读。'; throw error; }
renderer.setPixelRatio(Math.min(devicePixelRatio,2));
renderer.outputColorSpace=THREE.SRGBColorSpace;
renderer.toneMapping=THREE.ACESFilmicToneMapping;renderer.toneMappingExposure=.9;
renderer.setClearColor(0xdfe4d2,0);
const scene=new THREE.Scene();
const camera=new THREE.OrthographicCamera(-3,3,2.5,-2.5,.05,50);
const controls=new OrbitControls(camera,canvas);
controls.enableDamping=true;controls.dampingFactor=.09;controls.enablePan=false;
controls.minZoom=.65;controls.maxZoom=3.5;controls.maxPolarAngle=Math.PI*.49;
scene.add(new THREE.HemisphereLight(0xfaf8e4,0x748466,2.5));
const sun=new THREE.DirectionalLight(0xfff0d1,2.6);sun.position.set(-3,6,4);scene.add(sun);
const fill=new THREE.DirectionalLight(0xe6efed,.7);fill.position.set(3,2,-4);scene.add(fill);
const windUniform={value:state.wind?1:0},timeUniform={value:0};
function material(color,vegetation=true) {
  const result=new THREE.MeshStandardMaterial({color,roughness:.85,flatShading:true,side:THREE.DoubleSide});
  if(vegetation) result.onBeforeCompile=shader=>{
    shader.uniforms.uTime=timeUniform;shader.uniforms.uWind=windUniform;
    shader.vertexShader='uniform float uTime; uniform float uWind;\n'+shader.vertexShader;
    shader.vertexShader=shader.vertexShader.replace('#include <begin_vertex>',`#include <begin_vertex>
      float h = max(position.y, 0.0);
      float gust = sin(uTime * 1.2 + position.x * 0.8 + position.z * 0.6);
      transformed.x += uWind * h * h * 0.018 * gust;
      transformed.z += uWind * h * 0.012 * sin(uTime * 1.7 + position.x);`);
  };
  return result;
}
const materials=Object.fromEntries(Object.entries(blackberryColors).map(([key,color])=>[key,material(color)]));
let plantGroup=new THREE.Group(),plantData,activeView='garden',revision=0;
scene.add(plantGroup);
controls.addEventListener('start',()=>{
  activeView='custom';
  for(const button of document.querySelectorAll('[data-view]'))button.setAttribute('aria-pressed','false');
});
const vector=a=>new THREE.Vector3(...a);
function clearPlant() {
  for(const child of plantGroup.children)child.geometry.dispose();
  plantGroup.clear();
}
function rebuild() {
  clearPlant();
  const mesh=blackberryMesh(THREE,state);plantData=mesh.plantData;
  let triangles=0;
  for(const part of mesh.parts) {
    const geometry=new THREE.BufferGeometry();
    geometry.setAttribute('position',new THREE.Float32BufferAttribute(part.positions,3));
    geometry.setAttribute('normal',new THREE.Float32BufferAttribute(part.normals,3));
    const object=new THREE.Mesh(geometry,materials[part.material]);object.name=part.material;plantGroup.add(object);
    triangles+=part.positions.length/9;
  }
  revision++;
  $('model-stats').textContent=`${plantData.canes.length} 根主枝 / ${state.skeleton?0:plantData.leaves.length} 簇复叶`;
  $('scene-label').textContent=state.skeleton?'骨架 · 新枝与结果枝的空间关系':({leaf:'枝叶 · 先看轮廓与空隙',flower:'花期 · 结果枝上的五瓣小花',fruit:'果期 · 同丛保留无果新枝'})[state.stage];
  $('seed-value').value=state.seed;
  window.blackberryStudy={ready:true,revision,settings:{...state},counts:{canes:plantData.canes.length,leaves:plantData.leaves.length,fruitSites:plantData.fruitSites.length,triangles},renderer:()=>({geometries:renderer.info.memory.geometries,textures:renderer.info.memory.textures}),renderSize:()=>({width:canvas.width,height:canvas.height})};
}
// A quiet display plinth and illustrative contact shadow, not a physical terrain simulation.
const groundMaterial=material('#bfc6a5',false);
const ground=new THREE.Mesh(new THREE.CylinderGeometry(2.35,2.38,.055,72),groundMaterial);ground.position.y=-.045;ground.scale.z=.83;scene.add(ground);
const rim=new THREE.Mesh(new THREE.CylinderGeometry(2.38,2.35,.045,72),material('#aeb99a',false));rim.position.y=-.09;rim.scale.z=.83;scene.add(rim);
for(let i=0;i<9;i++) {
  const shadow=new THREE.Mesh(new THREE.CircleGeometry(.45+i*.075,48),new THREE.MeshBasicMaterial({color:'#697451',transparent:true,opacity:.025,depthWrite:false}));
  shadow.rotation.x=-Math.PI/2;shadow.position.set(.16,-.012+i*.0001,-.07);shadow.scale.y=.75;scene.add(shadow);
}
const groundBits=new THREE.Group();scene.add(groundBits);
const rng=random(93),grassMat=material('#8e9e73',false);
for(let i=0;i<100;i++) {
  const a=rng()*Math.PI*2,r=.4+rng()*1.8;
  const x=Math.cos(a)*r,z=Math.sin(a)*r*.78;
  const shape=new THREE.BufferGeometry();shape.setAttribute('position',new THREE.Float32BufferAttribute([x,-.014,z,x+.025,.03+rng()*.035,z+.01,x+.017,-.014,z+.018],3));shape.computeVertexNormals();groundBits.add(new THREE.Mesh(shape,grassMat));
}
const person=new THREE.Group();person.position.set(-2.02,0,.2);scene.add(person);
const personMat=material('#939c83',false);
function humanPart(geometry,position) {const mesh=new THREE.Mesh(geometry,personMat);mesh.position.set(...position);person.add(mesh);return mesh;}
humanPart(new THREE.SphereGeometry(.10,12,8),[0,1.6,0]);
humanPart(new THREE.CylinderGeometry(.13,.105,.48,8),[0,1.22,0]);
humanPart(new THREE.CylinderGeometry(.105,.11,.22,8),[0,.9,0]);
for(const sign of [-1,1]) {
  humanPart(new THREE.CylinderGeometry(.047,.035,.75,7),[sign*.075,.43,0]).rotation.z=sign*-.06;
  humanPart(new THREE.CylinderGeometry(.037,.026,.55,7),[sign*.17,1.12,0]).rotation.z=sign*.12;
  humanPart(new THREE.BoxGeometry(.085,.06,.19),[sign*.09,.03,.035]);
}
const label=document.createElement('span');label.textContent='1.7 m';label.style.cssText='position:absolute;font:9px ui-monospace,monospace;color:#707b61;pointer-events:none';viewport.append(label);
function resize() {
  const w=viewport.clientWidth,h=viewport.clientHeight,aspect=w/h;
  const half=Math.max(1.62,2.65/aspect);
  camera.left=-half*aspect;camera.right=half*aspect;camera.top=half;camera.bottom=-half;
  if(activeView==='top')camera.zoom=Math.min(1,half/2.65);
  camera.updateProjectionMatrix();
  renderer.setPixelRatio(state.pixel?1:Math.min(devicePixelRatio,2));
  const factor=state.pixel?Math.min(1,300/w):1;
  renderer.setSize(Math.round(w*factor),Math.round(h*factor),false);
  canvas.style.imageRendering=state.pixel?'pixelated':'auto';
  $('render-label').textContent=state.pixel?'低分辨率栅格预览 / 非游戏管线':'原创低模 / 实时 3D';
}
new ResizeObserver(resize).observe(viewport);
function setView(name) {
  activeView=name;
  camera.zoom=name==='detail'?2.3:name==='top'?Math.min(1,camera.top/2.65):1;
  const target=new THREE.Vector3(0,.72,0);
  const offset={garden:[3.5,2.5,5.5],side:[5,.9,0],top:[.1,7,.05],detail:[1.4,1.3,3]}[name];
  if(name==='detail') {
    const site=[...plantData.fruitSites].sort((a,b)=>(b.base[0]+.7*b.base[2])-(a.base[0]+.7*a.base[2]))[0];
    if(site)target.copy(vector(site.base));
  }
  controls.target.copy(target);camera.position.copy(target).add(vector(offset));camera.updateProjectionMatrix();controls.update();
  for(const button of document.querySelectorAll('[data-view]'))button.setAttribute('aria-pressed',String(button.dataset.view===name));
}
function sync() {
  for(const key of ['canes','spread','density'])$(key).value=state[key];
  $('canes-value').value=state.canes;$('spread-value').value=`${state.spread.toFixed(2)}×`;$('density-value').value=`${Math.round(state.density*100)}%`;
  for(const key of ['skeleton','wind','scale','pixel'])$(key).checked=state[key];
  for(const button of document.querySelectorAll('[data-stage]'))button.setAttribute('aria-pressed',String(button.dataset.stage===state.stage));
  person.visible=state.scale;label.hidden=!state.scale;windUniform.value=state.wind?1:0;
}
for(const key of ['canes','spread','density'])$(key).addEventListener('input',()=>{state[key]=Number($(key).value);sync();rebuild();if(activeView==='detail')setView('detail');});
for(const key of ['skeleton','wind','scale','pixel'])$(key).addEventListener('change',()=>{state[key]=$(key).checked;sync();if(key==='skeleton')rebuild();else if(window.blackberryStudy)window.blackberryStudy.settings={...state};if(key==='pixel')resize();});
for(const button of document.querySelectorAll('[data-stage]'))button.addEventListener('click',()=>{state.stage=button.dataset.stage;sync();rebuild();});
for(const button of document.querySelectorAll('[data-view]'))button.addEventListener('click',()=>setView(button.dataset.view));
$('reseed').addEventListener('click',()=>{state.seed=(state.seed+17)%100001;rebuild();if(activeView==='detail')setView('detail');});
$('reset').addEventListener('click',()=>{Object.assign(state,defaults,{stage:'fruit',skeleton:false,wind:!matchMedia('(prefers-reduced-motion: reduce)').matches,scale:true,pixel:false});sync();rebuild();setView('garden');resize();});
canvas.addEventListener('webglcontextlost',event=>{event.preventDefault();$('error').hidden=false;$('error').textContent='WebGL 上下文已丢失，请重新加载页面。';});
sync();rebuild();setView('garden');resize();
let previous=0,elapsed=0;
renderer.setAnimationLoop(now=>{
  const dt=previous?Math.min((now-previous)/1000,.05):0;previous=now;
  if(state.wind&&!document.hidden)elapsed+=dt;
  timeUniform.value=elapsed;
  controls.update();renderer.render(scene,camera);
  const p=new THREE.Vector3(-2.02,1.84,.2).project(camera);
  label.style.left=`${(p.x*.5+.5)*viewport.clientWidth-13}px`;label.style.top=`${(-p.y*.5+.5)*viewport.clientHeight}px`;
  label.hidden=!state.scale||p.x<-.95||p.x>.95||Math.abs(p.y)>.95;
});
