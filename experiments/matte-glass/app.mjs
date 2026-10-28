import { TINTS, PRESETS, RANGES, DEFAULTS, normalizeSettings, activePreset, materialProperties, materialRecipe } from './material.mjs';

const $ = selector => document.querySelector(selector);
const $$ = selector => [...document.querySelectorAll(selector)];
const storageKey = 're-flora.matte-glass.v1';
let settings = { ...DEFAULTS };
try { settings = normalizeSettings(JSON.parse(localStorage.getItem(storageKey))); } catch { /* Private browsing or invalid saved state: use defaults. */ }
const stage = $('#stage');
let toastTimer;
let selectedSeed = 'daisy';
let seeds = 128;
const inventory = { daisy: 12, poppy: 8, fern: 6 };
let customBackdropUrl;
let backdropRequest = 0;

// Stable, low-resolution surface texture. No per-frame noise or external assets.
function makeGrain() {
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 32;
  const context = canvas.getContext('2d');
  const pixels = context.createImageData(32, 32);
  let seed = 1837;
  for (let i = 0; i < pixels.data.length; i += 4) {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    const level = (seed >>> 24) < 128 ? 90 : 220;
    pixels.data[i] = pixels.data[i + 1] = pixels.data[i + 2] = level;
    pixels.data[i + 3] = 140;
  }
  context.putImageData(pixels, 0, 0);
  stage.style.setProperty('--grain-map', `url("${canvas.toDataURL()}")`);
}

function render() {
  for (const [name, value] of Object.entries(materialProperties(settings))) stage.style.setProperty(name, value);
  stage.dataset.scene = settings.scene;
  for (const key of Object.keys(RANGES)) {
    $(`#${key}`).value = settings[key];
    $(`#${key}-value`).value = `${settings[key]}${key === 'frost' || key === 'pixel' ? ' px' : '%'}`;
  }
  for (const key of ['tinted', 'compare']) $(`#${key}`).checked = settings[key];
  $('#tint-options').disabled = !settings.tinted;
  $('#tint-name').textContent = settings.tinted ? TINTS[settings.tint].name : 'Untinted';
  const preset = activePreset(settings);
  $$('[data-preset]').forEach(button => button.setAttribute('aria-pressed', button.dataset.preset === preset));
  $$('[data-tint]').forEach(button => button.setAttribute('aria-pressed', button.dataset.tint === settings.tint));
  $$('[data-scene]').forEach(button => button.setAttribute('aria-pressed', button.dataset.scene === settings.scene));
  $('#material-name').textContent = `${settings.tinted ? TINTS[settings.tint].name : 'Untinted'} · ${preset ? PRESETS[preset].name : 'Custom'}`.toUpperCase();
  $('#specimen').classList.toggle('neutral', settings.compare);
  $('#specimen-label').textContent = settings.compare ? 'UNTINTED REFERENCE' : 'A QUIET LITTLE DETAIL';
  $('#specimen-title').innerHTML = settings.compare ? 'Same frost.<br>No added color.' : 'Light comes through.<br>The noise stays out.';
  $('#specimen-copy').innerHTML = settings.compare ? 'Only the body color changes.<br>Texture and opacity stay equal.' : 'Frosted, not glossy.<br>Made to belong in the garden.';
  $('#specimen-meta').textContent = settings.compare ? '02 / NEUTRAL SURFACE' : '01 / MATTE SURFACE';
  const scenes = { day: ['A slow afternoon', 'Day 12 · Clear skies', 'DAYLIGHT'], dusk: ['Just before the stars', 'Day 12 · Evening hush', 'DUSK'], night: ['Under a quiet moon', 'Day 12 · Starlit skies', 'NIGHT'] };
  const [time, weather, label] = scenes[settings.scene];
  $('#scene-time').textContent = time;
  $('#scene-weather').textContent = weather;
  $('#scene-note').textContent = `${label} / GARDEN STUDY`;
  try { localStorage.setItem(storageKey, JSON.stringify(settings)); } catch { /* Controls still work without persistence. */ }
}

function update(patch) {
  settings = normalizeSettings({ ...settings, ...patch });
  render();
}

function notify(message) {
  clearTimeout(toastTimer);
  $('#toast').textContent = message;
  $('#toast').hidden = false;
  toastTimer = setTimeout(() => { $('#toast').hidden = true; }, 3200);
}

