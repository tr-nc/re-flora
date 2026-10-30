import * as THREE from 'three';
import {OrbitControls} from 'three/addons/controls/OrbitControls.js';
import {mergeGeometries} from 'three/addons/utils/BufferGeometryUtils.js';
import {defaults,makePlant,point,add,mul,random} from './model.mjs';

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
const materials={
  old:material('#765347'),fresh:material('#6e7f46'),twig:material('#7e7251'),prickle:material('#af8470'),
  leaf0:material('#344f35'),leaf1:material('#49653a'),leaf2:material('#627d43'),leaf3:material('#72854b'),leaf4:material('#597444'),
  green:material('#819252'),red:material('#9d454c'),ripe:material('#302637'),ripeLight:material('#514255'),
  petal:material('#eee6d9'),heart:material('#c3a35c'),sepal:material('#5d7143'),
};
let plantGroup=new THREE.Group(),plantData,activeView='garden',revision=0;
scene.add(plantGroup);
controls.addEventListener('start',()=>{
  activeView='custom';
  for(const button of document.querySelectorAll('[data-view]'))button.setAttribute('aria-pressed','false');
});
const vector=a=>new THREE.Vector3(...a);
const sphere=new THREE.IcosahedronGeometry(1,1);
const petalSphere=new THREE.IcosahedronGeometry(1,0);
let buckets;
function collect(key,geometry) {
  // The common non-indexed position/normal layout allows all primitives to merge.
  let g=geometry.index?geometry.toNonIndexed():geometry;
  if(g!==geometry) geometry.dispose();
  for(const name of Object.keys(g.attributes)) if(!['position','normal'].includes(name))g.deleteAttribute(name);
  (buckets[key]??=[]).push(g);
}
function blob(key,position,scale,rotation=0,coarse=false) {
  const g=(coarse?petalSphere:sphere).clone();
  g.scale(...scale);g.rotateY(rotation);g.translate(...position);collect(key,g);
}
function tube(key,curve,radius,segments=18) {
  const c=new THREE.CubicBezierCurve3(...curve.map(vector));
  const g=new THREE.TubeGeometry(c,segments,radius,5,false);
  const positions=g.getAttribute('position');
  for(let i=0;i<=segments;i++) {
    const center=c.getPointAt(i/segments),shrink=1-.75*(i/segments);
    for(let j=0;j<=5;j++) {
      const k=i*6+j,p=new THREE.Vector3().fromBufferAttribute(positions,k).sub(center).multiplyScalar(shrink).add(center);
      positions.setXYZ(k,p.x,p.y,p.z);
    }
  }
  g.computeVertexNormals();collect(key,g);
}
function stem(key,a,b,radius=.003) {tube(key,[a,add(mul(a,2/3),mul(b,1/3)),add(mul(a,1/3),mul(b,2/3)),b],radius,3);}
function leafBlade(base,angle,tilt,length,tone) {
  const outline=[[0,0],[.18,.30],[.26,.24],[.33,.43],[.41,.35],[.48,.49],[.57,.36],[.64,.4],[.73,.26],[.8,.28],[1,0]];
  const vertices=[],indices=[];
  for(const [u,w] of outline) vertices.push(u*length,Math.sin(u*Math.PI)*length*.075,-w*length*.72);
  for(let i=outline.length-2;i>0;i--) {const [u,w]=outline[i];vertices.push(u*length,Math.sin(u*Math.PI)*length*.075,w*length*.72);}
  const count=vertices.length/3,center=count;vertices.push(length*.48,length*.14,0);
  for(let i=0;i<count;i++)indices.push(center,i,(i+1)%count);
  const g=new THREE.BufferGeometry();g.setAttribute('position',new THREE.Float32BufferAttribute(vertices,3));g.setIndex(indices);g.computeVertexNormals();
  g.rotateZ(tilt);g.rotateY(-angle);g.translate(...base);collect(`leaf${tone}`,g);
}
function leafCluster(spec) {
  const {base,angle,size,tilt,tone}=spec;
  const hub=add(base,[Math.cos(angle)*.075,.035,Math.sin(angle)*.075]);
  stem('twig',base,hub,.0028);
  for(let k=0;k<3;k++) {
    const a=angle+(k-1)*1.05;
    const start=add(hub,[Math.cos(a)*.025,0,Math.sin(a)*.025]);
    stem('twig',hub,start,.0017);
    leafBlade(start,a,tilt+(k===1?.15:-.12),size*(k===1?1.1:.85),(tone+k%2)%5);
  }
}
function fruitCluster(site,index) {
  const {base,angle,size}=site;
  for(let k=0;k<3;k++) {
    const a=angle+(k-1)*1.55;
    const top=add(base,[Math.cos(a)*.065,(k===1?.035:-.012),Math.sin(a)*.065]);
    stem('twig',base,top,.002);
    const ripe=(index+k)%5, key=ripe<3?'ripe':ripe===3?'red':'green';
    for(let l=0;l<4;l++) {
      const count=l===3?4:6,r=l===3?.010:.016;
      for(let j=0;j<count;j++) {
        const theta=j/count*Math.PI*2+(l%2)*.5;
        const p=add(top,[Math.cos(theta)*r*size,-(.012+l*.011)*size,Math.sin(theta)*r*size]);
        blob(key==='ripe'&&j%3===0?'ripeLight':key,p,[.010*size,.010*size,.010*size]);
      }
    }
    for(let j=0;j<5;j++) {
      const a=j*Math.PI*2/5;
      blob('sepal',add(top,[Math.cos(a)*.012,0,Math.sin(a)*.012]),[.015,.003,.005],-a,true);
    }
  }
}
function flowerCluster(site) {
  for(let k=0;k<3;k++) {
    const a=site.angle+(k-1)*1.8,center=add(site.base,[Math.cos(a)*.07,.025+k*.013,Math.sin(a)*.07]);
    stem('twig',site.base,center,.002);
    for(let j=0;j<5;j++) {
      const angle=j/5*Math.PI*2;
      blob('petal',add(center,[Math.cos(angle)*.023,0,Math.sin(angle)*.023]),[.026,.007,.017],-angle,true);
    }
    blob('heart',add(center,[0,.009,0]),[.011,.008,.011],0,true);
  }
}
function clearPlant() {
  for(const child of plantGroup.children)child.geometry.dispose();
  plantGroup.clear();
}
function rebuild() {
  clearPlant();buckets={};plantData=makePlant(state);
  for(const cane of plantData.canes) {
    tube(cane.fresh?'fresh':'old',cane.curve,cane.radius);
    for(const lateral of cane.laterals)tube('twig',lateral.curve,.005,7);
    for(let j=1;j<18;j++) {
      const t=j/19,p=point(cane.curve,t),a=j*2.4+cane.id;
      const tip=add(p,[Math.cos(a)*.026,-.013,Math.sin(a)*.026]);
      stem('prickle',p,tip,.006*(1-t*.5));
    }
  }
  if(!state.skeleton) {
    for(const leaf of plantData.leaves)leafCluster(leaf);
    if(state.stage!=='leaf')plantData.fruitSites.forEach((site,i)=>state.stage==='fruit'?fruitCluster(site,i):flowerCluster(site));
  }
  let triangles=0;
  for(const [key,geometries] of Object.entries(buckets)) {
    const merged=mergeGeometries(geometries,false);
    geometries.forEach(g=>g.dispose());
    const mesh=new THREE.Mesh(merged,materials[key]);mesh.name=key;plantGroup.add(mesh);
    triangles+=merged.getAttribute('position').count/3;
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
