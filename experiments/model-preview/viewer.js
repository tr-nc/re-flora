import * as THREE from 'three';
import {OrbitControls} from 'three/addons/controls/OrbitControls.js';
import {PreviewPipeline} from './pipeline.js';
import {modelDefinitions,definitionFor} from './models/index.js';
import {sampleTime,advanceTime} from './timeline.mjs';
import {projectGroups} from './geometry.js';
import './color-picker.js';

const $=id=>document.getElementById(id);
const sourceCanvas=$('source'),pixelCanvas=$('pixel'),target=new THREE.Vector3();
let asset,definition,modelSettings,pipeline,camera,pixelCamera,controls,request=0,last=0;
const state={ready:false,loading:false,failed:false,model:'leaf',dirty:true,playing:false,time:0,clip:0,fps:60,speed:1,
  resolution:32,levels:8,projection:'orthographic',variant:'compare',wireframe:false,rotate:false,background:'#253426',checker:false,conservativeCoverage:true};

function message(text,error=false){$('status').textContent=text;$('status').classList.toggle('error',error);}
function dirty(){state.dirty=true;}
function setPlaying(value){state.playing=value&&Boolean(asset?.clips.length);$('play').textContent=state.playing?'暂停':'播放';$('play').setAttribute('aria-pressed',String(state.playing));last=performance.now();}
function cameraFor(kind){
  const near=asset?.view.near??.05,far=asset?.view.far??100;
  return kind==='perspective'?new THREE.PerspectiveCamera(35,1,near,far):new THREE.OrthographicCamera(-1.7,1.7,1.7,-1.7,near,far);
}
function setProjection(kind){
  const old=camera;camera=cameraFor(kind);camera.position.copy(old.position);camera.quaternion.copy(old.quaternion);camera.zoom=old.zoom;
  if(asset&&camera.isOrthographicCamera){const half=asset.view.span/2;camera.left=-half;camera.right=half;camera.top=half;camera.bottom=-half;}
  pixelCamera=camera.clone();
  state.projection=kind;camera.updateProjectionMatrix();controls.forEach(control=>{control.object=camera;control.update();});$('projection').value=kind;dirty();
}
function setView(kind='reset'){
  if(!asset)return;
  target.fromArray(asset.view.target);
  const distance=asset.view.axisDistance??new THREE.Vector3(...asset.view.offset).length();
  const offset={front:[0,0,distance],back:[0,0,-distance],edge:[distance,0,.1],reset:asset.view.offset}[kind];
  camera.position.copy(target).add(new THREE.Vector3(...offset));camera.up.set(0,1,0);camera.zoom=1;
  camera.lookAt(target);camera.updateProjectionMatrix();controls.forEach(control=>control.update());dirty();
}
function syncPixelCamera(){
  // Game tiles use a fixed 3.4-unit model framing (butterfly_mesh.rs).
  // Keep the viewing direction, never the author's inspection zoom/distance.
  const direction=camera.position.clone().sub(target).normalize();
  const distance=asset.view.axisDistance??new THREE.Vector3(...asset.view.offset).length();
  pixelCamera.position.copy(target).addScaledVector(direction,distance);
  pixelCamera.quaternion.copy(camera.quaternion);
  pixelCamera.zoom=1;pixelCamera.updateProjectionMatrix();
}
function resize(){
  if(!pipeline)return;
  const stages=[sourceCanvas.parentElement,pixelCanvas.parentElement];
  const sourceSize=Math.max(1,Math.floor(Math.min(stages[0].clientWidth,stages[0].clientHeight)));
  const available=Math.max(1,Math.floor(Math.min(stages[1].clientWidth,stages[1].clientHeight)));
  const scale=Math.floor(available/state.resolution),pixelSize=scale?scale*state.resolution:available;
  pipeline.resize(sourceSize,state.resolution);
  sourceCanvas.style.width=sourceCanvas.style.height=`${sourceSize}px`;
  pixelCanvas.style.width=pixelCanvas.style.height=`${pixelSize}px`;
  $('tile-label').textContent=`${state.resolution} × ${state.resolution}`;
  $('pixel-info').textContent=scale?`${state.resolution**2} 像素画布 · ${scale}× 最近邻`:'窗口不足以整数放大';dirty();
}
function setVariant(value){
  const names={compare:'布局 1 · 双视图对照',model:'布局 2 · 模型优先',pixel:'布局 3 · 像素画放大'};
  state.variant=Object.hasOwn(names,value)?value:'compare';document.body.dataset.variant=state.variant;
  $('variant-label').textContent=names[state.variant];updateURL();resize();
}
function updateURL(){const url=new URL(location.href);url.searchParams.set('model',state.model);url.searchParams.set('variant',state.variant);history.replaceState(null,'',url);}
function cycleVariant(direction){const options=['compare','model','pixel'];setVariant(options[(options.indexOf(state.variant)+direction+3)%3]);}
function updateBackground(){document.documentElement.style.setProperty('--stage',state.background);document.body.classList.toggle('checker',state.checker);}
function buildModelControls(){
  $('model-controls').replaceChildren();
  for(const schema of definition.controls){
    const label=document.createElement('label');label.className=schema.type==='checkbox'?'check-row':'slider-label';
    const input=document.createElement(schema.type==='color'?'debug-color-picker':'input');input.id=`model-${schema.key}`;
    if(schema.type==='color'){
      label.textContent=schema.label;input.setAttribute('label',schema.label);input.setAttribute('value',modelSettings[schema.key]);
    }else if(schema.type==='checkbox'){
      input.type='checkbox';input.checked=modelSettings[schema.key];label.append(input,document.createTextNode(` ${schema.label}`));
    }else{
      label.htmlFor=input.id;label.textContent=schema.label;
      const output=document.createElement('output');output.id=`model-${schema.key}-value`;output.textContent=modelSettings[schema.key];label.append(output);
      input.type='range';input.min=schema.min;input.max=schema.max;input.step=schema.step;input.value=modelSettings[schema.key];
    }
    $('model-controls').append(label);if(schema.type!=='checkbox')$('model-controls').append(input);
    input.addEventListener('input',()=>{
      modelSettings[schema.key]=schema.type==='color'?input.value:schema.type==='checkbox'?input.checked:Number(input.value);
      const output=$(`model-${schema.key}-value`);if(output)output.textContent=modelSettings[schema.key];asset.apply(modelSettings);dirty();
    });
  }
  if(definition.colorPresets){
    const section=document.createElement('div');section.className='color-presets';
    const title=document.createElement('strong');title.textContent='叶片配色 · 仅覆盖颜色';section.append(title);
    const buttons=document.createElement('div');buttons.className='preset-buttons';section.append(buttons);
    for(const preset of definition.colorPresets){
      const button=document.createElement('button');button.type='button';button.textContent=preset.name;
      button.dataset.preset=preset.name;
      button.addEventListener('click',()=>{
        Object.assign(modelSettings,preset.colors);
        for(const [key,color] of Object.entries(preset.colors))$(`model-${key}`).value=color;
        asset.apply(modelSettings);dirty();
      });
      buttons.append(button);
    }
    $('model-controls').append(section);
  }
}
function syncControls(){
  for(const key of ['resolution','fps','speed','levels','projection'])$(key).value=state[key];
  for(const key of ['wireframe','rotate','checker'])$(key).checked=state[key];
  $('conservative-coverage').checked=state.conservativeCoverage;
  $('background').value=state.background;updateBackground();
  $('resolution-value').textContent=`${state.resolution} × ${state.resolution}`;
  $('levels-value').textContent=state.levels?`${state.levels} 级 / 材质`:'关闭';
  $('fps-value').textContent=`${state.fps} FPS`;$('speed-value').textContent=`${state.speed}×`;
  $('coverage-mode').textContent=state.conservativeCoverage?'通用保守覆盖 + 八邻接补点':'旧八邻接补点';
  $('clip').replaceChildren();
  for(const [index,clip]of (asset?.clips??[]).entries())$('clip').add(new Option(clip.name,String(index)));
  if(!asset?.clips.length)$('clip').add(new Option('无动画','0'));
  $('clip').value=state.clip;
  $('phase').max=Math.max(0,Math.ceil((asset?.clips[state.clip]?.duration??0)*1000)-1);
  const disabled=!state.ready||state.loading||state.failed;
  for(const element of document.querySelectorAll('button,input,select'))if(element.id!=='model')element.disabled=disabled;
  for(const id of ['play','previous-frame','next-frame','clip','phase','fps','speed'])$(id).disabled=disabled||!asset?.clips.length;
  $('play').textContent=state.playing?'暂停':'播放';$('play').setAttribute('aria-pressed',String(state.playing));
}
async function loadModel(id){
  const nextDefinition=definitionFor(id);if(!nextDefinition)throw new Error('未知模型');
  const token=++request;state.loading=true;setPlaying(false);syncControls();message(`正在加载${nextDefinition.label}…`);
  let next;
  try{
    next=await nextDefinition.create();
    if(token!==request){next.dispose();return;}
    const values={...nextDefinition.defaults};next.apply(values);
    asset?.dispose();pipeline.releaseAsset();asset=next;next=null;definition=nextDefinition;modelSettings=values;
    Object.assign(state,{model:id,ready:true,loading:false,failed:false,playing:false,time:0,clip:0,fps:60,speed:1,levels:8,wireframe:false,rotate:false,checker:false,conservativeCoverage:true,...definition.preview});
    $('model').value=id;setProjection('orthographic');setView();
    buildModelControls();syncControls();setVariant(state.variant);
    $('model-info').textContent=definition.label;
    const triangles=asset.meshes.reduce((sum,mesh)=>sum+(mesh.geometry.index?.count??mesh.geometry.attributes.position.count)/3,0);
    $('geometry-info').textContent=`${triangles} 三角形 · ${asset.repairGroups.length} 个补点组`;
    message(asset.description);updateURL();dirty();
  }catch(error){
    next?.dispose();if(token!==request)return;
    state.loading=false;state.ready=Boolean(asset);$('model').value=state.model;syncControls();message(`加载失败：${error.message}`,true);
  }
}
function render(){
  const duration=asset.clips[state.clip]?.duration??0,time=sampleTime(state.time,duration,state.fps);
  syncPixelCamera();
  const frame=pipeline.render(asset,camera,pixelCamera,{time,clip:state.clip,wireframe:state.wireframe,levels:state.levels,conservativeCoverage:state.conservativeCoverage});
  $('phase').value=Math.floor(state.time*1000);$('phase-value').textContent=duration?`${time.toFixed(3)} / ${duration.toFixed(3)} s`:'静态模型 · t = 0';
  let repairMessage=state.conservativeCoverage?'通用保守覆盖 + 八邻接补点':'旧八邻接补点';
  if(frame.repair){
    const {added,groups}=frame.repair;
    const separated=groups.some(group=>group.after>1);
    const outcome=added?`已补 ${added} 个像素${separated?'；仍有分离区域无允许的连接路径':''}`
      :separated?'未补点：仍有分离区域，但没有允许的几何连接路径'
      :groups.some(group=>group.before>0)?'无需补点：当前各组已八邻接连通（斜向接触也算）'
      :'未补点：当前没有可见采样点';
    const preserved=groups.reduce((sum,group)=>sum+group.preserved,0);
    repairMessage=`${outcome}${preserved?`（其中 ${preserved} 个几何覆盖像素）`:''} · ${groups.map(group=>`${asset.repairGroups.find(g=>g.id===group.id).label} ${group.before}→${group.after}`).join(' · ')}`;
  }
  if($('repair-info').textContent!==repairMessage)$('repair-info').textContent=repairMessage;
  $('camera-info').textContent=`方向同步 · 左侧 ${camera.zoom.toFixed(2)}× · 右侧固定游戏取景`;
  state.dirty=false;
}
function tick(now){
  const delta=(now-last)/1000;last=now;
  if(state.ready&&!state.loading&&!state.failed){
    try{
      if(state.playing){
        const duration=asset.clips[state.clip].duration;
        state.time=advanceTime(state.time,delta,duration,state.speed);
        if(sampleTime(state.time,duration,state.fps)!==pipeline.last?.pixelTime)dirty();
      }
      if(state.rotate){camera.position.sub(target).applyAxisAngle(new THREE.Vector3(0,1,0),Math.min(delta,.05)*.4).add(target);camera.lookAt(target);controls.forEach(control=>control.update());dirty();}
      if(state.dirty)render();
    }catch(error){state.failed=true;setPlaying(false);syncControls();message(`预览失败：${error.message}，请刷新恢复。`,true);console.error(error);}
  }
  requestAnimationFrame(tick);
}
function downloadBlob(blob,name){const url=URL.createObjectURL(blob),link=document.createElement('a');link.href=url;link.download=name;link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
function stepFrame(direction){
  if(!asset?.clips.length)return;setPlaying(false);
  const duration=asset.clips[state.clip].duration,count=Math.ceil(duration*state.fps);
  const index=Math.floor((sampleTime(state.time,duration,state.fps)+1e-10)*state.fps);
  state.time=((index+direction+count)%count)/state.fps;dirty();
}
function init(){
  pipeline=new PreviewPipeline(sourceCanvas,pixelCanvas);camera=cameraFor('orthographic');
  controls=[sourceCanvas,pixelCanvas].map(canvas=>{
    const control=new OrbitControls(camera,canvas);control.target=target;control.enablePan=false;control.enableDamping=false;
    control.minZoom=.55;control.maxZoom=2.5;control.minDistance=3.5;control.maxDistance=12;
    if(canvas===pixelCanvas)control.enableZoom=false;
    control.minPolarAngle=.01;control.maxPolarAngle=Math.PI-.01;control.addEventListener('change',dirty);
    canvas.addEventListener('keydown',event=>{
      const delta={ArrowLeft:[-.12,0],ArrowRight:[.12,0],ArrowUp:[0,-.12],ArrowDown:[0,.12]}[event.key];if(!delta)return;
      event.preventDefault();event.stopPropagation();
      const spherical=new THREE.Spherical().setFromVector3(camera.position.clone().sub(target));spherical.theta+=delta[0];spherical.phi=THREE.MathUtils.clamp(spherical.phi+delta[1],.01,Math.PI-.01);
      camera.position.setFromSpherical(spherical).add(target);camera.lookAt(target);control.update();dirty();
    });
    canvas.addEventListener('webglcontextlost',event=>{event.preventDefault();state.failed=true;setPlaying(false);syncControls();message('WebGL 上下文丢失，请刷新恢复。',true);});
    return control;
  });
  for(const definition of modelDefinitions)$('model').add(new Option(definition.label,definition.id));
  $('model').addEventListener('change',()=>loadModel($('model').value));
  for(const id of ['front','back','edge'])$(id).addEventListener('click',()=>setView(id));
  $('reset-view').addEventListener('click',()=>setView());$('projection').addEventListener('change',()=>setProjection($('projection').value));
  for(const key of ['resolution','levels','fps','speed'])$(key).addEventListener('input',()=>{state[key]=Number($(key).value);syncControls();if(key==='resolution')resize();dirty();});
  for(const key of ['wireframe','rotate','checker'])$(key).addEventListener('change',()=>{state[key]=$(key).checked;syncControls();dirty();});
  $('conservative-coverage').addEventListener('change',()=>{state.conservativeCoverage=$('conservative-coverage').checked;syncControls();dirty();});
  $('background').addEventListener('input',()=>{state.background=$('background').value;updateBackground();});
  $('play').addEventListener('click',()=>setPlaying(!state.playing));
  $('previous-frame').addEventListener('click',()=>stepFrame(-1));$('next-frame').addEventListener('click',()=>stepFrame(1));
  $('phase').addEventListener('input',()=>{setPlaying(false);state.time=Number($('phase').value)/1000;dirty();});
  $('clip').addEventListener('change',()=>{setPlaying(false);state.clip=Number($('clip').value);state.time=0;syncControls();dirty();});
  $('reset-all').addEventListener('click',()=>loadModel(state.model));
  $('download').addEventListener('click',()=>{
    render();const time=pipeline.last.pixelTime.toFixed(3);
    const mode=state.conservativeCoverage?'coverage':'bridge-only';
    pixelCanvas.toBlob(blob=>{if(blob)downloadBlob(blob,`${state.model}-${mode}-${state.resolution}px-${time}s.png`);},'image/png');
  });
  $('previous').addEventListener('click',()=>cycleVariant(-1));$('next').addEventListener('click',()=>cycleVariant(1));
  document.addEventListener('keydown',event=>{if(event.target.closest('input,select,textarea,canvas,debug-color-picker,[contenteditable]'))return;if(['ArrowLeft','ArrowRight'].includes(event.key)){event.preventDefault();cycleVariant(event.key==='ArrowLeft'?-1:1);}});
  document.addEventListener('visibilitychange',()=>{last=performance.now();});
  const observer=new ResizeObserver(resize);document.querySelectorAll('.stage').forEach(stage=>observer.observe(stage));window.addEventListener('resize',resize);
  const params=new URLSearchParams(location.search);setVariant(params.get('variant'));
  const id=definitionFor(params.get('model'))?params.get('model'):'leaf';loadModel(id);requestAnimationFrame(tick);
}

window.readModelPreview=(includeGeometry=false)=>({
  ...state,modelSettings:{...modelSettings},sourceTime:pipeline?.last?.sourceTime,pixelTime:pipeline?.last?.pixelTime,
  pixelBuffer:[pixelCanvas.width,pixelCanvas.height],sourceBuffer:[sourceCanvas.width,sourceCanvas.height],
  camera:camera?.position.toArray(),target:target.toArray(),zoom:camera?.zoom,
  pixelCamera:pixelCamera?.position.toArray(),pixelZoom:pixelCamera?.zoom,
  linkedRotation:pixelCamera&&camera?pixelCamera.quaternion.angleTo(camera.quaternion)<1e-6:false,
  repair:pipeline?.last?.repair,repairEnabled:true,
  clips:asset?.clips,meshes:asset?.meshes.map(mesh=>mesh.name),
  triangles:asset?.meshes.reduce((sum,mesh)=>sum+(mesh.geometry.index?.count??mesh.geometry.attributes.position.count)/3,0),
  ...(includeGeometry&&asset?{projectedGroups:projectGroups(asset,pixelCamera,state.resolution),originalRgba:Array.from(pipeline?.last?.original??[]),owners:Array.from(pipeline?.last?.owners??[])}:{}),
});
try{init();}catch(error){state.failed=true;message(`初始化失败：${error.message}`,true);console.error(error);}