for (const key of Object.keys(RANGES)) $(`#${key}`).addEventListener('input', event => update({ [key]: Number(event.target.value) }));
for (const key of ['tinted', 'compare']) $(`#${key}`).addEventListener('change', event => update({ [key]: event.target.checked }));
$$('[data-preset]').forEach(button => button.addEventListener('click', () => update(PRESETS[button.dataset.preset])));
$$('[data-tint]').forEach(button => button.addEventListener('click', () => update({ tint: button.dataset.tint })));
$$('[data-scene]').forEach(button => button.addEventListener('click', () => update({ scene: button.dataset.scene })));
$('#reset').addEventListener('click', () => { settings = { ...DEFAULTS }; render(); });

const descriptions = { daisy: 'Daisy · happiest in the sunshine', poppy: 'Poppy · a little color goes a long way', fern: 'Fern · a soft spot in the shade' };
function renderSeeds() {
  $$('[data-seed]').forEach(button => {
    button.setAttribute('aria-pressed', button.dataset.seed === selectedSeed);
    button.querySelector('small').textContent = `× ${inventory[button.dataset.seed]}`;
  });
  $('#plant-description').textContent = descriptions[selectedSeed];
  $('#seed-count').textContent = seeds;
  $('#plant').disabled = inventory[selectedSeed] === 0 || seeds < 4;
}
$$('[data-seed]').forEach(button => button.addEventListener('click', () => { selectedSeed = button.dataset.seed; renderSeeds(); }));
$('#plant').addEventListener('click', () => {
  if (inventory[selectedSeed] === 0 || seeds < 4) return;
  inventory[selectedSeed]--;
  seeds -= 4;
  renderSeeds();
  notify(`${selectedSeed[0].toUpperCase() + selectedSeed.slice(1)} planted. A small beginning. ✳`);
});
function selectTool(button) {
  $$('[data-tool]').forEach(item => item.setAttribute('aria-pressed', item === button));
  $('#tool-name').textContent = button.dataset.tool;
}
$$('[data-tool]').forEach(button => button.addEventListener('click', () => selectTool(button)));
document.addEventListener('keydown', event => {
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || $('#recipe-dialog').open || event.target.closest('input, textarea, select, button, [contenteditable]')) return;
  const tool = $$('[data-tool]')[Number(event.key) - 1];
  if (tool) selectTool(tool);
});

$('#backdrop').addEventListener('change', async event => {
  const file = event.target.files[0];
  if (!file) return;
  const request = ++backdropRequest;
  // Bound decode cost; all files stay local and are never uploaded or persisted.
  if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type) || file.size > 20 * 1024 * 1024) {
    notify('Choose a PNG, JPEG or WebP under 20 MB.');
    event.target.value = '';
    return;
  }
  const url = URL.createObjectURL(file);
  const image = new Image();
  image.src = url;
  try {
    await image.decode();
    if (request !== backdropRequest) { URL.revokeObjectURL(url); return; }
    if (image.naturalWidth * image.naturalHeight > 40_000_000) throw new Error('Image too large');
    if (customBackdropUrl) URL.revokeObjectURL(customBackdropUrl);
    customBackdropUrl = url;
    $('#landscape').style.backgroundImage = `url("${url}")`;
    $('#restore-backdrop').hidden = false;
  } catch {
    URL.revokeObjectURL(url);
    if (request === backdropRequest) notify('Could not open that image. Try a smaller screenshot.');
  }
  event.target.value = '';
});
$('#restore-backdrop').addEventListener('click', () => {
  backdropRequest++;
  if (customBackdropUrl) URL.revokeObjectURL(customBackdropUrl);
  customBackdropUrl = undefined;
  $('#landscape').style.backgroundImage = '';
  $('#restore-backdrop').hidden = true;
});

$('#export').addEventListener('click', () => {
  $('#recipe').value = JSON.stringify(materialRecipe(settings), null, 2);
  $('#copy-status').textContent = '';
  $('#recipe-dialog').showModal();
});
$('#close-recipe').addEventListener('click', () => $('#recipe-dialog').close());
$('#copy-recipe').addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText($('#recipe').value);
    $('#copy-status').textContent = 'Copied.';
  } catch {
    $('#recipe').focus();
    $('#recipe').select();
    $('#copy-status').textContent = 'Select and copy, or download.';
  }
});
$('#download-recipe').addEventListener('click', () => {
  const url = URL.createObjectURL(new Blob([$('#recipe').value], { type: 'application/json' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = 're-flora-matte-glass.json';
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
});

$('#compatibility').hidden = CSS.supports('backdrop-filter', 'blur(1px)') || CSS.supports('-webkit-backdrop-filter', 'blur(1px)');
makeGrain();
render();
renderSeeds();
