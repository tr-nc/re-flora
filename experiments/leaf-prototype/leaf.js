// THROWAWAY: evaluate a 32-triangle leaf, not a production renderer.
// Three observation layouts (?variant=compare|model|pixel), one shared model/camera.
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

const $ = id => document.getElementById(id);
const fields = ['resolution', 'steps', 'width', 'fold', 'curl', 'season', 'veins', 'light', 'transmission'];
const defaults = Object.fromEntries(fields.map(id => [id, Number($(id).value)]));
const state = { ...defaults, variant: 'compare', dirty: true, ready: false };
const sourceCanvas = $('source'), pixelCanvas = $('pixel');
const scene = new THREE.Scene();
const camera = new THREE.OrthographicCamera(-1.7, 1.7, 1.7, -1.7, .1, 40);
const target = new THREE.Vector3(0, -.06, .05);
let sourceRenderer, pixelRenderer, controls, leaf, wire, last = 0;
const uniforms = {
  season: { value: state.season }, veins: { value: state.veins },
  transmission: { value: state.transmission }, steps: { value: 0 },
  lightDirection: { value: new THREE.Vector3() },
};
const material = new THREE.ShaderMaterial({
  side: THREE.DoubleSide,
  uniforms,
  vertexShader: `
    varying vec2 leafUV;
    varying vec3 worldNormal;
    varying vec3 worldPosition;
    void main() {
      leafUV = uv;
      worldNormal = normalize(mat3(modelMatrix) * normal);
      worldPosition = (modelMatrix * vec4(position, 1.)).xyz;
      gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.);
    }`,
  fragmentShader: `
    uniform float season, veins, transmission, steps;
    uniform vec3 lightDirection;
    varying vec2 leafUV;
    varying vec3 worldNormal;
    varying vec3 worldPosition;
    float hash(vec2 p) { return fract(sin(dot(p,vec2(127.1,311.7))) * 43758.5453); }
    float noise(vec2 p) {
      vec2 i=floor(p), f=fract(p); f=f*f*(3.-2.*f);
      return mix(mix(hash(i),hash(i+vec2(1,0)),f.x),mix(hash(i+vec2(0,1)),hash(i+vec2(1,1)),f.x),f.y);
    }
    void main() {
      bool stem = leafUV.x > 1.5;
      vec2 uv = leafUV;
      float lateral = abs(uv.x-.5)*2.;
      float mottling = noise(uv*vec2(13.,19.));
      vec3 green = vec3(.105,.24,.026);
      vec3 ochre = vec3(.57,.225,.023);
      vec3 base = mix(green,ochre,clamp(season + (mottling-.5)*.15,0.,1.));
      base *= .87 + .19*mottling;
      // Broad, quiet variation survives downsampling; veins don't require geometry.
      float branchPhase = uv.y*7. - lateral*1.45;
      float branchDistance = abs(fract(branchPhase+.5)-.5);
      float branchAA = max(fwidth(branchPhase)*.65,.012);
      float branches = (1.-smoothstep(.015,.015+branchAA,branchDistance))
        * smoothstep(.03,.12,lateral) * (1.-smoothstep(.73,1.,lateral));
      float midrib = 1.-smoothstep(.012,.012+max(fwidth(uv.x),.003),abs(uv.x-.5));
      base = mix(base,base*1.45+vec3(.025,.025,.003),veins*max(midrib,branches*.6));
      base *= 1.-.16*smoothstep(.83,1.,lateral);
      if (!gl_FrontFacing) base = mix(base,vec3(.32,.36,.13),.30);
      if (stem) base = mix(vec3(.19,.23,.055),vec3(.29,.13,.033),season);
      vec3 n = normalize(worldNormal) * (gl_FrontFacing ? 1. : -1.);
      vec3 l = normalize(lightDirection);
      float diffuse = max(dot(n,l),0.);
      float through = max(dot(-n,l),0.) * transmission * (stem ? .15 : .75);
      float illumination = .38 + .76*diffuse + through;
      if (steps > .5) illumination = floor(illumination*steps+.5)/steps;
      vec3 color = base * illumination;
      // Wide, subtle waxy highlight, not a plastic specular spot.
      vec3 viewDirection = normalize(cameraPosition-worldPosition);
      float sheen = pow(max(dot(n,normalize(l+viewDirection)),0.),24.)*.035*diffuse;
      color += vec3(sheen);
      color += base*vec3(.14,.07,0.)*through;
      gl_FragColor = vec4(color,1.);
      #include <colorspace_fragment>
    }`,
});

