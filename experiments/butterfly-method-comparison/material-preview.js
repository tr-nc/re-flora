import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { createPixelOutput } from './pixel-outline.js';

const $ = id => document.getElementById(id);
const status = $('status');
const sourceCanvas = $('source-canvas');
const pixelCanvas = $('pixel-canvas');
const state = {
  ready: false, failed: false, playing: !matchMedia('(prefers-reduced-motion: reduce)').matches,
  phase: 0, sampledTime: 0, resolution: 12, dirty: true, last: 0,
};
const target = new THREE.Vector3();
const span = 3.4;
let scene, camera, sourceRenderer, pixelRenderer, pixelOutput, mixer, controls, keyLight, fillLight;
const surfaceMaterials = new Set();
let lastSourceTime = null, lastPixelTime = null;
let resizeObserver;

function fail(error) {
  state.failed = true;
  state.playing = false;
  status.textContent = `预览无法继续：${error.message}。请检查 WebGL 支持后刷新。`;
  status.classList.add('error');
  for (const id of ['play', 'download']) $(id).disabled = true;
}

function setPlaying(value) {
  state.playing = value;
  state.last = performance.now();
  $('play').textContent = value ? '暂停' : '播放';
  $('play').setAttribute('aria-pressed', String(value));
}

function createCamera(kind) {
  const result = kind === 'perspective'
    ? new THREE.PerspectiveCamera(35, 1, .05, 100)
    : new THREE.OrthographicCamera(-span/2, span/2, span/2, -span/2, .05, 100);
  return result;
}

function initializeView() {
  const distance = span / (2 * Math.tan(THREE.MathUtils.degToRad(35/2)));
  camera.position.set(-.3535533906, .8660254038, -.3535533906).multiplyScalar(distance);
  camera.zoom = 1;
  target.set(0, 0, 0);
  camera.lookAt(target);
  camera.updateProjectionMatrix();
  // Both controllers point at this same camera/target. No copy-and-feedback loop.
  controls[0].update();
  controls[1].update();
  state.dirty = true;
}

function resize() {
  if (!sourceRenderer) return;
  const panel = document.querySelector('.controls-panel');
  const inset = getComputedStyle(panel).position === 'sticky' ? panel.getBoundingClientRect().height : 0;
  document.documentElement.style.setProperty('--controls-height', `${inset}px`);
  const available = Math.min(384, sourceCanvas.parentElement.clientWidth, pixelCanvas.parentElement.clientWidth);
  const displaySize = Math.max(state.resolution, Math.floor(available / state.resolution) * state.resolution);
  sourceRenderer.setPixelRatio(Math.min(devicePixelRatio || 1, 2));
  sourceRenderer.setSize(displaySize, displaySize, false);
  pixelRenderer.setPixelRatio(1);
  pixelRenderer.setSize(state.resolution, state.resolution, false);
  pixelOutput.resize(state.resolution);
  for (const canvas of [sourceCanvas, pixelCanvas]) {
    canvas.style.width = canvas.style.height = `${displaySize}px`;
  }
  $('source-size').textContent = `原始 3D · ${sourceCanvas.width} × ${sourceCanvas.height} 渲染缓冲`;
  $('pixel-size').textContent = `${pixelCanvas.width} × ${pixelCanvas.height} 原生缓冲 → ${displaySize}px 显示 · ${displaySize / state.resolution}× 最近邻`;
  $('resolution-label').textContent = `${state.resolution} × ${state.resolution}`;
  $('resolution-value').textContent = `${state.resolution} × ${state.resolution}`;
  state.dirty = true;
}

function render() {
  const time = $('sampling').value === 'stepped' ? Math.floor(state.phase * 5) / 5 : state.phase;
  mixer.setTime(time);
  scene.updateMatrixWorld(true);
  camera.updateMatrixWorld(true);
  state.sampledTime = time;
  // The same 3D light/shadow result is rasterized at each resolution. Only
  // the low-resolution output receives the optional silhouette post-pass.
  sourceRenderer.render(scene, camera);
  lastSourceTime = time;
  pixelOutput.render(scene, camera);
  lastPixelTime = time;
  $('phase').value = Math.floor(state.phase * 1000);
  $('phase-label').textContent = `${time.toFixed(3)} s`;
  const position = camera.position.toArray().map(v => v.toFixed(3)).join(', ');
  $('camera-state').textContent = `${camera.isOrthographicCamera ? '正交' : '透视'} · 相机 (${position}) · zoom ${camera.zoom.toFixed(2)} · 共享时刻 ${time.toFixed(3)}s · 两块翼面源网格 / 无身体`;
  state.dirty = false;
}

function tick(now) {
  if (state.failed) return;
  if (state.ready) {
    if (state.playing) {
      state.phase = (state.phase + (now - state.last) / 1000) % 1;
      state.dirty = true;
    }
    if (state.dirty) render();
  }
  state.last = now;
  requestAnimationFrame(tick);
}

