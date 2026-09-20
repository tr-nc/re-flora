import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

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
let scene, camera, sourceRenderer, pixelRenderer, mixer, controls;
let lastSourceTime = null, lastPixelTime = null;
let resizeObserver;

function fail(error) {
  state.failed = true;
  state.playing = false;
  status.textContent = `预览无法继续：${error.message}。请检查 WebGL 支持后刷新。`;
  status.classList.add('error');
  for (const id of ['play', 'reset', 'download']) $(id).disabled = true;
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

function resetView() {
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
  const available = Math.min(384, sourceCanvas.parentElement.clientWidth, pixelCanvas.parentElement.clientWidth);
  const displaySize = Math.max(state.resolution, Math.floor(available / state.resolution) * state.resolution);
  sourceRenderer.setPixelRatio(Math.min(devicePixelRatio || 1, 2));
  sourceRenderer.setSize(displaySize, displaySize, false);
  pixelRenderer.setPixelRatio(1);
  pixelRenderer.setSize(state.resolution, state.resolution, false);
  for (const canvas of [sourceCanvas, pixelCanvas]) {
    canvas.style.width = canvas.style.height = `${displaySize}px`;
  }
  $('source-size').textContent = `原始 3D · ${sourceCanvas.width} × ${sourceCanvas.height} 渲染缓冲`;
  $('pixel-size').textContent = `${pixelCanvas.width} × ${pixelCanvas.height} 原生缓冲 → ${displaySize}px 显示 · ${displaySize / state.resolution}× 最近邻`;
  $('resolution-label').textContent = `${state.resolution} × ${state.resolution}`;
  state.dirty = true;
}

function render() {
  const time = $('sampling').value === 'stepped' ? Math.floor(state.phase * 5) / 5 : state.phase;
  mixer.setTime(time);
  scene.updateMatrixWorld(true);
  camera.updateMatrixWorld(true);
  state.sampledTime = time;
  // No screenshot or atlas input: rasterize the same scene independently into
  // the high-res canvas and the actual N×N, non-MSAA canvas.
  sourceRenderer.render(scene, camera);
  lastSourceTime = time;
  pixelRenderer.render(scene, camera);
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
  };
}

function collectMeshNames(root) {
  const names = [];
  root.traverse(object => { if (object.isMesh) names.push(object.name); });
  return names;
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
  resetView();
  resizeObserver = new ResizeObserver(resize);
  for (const canvas of [sourceCanvas, pixelCanvas]) {
    resizeObserver.observe(canvas.parentElement);
    canvas.addEventListener('webglcontextlost', event => {
      event.preventDefault();
      fail(new Error('WebGL 上下文丢失'));
    });
  }
  window.addEventListener('resize', resize);
  resize();
  const gltf = await new GLTFLoader().loadAsync('blender-v4/butterfly-prototype.glb');
  if (state.failed) return; // Do not revive controls after a context loss during loading.
  if (gltf.animations.length !== 1) throw new Error('GLB 必须包含一个共享动画');
  if (gltf.animations[0].duration !== 1) throw new Error('GLB 动画必须为一秒循环');
  const forbidden = /head|thorax|abdomen|body/i;
  gltf.scene.traverse(object => {
    if (forbidden.test(object.name)) throw new Error(`源模型仍有身体节点：${object.name}`);
  });
  scene.add(gltf.scene);
  mixer = new THREE.AnimationMixer(gltf.scene);
  mixer.clipAction(gltf.animations[0]).play();
  state.ready = true;
  for (const id of ['play', 'reset', 'download']) $(id).disabled = false;
  setPlaying(state.playing);
  status.textContent = '纯翼面模型已就绪 · 在任意画面中拖动，另一幅同步 · 两边相机和动画完全共享';
  render();
  requestAnimationFrame(tick);
}

$('play').addEventListener('click', () => setPlaying(!state.playing));
$('reset').addEventListener('click', resetView);
$('phase').addEventListener('input', event => {
  setPlaying(false);
  state.phase = Number(event.target.value) / 1000;
  state.dirty = true;
});
$('sampling').addEventListener('change', () => { state.dirty = true; });
$('resolution').addEventListener('change', event => {
  state.resolution = Number(event.target.value);
  resize();
});
$('background').addEventListener('change', event => {
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
  const name = `butterfly-v4-${state.resolution}px-${state.sampledTime.toFixed(3)}s.png`;
  pixelCanvas.toBlob(blob => {
    if (!blob) return;
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url; link.download = name; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }, 'image/png');
});
document.addEventListener('visibilitychange', () => { state.last = performance.now(); });
init().catch(fail);