function makeGeometry() {
  const positions = [], uvs = [], indices = [];
  function add(x, t, u, stem = false) {
    const z = state.fold*Math.abs(x) + state.curl*(t*t*t-.15)*.7 + .055*Math.sin(t*3.6)*x;
    positions.push(x, (t-.5)*2.25, z);
    uvs.push(stem ? 2 : u, t);
    return positions.length/3-1;
  }
  const root = add(0,0,.5);
  const rows = [];
  for (let i=1;i<8;i++) {
    const t=i/8;
    const halfWidth=.58*state.width*Math.pow(Math.sin(Math.PI*t),.82)*(1.08-.24*t);
    const offset=.035*Math.sin(t*Math.PI*1.5);
    rows.push([
      add(offset-halfWidth*(1.+.05*Math.sin(t*12.)),t,0),
      add(offset,t,.5),
      add(offset+halfWidth*(.95+.035*Math.cos(t*15.)),t,1),
    ]);
  }
  indices.push(root,rows[0][1],rows[0][0], root,rows[0][2],rows[0][1]);
  for(let i=0;i<rows.length-1;i++) {
    const a=rows[i], b=rows[i+1];
    for(let j=0;j<2;j++) indices.push(a[j],a[j+1],b[j+1], a[j],b[j+1],b[j]);
  }
  const tip=add(-.035,1,.5), end=rows.at(-1);
  indices.push(end[0],end[1],tip, end[1],end[2],tip);
  // A tapered, slightly bent petiole: two double-sided quads, four triangles.
  const stemRows = [-.15,-.075,.008].map(t=>{
    const x=.09*Math.pow(t/.15,2), w=.014*(1+t*2);
    return [add(x-w,t,2,true),add(x+w,t,2,true)];
  });
  for(let i=0;i<2;i++) {
    const a=stemRows[i],b=stemRows[i+1];
    indices.push(a[0],a[1],b[1],a[0],b[1],b[0]);
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position',new THREE.Float32BufferAttribute(positions,3));
  geometry.setAttribute('uv',new THREE.Float32BufferAttribute(uvs,2));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}

function rebuild() {
  const geometry = makeGeometry();
  if (!leaf) {
    leaf = new THREE.Mesh(geometry,material);
    scene.add(leaf);
    wire = new THREE.LineSegments(new THREE.WireframeGeometry(geometry),new THREE.LineBasicMaterial({ color: 0xf4e8bc, transparent:true, opacity:.65 }));
    scene.add(wire);
  } else {
    leaf.geometry.dispose();
    leaf.geometry = geometry;
    wire.geometry.dispose();
    wire.geometry = new THREE.WireframeGeometry(geometry);
  }
  $('mesh-info').textContent = `${geometry.index.count/3} 三角形 · ${geometry.attributes.position.count} 顶点 · 双面薄片`;
  state.dirty = true;
}

function resize() {
  if(!sourceRenderer) return;
  const sourceStage=sourceCanvas.parentElement, pixelStage=pixelCanvas.parentElement;
  const size=Math.max(1,Math.floor(Math.min(sourceStage.clientWidth,sourceStage.clientHeight)));
  sourceRenderer.setSize(size,size,false);
  sourceCanvas.style.width=sourceCanvas.style.height=`${size}px`;
  pixelRenderer.setSize(state.resolution,state.resolution,false);
  // Integer CSS magnification: no unevenly sized logical pixels at 1× browser scale.
  const scale=Math.max(1,Math.floor(Math.min(pixelStage.clientWidth,pixelStage.clientHeight)/state.resolution));
  pixelCanvas.style.width=pixelCanvas.style.height=`${state.resolution*scale}px`;
  $('tile-label').textContent=`${state.resolution} × ${state.resolution}`;
  $('pixel-info').textContent=`${state.resolution**2} 像素画布 · ${scale}× 最近邻`;
  state.dirty=true;
}

function setView(kind='reset') {
  const offsets={front:[0,0,6],back:[0,0,-6],edge:[6,0,.10],reset:[2.3,1.1,6]};
  camera.position.copy(target).add(new THREE.Vector3(...offsets[kind]));
  camera.zoom=1;
  camera.up.set(0,1,0);
  camera.lookAt(target);
  camera.updateProjectionMatrix();
  controls.forEach(control=>control.update());
  state.dirty=true;
}

function updateFields() {
  for (const id of fields) {
    state[id]=Number($(id).value);
    $(`${id}-value`).textContent=id==='resolution' ? `${state[id]} × ${state[id]}`
      : id==='steps' ? (state[id]===0?'连续':`${state[id]} 级 / 单位明度`)
      : id==='light' ? `${state[id]}°` : state[id].toFixed(2);
  }
  for (const key of ['season','veins','transmission']) uniforms[key].value=state[key];
  const angle=THREE.MathUtils.degToRad(state.light);
  uniforms.lightDirection.value.set(Math.sin(angle),.65,Math.cos(angle)).normalize();
  state.dirty=true;
}

function setVariant(value) {
  const names={compare:'A · 双视图对照',model:'B · 模型优先',pixel:'C · 像素画放大'};
  state.variant=Object.hasOwn(names,value)?value:'compare';
  document.body.dataset.variant=state.variant;
  $('variant-label').textContent=names[state.variant];
  const url=new URL(location.href);
  url.searchParams.set('variant',state.variant);
  history.replaceState(null,'',url);
  resize();
}
function cycleVariant(direction) {
  const variants=['compare','model','pixel'];
  setVariant(variants[(variants.indexOf(state.variant)+direction+3)%3]);
}

function render() {
  wire.visible=$('wireframe').checked;
  uniforms.steps.value=0;
  sourceRenderer.render(scene,camera);
  wire.visible=false; // Topology is an inspection overlay, not part of the artwork.
  uniforms.steps.value=state.steps;
  pixelRenderer.render(scene,camera);
  $('camera-info').textContent=`共享视角 (${camera.position.toArray().map(n=>n.toFixed(2)).join(', ')}) · 缩放 ${camera.zoom.toFixed(2)}`;
  state.dirty=false;
}
function tick(now) {
  if(!state.ready) return;
  if($('rotate').checked) {
    const delta=Math.min((now-last)/1000,.05);
    camera.position.sub(target).applyAxisAngle(new THREE.Vector3(0,1,0),delta*.4).add(target);
    camera.lookAt(target);
    controls.forEach(control=>control.update());
    state.dirty=true;
  }
  if(state.dirty) render();
  last=now;
  requestAnimationFrame(tick);
}

function init() {
  sourceRenderer=new THREE.WebGLRenderer({canvas:sourceCanvas,alpha:true,antialias:true});
  sourceRenderer.setPixelRatio(Math.min(devicePixelRatio,2));
  pixelRenderer=new THREE.WebGLRenderer({canvas:pixelCanvas,alpha:true,antialias:false,preserveDrawingBuffer:true});
  pixelRenderer.setPixelRatio(1);
  for(const renderer of [sourceRenderer,pixelRenderer]) {
    renderer.setClearColor(0,0);
    renderer.outputColorSpace=THREE.SRGBColorSpace;
  }
  controls=[sourceCanvas,pixelCanvas].map(canvas=>{
    const control=new OrbitControls(camera,canvas);
    control.target=target;
    control.enablePan=false;
    control.enableDamping=false;
    control.minZoom=.55;
    control.maxZoom=2.5;
    control.addEventListener('change',()=>{state.dirty=true;});
    canvas.addEventListener('webglcontextlost',event=>{
      event.preventDefault(); state.ready=false;
      $('status').textContent='WebGL 上下文丢失，请刷新页面恢复预览。';
    });
    canvas.addEventListener('keydown',event=>{
      const offsets={ArrowLeft:[-.12,0],ArrowRight:[.12,0],ArrowUp:[0,-.12],ArrowDown:[0,.12]};
      if(!offsets[event.key]) return;
      event.preventDefault();event.stopPropagation();
      const [theta,phi]=offsets[event.key];
      const spherical=new THREE.Spherical().setFromVector3(camera.position.clone().sub(target));
      spherical.theta+=theta;
      spherical.phi=THREE.MathUtils.clamp(spherical.phi+phi,.01,Math.PI-.01);
      camera.position.setFromSpherical(spherical).add(target);
      camera.lookAt(target);control.update();state.dirty=true;
    });
    return control;
  });
  updateFields();rebuild();setView();
  for(const id of fields) $(id).addEventListener('input',()=>{
    updateFields();
    if(['width','fold','curl'].includes(id)) rebuild();
    if(id==='resolution') resize();
  });
  document.querySelectorAll('[data-resolution]').forEach(button=>button.addEventListener('click',()=>{
    $('resolution').value=button.dataset.resolution;updateFields();resize();
  }));
  for(const kind of ['front','back','edge']) $(kind).addEventListener('click',()=>setView(kind));
  $('reset-view').addEventListener('click',()=>setView());
  $('wireframe').addEventListener('change',()=>{state.dirty=true;});
  $('background').addEventListener('change',()=>{
    document.body.classList.remove('paper','checker');
    if($('background').value!=='forest') document.body.classList.add($('background').value);
  });
  $('reset-all').addEventListener('click',()=>{
    for(const id of fields) $(id).value=defaults[id];
    $('rotate').checked=$('wireframe').checked=false;
    $('background').value='forest';document.body.classList.remove('paper','checker');
    updateFields();rebuild();setView();resize();
  });
  $('download').addEventListener('click',()=>{
    render();
    const a=document.createElement('a');a.download=`leaf-${state.resolution}x${state.resolution}.png`;
    a.href=pixelCanvas.toDataURL('image/png');a.click();
  });
  $('previous').addEventListener('click',()=>cycleVariant(-1));
  $('next').addEventListener('click',()=>cycleVariant(1));
  document.addEventListener('keydown',event=>{
    if(event.target.closest('input,select,textarea,canvas,[contenteditable]')) return;
    if(['ArrowLeft','ArrowRight'].includes(event.key)) {
      event.preventDefault();cycleVariant(event.key==='ArrowLeft'?-1:1);
    }
  });
  const observer=new ResizeObserver(resize);
  document.querySelectorAll('.stage').forEach(stage=>observer.observe(stage));
  window.addEventListener('resize',resize);
  setVariant(new URLSearchParams(location.search).get('variant'));
  state.ready=true;
  $('status').textContent='32 面候选：尖椭圆轮廓、弯曲主脉、短叶柄、浅色背面。程序叶脉 / 近似透光；尚未模拟飘落，也不等同于游戏最终渲染。所有调整仅保留在本页内存。';
  requestAnimationFrame(tick);
}

// Read-only inspection for prototype validation; no persistent state or game hooks.
window.readLeafPrototype=()=>({
  ...state, triangles:leaf?.geometry.index.count/3,
  pixelBuffer:[pixelCanvas.width,pixelCanvas.height],
  sharedCamera:controls?.every(c=>c.object===camera&&c.target===target),
  camera:camera.position.toArray(),zoom:camera.zoom,
});
try { init(); } catch(error) {
  state.ready=false;
  $('status').textContent=`预览初始化失败：${error.message}。请通过本地 HTTP 服务打开并确认浏览器支持 WebGL。`;
  console.error(error);
}