// Read-only diagnostics shared by the validation script and the visible UI.
export function readPreviewState() {
  return {
    ready: state.ready, failed: state.failed, playing: state.playing,
    phase: state.phase, sampledTime: state.sampledTime,
    sourceTime: lastSourceTime, pixelTime: lastPixelTime,
    pixelSize: [pixelCanvas.width, pixelCanvas.height],
    sourceSize: [sourceCanvas.width, sourceCanvas.height],
    sharedCamera: !!controls && controls.every(c => c.object === camera && c.target === target),
    cameraPosition: camera?.position.toArray(), cameraQuaternion: camera?.quaternion.toArray(),
    projection: camera?.isOrthographicCamera ? 'orthographic' : 'perspective',
    zoom: camera?.zoom, meshNames: scene ? collectMeshNames(scene) : [],
    surfaceMaterials: [...surfaceMaterials].map(m => ({name: m.name, color: m.color.getHexString(), emissive: m.emissive.getHexString()})),
    settings: readSettings(), keyShadowEnabled: !!keyLight?.castShadow,
    sourceShadows: !!sourceRenderer?.shadowMap.enabled, pixelShadows: !!pixelRenderer?.shadowMap.enabled,
    shadowMapSize: keyLight?.shadow.map ? [keyLight.shadow.map.width, keyLight.shadow.map.height] : null,
  };
}

function collectMeshNames(root) {
  const names = [];
  root.traverse(object => { if (object.isMesh) names.push(object.name); });
  return names;
}

function readSettings() {
  return {
    upper: $('upper-color').value, lower: $('lower-color').value,
    shadows: $('shadows').checked,
    outline: {enabled: $('outline-enabled').checked, outside: $('outline-outside').checked,
      color: $('outline-color').value, alpha: Number($('outline-alpha').value)},
  };
}

function updateOutline() {
  const outline = readSettings().outline;
  pixelOutput?.setOutline(outline);
  for (const id of ['outline-color', 'outline-alpha', 'outline-outside']) $(id).disabled = !outline.enabled;
  $('outline-alpha-value').textContent = `${Math.round(outline.alpha*100)}%`;
  state.dirty = true;
}

function updateMaterials() {
  const settings = readSettings();
  for (const material of surfaceMaterials) {
    const color = material.name === 'Wing upper' ? settings.upper : settings.lower;
    material.color.set(settings.shadows ? color : '#000000');
    material.emissive.set(settings.shadows ? '#000000' : color);
    material.emissiveIntensity = 1;
    material.metalness = 0;
    material.roughness = 1;
    material.needsUpdate = true;
  }
  if (keyLight) {
    keyLight.intensity = settings.shadows ? 2.4 : 0;
    fillLight.intensity = settings.shadows ? .65 : 0;
    keyLight.castShadow = settings.shadows;
    sourceRenderer.shadowMap.enabled = pixelRenderer.shadowMap.enabled = settings.shadows;
  }
  state.dirty = true;
}

