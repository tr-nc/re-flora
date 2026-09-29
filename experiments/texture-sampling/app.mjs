import { clamp, texel, neighbors, nearest, bilinear, bicubic, area, srgbEncode, makeTexture, makeMipmaps, mipSelection, sampleMip, trilinear, lodForScale } from './sampling.mjs';
const $ = id => document.getElementById(id);
const names = ['A', 'B', 'C', 'D'];
let texture = makeTexture('colors'), playing = false, lastTime = 0, dirty = true;
let mipmaps = makeMipmaps(texture);
const source = $('source'), ctx = source.getContext('2d');
const views = ['nearest', 'bilinear', 'trilinear', 'bicubic', 'area'].map(id => {
  const canvas = $(id), context = canvas.getContext('2d');
  return { id, context, image: context.createImageData(96, 96) };
});
const rgb = c => `rgb(${c.map(v => Math.round(srgbEncode(v))).join(',')})`;
function currentLod() {
  return mipSelection(mipmaps, $('auto-lod').checked ? lodForScale(+$('scale').value) : +$('lod').value);
}
function buildMipPreviews() {
  $('mip-levels').innerHTML = mipmaps.map((t, i) => `<div class="mip-level" id="mip-level-${i}"><strong>L${i} · ${t.width}×${t.height}</strong><canvas width="128" height="128" id="mip-canvas-${i}" aria-label="Mip 层 ${i}"></canvas><small id="mip-weight-${i}"></small></div>`).join('');
}
function drawMips() {
  const selection = currentLod(), { lod, lower, upper, blend } = selection;
  const x = +$('sample-x').value, y = +$('sample-y').value;
  $('lod').value = lod; $('lod-value').textContent = lod.toFixed(2);
  const description = `L${lower} × ${((1 - blend) * 100).toFixed(1)}% + L${upper} × ${(blend * 100).toFixed(1)}%`;
  $('tri-status').textContent = ` 当前：${description}`;
  $('lod-explanation').textContent = `${$('auto-lod').checked ? '自动：由缩放估算' : '手动：仅覆盖 Trilinear 的层级选择'} · LOD = ${lod.toFixed(2)} · ${description}`;
  mipmaps.forEach((t, level) => {
    const c = $(`mip-canvas-${level}`).getContext('2d'), size = 128 / t.width;
    for (let j = 0; j < t.height; j++) for (let i = 0; i < t.width; i++) {
      c.fillStyle = rgb(texel(t, i, j)); c.fillRect(i * size, j * size, size, size);
    }
    const weight = (level === lower ? 1 - blend : 0) + (level === upper ? blend : 0);
    $(`mip-level-${level}`).classList.toggle('active', weight > 0);
    $(`mip-weight-${level}`).textContent = `层权重 ${(weight * 100).toFixed(1)}%`;
    const mx = (x + 0.5) * t.width / texture.width - 0.5, my = (y + 0.5) * t.height / texture.height - 0.5;
    if (level === lower || level === upper) for (const p of neighbors(mx, my)) {
      c.strokeStyle = '#e6ffaf'; c.lineWidth = 1;
      c.strokeRect(clamp(p.x, 0, t.width - 1) * size + 1, clamp(p.y, 0, t.height - 1) * size + 1, size - 2, size - 2);
    }
    const sx = (mx + 0.5) * size, sy = (my + 0.5) * size;
    for (const [color, width] of [['#000', 4], ['#fff', 1.5]]) {
      c.strokeStyle = color; c.lineWidth = width; c.beginPath(); c.moveTo(sx - 6, sy); c.lineTo(sx + 6, sy); c.moveTo(sx, sy - 6); c.lineTo(sx, sy + 6); c.stroke();
    }
  });
  $('mip-lower-chip').style.background = rgb(sampleMip(mipmaps, lower, x, y));
  $('mip-upper-chip').style.background = rgb(sampleMip(mipmaps, upper, x, y));
  $('mip-result-chip').style.background = rgb(trilinear(mipmaps, x, y, lod));
  $('mip-lower-label').textContent = `L${lower} 的 Bilinear`;
  $('mip-upper-label').textContent = `L${upper} 的 Bilinear`;
  $('mip-formula').textContent = `t = LOD − floor(LOD) = ${blend.toFixed(2)}；结果 = Bilinear(L${lower}, UV) × ${(1 - blend).toFixed(2)} + Bilinear(L${upper}, UV) × ${blend.toFixed(2)}。${lower === upper ? '已到最末层，实际只需这一层。' : '概念上是 4 + 4 个纹素贡献，不是原图上 8 个最近邻。'}`;
}
function drawInspector() {
  const x = +$('sample-x').value, y = +$('sample-y').value;
  $('x-value').textContent = x.toFixed(2); $('y-value').textContent = y.toFixed(2);
  const fx = x - Math.floor(x), fy = y - Math.floor(y);
  const points = neighbors(x, y), size = 30;
  ctx.clearRect(0, 0, 480, 480);
  for (let j = 0; j < 16; j++) for (let i = 0; i < 16; i++) {
    ctx.fillStyle = rgb(texel(texture, i, j)); ctx.fillRect(i * size, j * size, size, size);
    ctx.strokeStyle = '#ffffff22'; ctx.lineWidth = 1; ctx.strokeRect(i * size, j * size, size, size);
    ctx.fillStyle = '#ffffff66'; ctx.beginPath(); ctx.arc((i + 0.5) * size, (j + 0.5) * size, 1.5, 0, Math.PI * 2); ctx.fill();
  }
  points.forEach((p, index) => {
    const px = clamp(p.x, 0, 15), py = clamp(p.y, 0, 15);
    ctx.strokeStyle = '#111'; ctx.lineWidth = 5; ctx.strokeRect(px * size + 2, py * size + 2, size - 4, size - 4);
    ctx.strokeStyle = '#e6ffaf'; ctx.lineWidth = 2; ctx.strokeRect(px * size + 2, py * size + 2, size - 4, size - 4);
    ctx.font = 'bold 13px system-ui'; ctx.textAlign = 'left'; ctx.lineWidth = 3; ctx.strokeStyle = '#111';
    ctx.strokeText(names[index], px * size + 4, py * size + 13); ctx.fillStyle = '#fff'; ctx.fillText(names[index], px * size + 4, py * size + 13);
  });
  const sx = (x + 0.5) * size, sy = (y + 0.5) * size;
  for (const [color, width] of [['#000', 5], ['#fff', 2]]) {
    ctx.strokeStyle = color; ctx.lineWidth = width; ctx.beginPath(); ctx.moveTo(sx - 10, sy); ctx.lineTo(sx + 10, sy); ctx.moveTo(sx, sy - 10); ctx.lineTo(sx, sy + 10); ctx.stroke();
  }
  $('coordinates').textContent = `小数部分 fx = ${fx.toFixed(2)}，fy = ${fy.toFixed(2)} · UV = (${((x + 0.5) / 16).toFixed(4)}, ${((y + 0.5) / 16).toFixed(4)})`;
  const expressions = ['(1−fx)(1−fy)', 'fx(1−fy)', '(1−fx)fy', 'fx·fy'];
  $('weights').innerHTML = points.map((p, i) => `<div class="weight"><span class="swatch" style="background:${rgb(texel(texture, p.x, p.y))}"></span>${names[i]} (${clamp(p.x, 0, 15)}, ${clamp(p.y, 0, 15)})<br><small>${expressions[i]}</small> <strong>${(p.weight * 100).toFixed(1)}%</strong><div class="bar"><i style="width:${p.weight * 100}%"></i></div></div>`).join('');
  $('formula').textContent = `上行 = A·(1−fx) + B·fx\n下行 = C·(1−fx) + D·fx\n结果 = 上行·(1−fy) + 下行·fy\n= ${points.map((p, i) => `${p.weight.toFixed(4)}·${names[i]}`).join(' + ')}`;
  $('formula').style.whiteSpace = 'pre-line';
  $('nearest-chip').style.background = rgb(nearest(texture, x, y));
  $('bilinear-chip').style.background = rgb(bilinear(texture, x, y));
  drawMips();
}
function drawComparison() {
  const scale = +$('scale').value, radians = +$('angle').value * Math.PI / 180, shift = +$('shift').value;
  const cos = Math.cos(radians), sin = Math.sin(radians);
  const dx = [cos / scale, -sin / scale], dy = [sin / scale, cos / scale];
  const { lod } = currentLod();
  drawMips();
  $('scale-value').textContent = `${scale.toFixed(2)} px/texel`;
  $('angle-value').textContent = `${$('angle').value}°`;
  $('shift-value').textContent = `${shift.toFixed(2)} texel`;
  $('footprint').textContent = `每个输出像素覆盖约 ${(1 / scale).toFixed(2)} × ${(1 / scale).toFixed(2)} 纹素。白框是原始 16×16 纹理范围，框外延伸边缘颜色。下方自动移动独立于上方白十字。`;
  for (const view of views) {
    for (let j = 0; j < 96; j++) for (let i = 0; i < 96; i++) {
      const px = i + 0.5 - 48, py = j + 0.5 - 48;
      const x = 7.5 + px * dx[0] + py * dy[0] - shift;
      const y = 7.5 + px * dx[1] + py * dy[1];
      const color = view.id === 'trilinear' ? trilinear(mipmaps, x, y, lod)
        : view.id === 'area' ? area(texture, x, y, dx, dy) : ({ nearest, bilinear, bicubic }[view.id])(texture, x, y);
      const offset = (j * 96 + i) * 4;
      for (let k = 0; k < 3; k++) view.image.data[offset + k] = Math.round(srgbEncode(color[k]));
      view.image.data[offset + 3] = 255;
    }
    view.context.putImageData(view.image, 0, 0);
    // A thin range marker, not part of the sampled texture.
    const c = view.context;
    c.save(); c.translate(48, 48); c.rotate(radians); c.scale(scale, scale); c.translate(shift, 0);
    c.strokeStyle = '#ffffff70'; c.lineWidth = 0.5 / scale; c.strokeRect(-8, -8, 16, 16); c.restore();
    if ($('magnify').checked) {
      c.imageSmoothingEnabled = false;
      c.drawImage(c.canvas, 36, 36, 24, 24, 0, 0, 96, 96);
    }
  }
}
function setPlaying(value) { playing = value; $('play').textContent = value ? '⏸ 暂停慢移' : '▶ 自动慢移'; $('play').setAttribute('aria-pressed', String(value)); }
function updateTexture() { texture = makeTexture($('texture').value); mipmaps = makeMipmaps(texture); buildMipPreviews(); drawInspector(); dirty = true; }
$('texture').addEventListener('change', updateTexture);
for (const id of ['sample-x', 'sample-y']) $(id).addEventListener('input', drawInspector);
for (const id of ['scale', 'angle', 'shift']) $(id).addEventListener('input', () => { dirty = true; if (id === 'shift') setPlaying(false); });
$('play').addEventListener('click', () => setPlaying(!playing));
$('magnify').addEventListener('change', () => { dirty = true; });
$('auto-lod').addEventListener('change', () => { dirty = true; drawMips(); });
$('lod').addEventListener('input', () => { $('auto-lod').checked = false; dirty = true; drawMips(); });
function preset(kind, scale, angle, play) {
  $('texture').value = kind; $('scale').value = scale; $('angle').value = angle; $('shift').value = 0;
  $('magnify').checked = scale < 1;
  $('auto-lod').checked = true;
  phase = 0;
  $('sample-x').value = 7.35; $('sample-y').value = 7.65;
  setPlaying(play); updateTexture();
}
$('reset').addEventListener('click', () => preset('colors', 4, 0, false));
$('preset-mix').addEventListener('click', () => preset('colors', 5, 0, false));
$('preset-stem').addEventListener('click', () => preset('stem', 2, 18, true));
$('preset-mini').addEventListener('click', () => preset('checker', 0.65, 12, true));
$('preset-mip').addEventListener('click', () => {
  preset('stem', 2, 0, false);
  $('auto-lod').checked = false; $('lod').value = 1.5; drawMips();
  $('mip-section').scrollIntoView({ behavior: 'smooth', block: 'start' });
});
function point(event) {
  const rect = source.getBoundingClientRect();
  $('sample-x').value = clamp((event.clientX - rect.left) / rect.width * 16 - 0.5, 0, 15);
  $('sample-y').value = clamp((event.clientY - rect.top) / rect.height * 16 - 0.5, 0, 15);
  drawInspector();
}
source.addEventListener('pointerdown', event => { source.focus(); source.setPointerCapture(event.pointerId); point(event); });
source.addEventListener('pointermove', event => { if (source.hasPointerCapture(event.pointerId)) point(event); });
source.addEventListener('pointerup', event => { if (source.hasPointerCapture(event.pointerId)) source.releasePointerCapture(event.pointerId); });
source.addEventListener('keydown', event => {
  const moves = { ArrowLeft: [-0.05, 0], ArrowRight: [0.05, 0], ArrowUp: [0, -0.05], ArrowDown: [0, 0.05] };
  if (!moves[event.key]) return;
  event.preventDefault(); const [x, y] = moves[event.key];
  $('sample-x').value = clamp(+$('sample-x').value + x, 0, 15); $('sample-y').value = clamp(+$('sample-y').value + y, 0, 15); drawInspector();
});
let phase = 0;
function frame(time) {
  const dt = Math.min((time - lastTime) / 1000, 0.05); lastTime = time;
  if (playing && !document.hidden) { phase += dt * 0.6; $('shift').value = Math.sin(phase) * 1.8; dirty = true; }
  if (dirty) { drawComparison(); dirty = false; }
  requestAnimationFrame(frame);
}
buildMipPreviews(); drawInspector(); requestAnimationFrame(frame);