async function init() {
  scene = new THREE.Scene();
  camera = createCamera($('projection').value);
  sourceRenderer = new THREE.WebGLRenderer({canvas: sourceCanvas, alpha: true, antialias: true});
  // Retain only the tiny pixel buffer so the PNG download represents the exact
  // last displayed pixel frame, including transparency, rather than a screenshot.
  pixelRenderer = new THREE.WebGLRenderer({canvas: pixelCanvas, alpha: true, antialias: false, preserveDrawingBuffer: true});
  for (const renderer of [sourceRenderer, pixelRenderer]) {
    renderer.setClearColor(0, 0);
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    renderer.toneMapping = THREE.NoToneMapping;
  }
  pixelOutput = createPixelOutput(pixelRenderer, state.resolution);
  keyLight = new THREE.DirectionalLight(0xffffff, 2.4);
  keyLight.position.set(3.8, 5, 2.8);
  keyLight.shadow.mapSize.set(1024, 1024);
  Object.assign(keyLight.shadow.camera, {left: -2, right: 2, top: 2, bottom: -2, near: .1, far: 15});
  keyLight.shadow.camera.updateProjectionMatrix();
  keyLight.shadow.bias = -.0003;
  keyLight.shadow.normalBias = .015;
  fillLight = new THREE.AmbientLight(0xffffff, .65);
  scene.add(keyLight, keyLight.target, fillLight);
  for (const renderer of [sourceRenderer, pixelRenderer]) renderer.shadowMap.type = THREE.PCFShadowMap;
  controls = [sourceCanvas, pixelCanvas].map(canvas => {
    const control = new OrbitControls(camera, canvas);
    control.target = target;
    control.enableDamping = false;
    control.enablePan = false;
    control.minDistance = 3.5;
    control.maxDistance = 12;
    control.minZoom = .6;
    control.maxZoom = 2.5;
    control.minPolarAngle = .01;
    control.maxPolarAngle = Math.PI - .01;
    // Arrow keys rotate the shared view instead of moving the page or panning.
    control.keyPanSpeed = 0;
    canvas.addEventListener('keydown', event => {
      const delta = {ArrowLeft: [-.12, 0], ArrowRight: [.12, 0], ArrowUp: [0, -.12], ArrowDown: [0, .12]}[event.key];
      if (!delta) return;
      event.preventDefault();
      const spherical = new THREE.Spherical().setFromVector3(camera.position.clone().sub(target));
      spherical.theta += delta[0];
      spherical.phi = THREE.MathUtils.clamp(spherical.phi + delta[1], .01, Math.PI-.01);
      camera.position.setFromSpherical(spherical).add(target);
      camera.lookAt(target);
      control.update();
      state.dirty = true;
    });
    control.addEventListener('change', () => { state.dirty = true; });
    return control;
  });
  initializeView();
  resizeObserver = new ResizeObserver(resize);
  resizeObserver.observe(document.querySelector('.controls-panel'));
  for (const canvas of [sourceCanvas, pixelCanvas]) {
    resizeObserver.observe(canvas.parentElement);
    canvas.addEventListener('webglcontextlost', event => {
      event.preventDefault();
      fail(new Error('WebGL 上下文丢失'));
    });
  }
  window.addEventListener('resize', resize);
  resize();
  const gltf = await new GLTFLoader().loadAsync('blender-v5/butterfly-prototype.glb');
  if (state.failed) return; // Do not revive controls after a context loss during loading.
  if (gltf.animations.length !== 1) throw new Error('GLB 必须包含一个共享动画');
  if (gltf.animations[0].duration !== 1) throw new Error('GLB 动画必须为一秒循环');
  const forbidden = /head|thorax|abdomen|body/i;
  gltf.scene.traverse(object => {
    if (forbidden.test(object.name)) throw new Error(`源模型仍有身体节点：${object.name}`);
    if (object.isMesh) {
      object.castShadow = object.receiveShadow = true;
      const materials = Array.isArray(object.material) ? object.material : [object.material];
      for (const material of materials) {
        if (!['Wing upper', 'Wing lower'].includes(material.name)) throw new Error(`未标记的翼面材质：${material.name}`);
        surfaceMaterials.add(material);
      }
    }
  });
  if (new Set([...surfaceMaterials].map(m => m.name)).size !== 2) throw new Error('必须包含上/下两种表面材质');
  scene.add(gltf.scene);
  updateMaterials();
  updateOutline();
  mixer = new THREE.AnimationMixer(gltf.scene);
  mixer.clipAction(gltf.animations[0]).play();
  state.ready = true;
  for (const id of ['play', 'download']) $(id).disabled = false;
  setPlaying(state.playing);
  status.textContent = '材质与像素边框已就绪 · 在任意画面中拖动，另一幅同步 · 参数刷新后重置';
  render();
  requestAnimationFrame(tick);
}

$('play').addEventListener('click', () => setPlaying(!state.playing));
$('phase').addEventListener('input', event => {
  setPlaying(false);
  state.phase = Number(event.target.value) / 1000;
  state.dirty = true;
});
$('sampling').addEventListener('change', () => { state.dirty = true; });
$('resolution').addEventListener('input', event => {
  state.resolution = Number(event.target.value);
  resize();
});
$('background').addEventListener('input', event => {
  document.documentElement.style.setProperty('--stage', event.target.value);
});
$('projection').addEventListener('change', event => {
  if (!controls) return;
  const previous = camera;
  camera = createCamera(event.target.value);
  camera.position.copy(previous.position);
  camera.quaternion.copy(previous.quaternion);
  camera.zoom = previous.zoom;
  camera.updateProjectionMatrix();
  for (const control of controls) control.object = camera;
  controls[0].update();
  controls[1].update();
  state.dirty = true;
});
$('download').addEventListener('click', () => {
  if (!state.ready || state.failed) return;
  render();
  const name = `butterfly-v5-${state.resolution}px-${state.sampledTime.toFixed(3)}s.png`;
  pixelCanvas.toBlob(blob => {
    if (!blob) return;
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url; link.download = name; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }, 'image/png');
});
for (const id of ['upper-color', 'lower-color']) $(id).addEventListener('input', updateMaterials);
$('shadows').addEventListener('change', updateMaterials);
for (const id of ['outline-enabled', 'outline-outside']) $(id).addEventListener('change', updateOutline);
for (const id of ['outline-color', 'outline-alpha']) $(id).addEventListener('input', updateOutline);
document.addEventListener('visibilitychange', () => { state.last = performance.now(); });
init().catch(fail);
