/* All illustrations are procedural Canvas paths; no imported image assets. */
(() => {
  'use strict';
  const { RopeSimulation, PRESETS, GROUND, ROCK } = window.RopeLab;
  const sim = new RopeSimulation();
  const $ = id => document.getElementById(id);
  const canvas = $('scene'), ctx = canvas.getContext('2d');
  const descriptions = {
    hemp: ['重力之下，自然垂落', '低抗弯 · 两端固定 · 拖起中段再松手', '01 / 松弛，不是拉伸。', '麻绳的柔软来自弯曲，而非橡皮筋式变长。试着拖起中间一段，观察重力如何重新分配弧线。'],
    hose: ['绕过石头，留住弧度', '高抗弯 · 粗管壁 · 试着将抗弯调低', '02 / 更挺括，但不是硬棍。', '试着提起水管、绕过石头，或调低抗弯比较轮廓。这里只表现弯曲差异；还没有截面扭转、盘卷记忆或管内水流。'],
    kite: ['让风牵起一根线', '轻线 · 自由负载 · 无风时风筝会落下', '03 / 风筝拉线，不是线把风筝推上去。', '增加风力，看线被逐渐拉紧；把风降到零，风筝会回落。端点受示意升力与阻力驱动，不是预设运动路径。']
  };
  let paused = matchMedia('(prefers-reduced-motion: reduce)').matches;
  let showNodes = false, last = 0, lastMetrics = 0;
  function updatePause() {
    $('pause').textContent = paused ? '继续' : '暂停';
    $('pause').setAttribute('aria-pressed', String(paused));
    $('sceneState').textContent = paused ? 'PAUSED / XPBD' : 'LIVE / XPBD';
  }
  function syncControls() {
    for (const name of ['wind', 'bend', 'length']) { $(name).value = sim.settings[name]; updateValue(name); }
    $('length').min = sim.mode === 'kite' ? 350 : sim.mode === 'hose' ? 720 : 660;
    $('length').max = sim.mode === 'kite' ? 550 : 950;
    // Set again after min/max changes: browsers clamp values against the old range.
    $('length').value = sim.settings.length; updateValue('length');
    $('pin').checked = sim.settings.pin;
    $('collision').checked = sim.settings.collision;
    const d = descriptions[sim.mode];
    ['sceneTitle', 'sceneSubtitle', 'captionTitle', 'caption'].forEach((id, i) => { $(id).textContent = d[i]; });
    document.querySelectorAll('[data-mode]').forEach(button => button.setAttribute('aria-pressed', String(button.dataset.mode === sim.mode)));
  }
  function updateValue(name) { $(name + 'Value').textContent = $(name).value + (name === 'length' ? '' : '%'); }
  for (const name of ['wind', 'bend', 'length']) $(name).addEventListener('input', () => { sim.settings[name] = Number($(name).value); updateValue(name); });
  document.querySelectorAll('[data-mode]').forEach(button => button.addEventListener('click', () => { sim.reset(button.dataset.mode); syncControls(); draw(); }));
  $('pin').addEventListener('change', () => sim.setPinned($('pin').checked));
  $('collision').addEventListener('change', () => { sim.settings.collision = $('collision').checked; });
  $('nodes').addEventListener('change', () => { showNodes = $('nodes').checked; draw(); });
  $('pause').addEventListener('click', () => { paused = !paused; updatePause(); });
  $('gust').addEventListener('click', () => { sim.gust(); paused = false; updatePause(); });
  $('reset').addEventListener('click', () => { sim.reset(); syncControls(); draw(); });
  function pointer(event) {
    const rect = canvas.getBoundingClientRect();
    const scale = Math.min(rect.width / 1000, rect.height / 570);
    return { x: (event.clientX - rect.left - (rect.width - 1000 * scale) / 2) / scale, y: (event.clientY - rect.top - (rect.height - 570 * scale) / 2) / scale };
  }
  canvas.addEventListener('pointerdown', event => {
    const p = pointer(event);
    if (sim.beginDrag(p.x, p.y) >= 0) { canvas.setPointerCapture(event.pointerId); canvas.focus({ preventScroll: true }); }
  });
  canvas.addEventListener('pointermove', event => { const p = pointer(event); sim.moveDrag(p.x, p.y); });
  for (const name of ['pointerup', 'pointercancel', 'lostpointercapture', 'blur']) canvas.addEventListener(name, () => sim.endDrag());
  canvas.addEventListener('keydown', event => {
    if (event.key === 'Escape') { sim.endDrag(); return; }
    const direction = { ArrowLeft: [-18, 0], ArrowRight: [18, 0], ArrowUp: [0, -18], ArrowDown: [0, 18] }[event.key];
    if (direction) {
      event.preventDefault();
      if (!sim.drag) { const p = sim.points[24]; sim.beginDrag(p.x, p.y); }
      if (sim.drag) sim.moveDrag(sim.drag.x + direction[0], sim.drag.y + direction[1]);
    }
  });
  document.addEventListener('visibilitychange', () => { last = 0; sim.endDrag(); });
  function stroke(points, color, width, dash = []) {
    ctx.beginPath(); points.forEach((p, i) => i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y));
    ctx.strokeStyle = color; ctx.lineWidth = width; ctx.setLineDash(dash); ctx.stroke(); ctx.setLineDash([]);
  }
  function ellipse(x, y, rx, ry, color) { ctx.beginPath(); ctx.ellipse(x, y, rx, ry, 0, 0, Math.PI * 2); ctx.fillStyle = color; ctx.fill(); }
  function text(str, x, y, color = '#7b8971', size = 11) { ctx.font = `${size}px -apple-system, sans-serif`; ctx.fillStyle = color; ctx.fillText(str, x, y); }
  function plant(x, y, size, flip = 1) {
    ctx.save(); ctx.translate(x, y); ctx.scale(flip * size, size);
    stroke([{ x: 0, y: 0 }, { x: -3, y: -48 }], '#8d9b76', 2);
    for (let i = 0; i < 4; i++) {
      ctx.save(); ctx.translate(-i * 0.7, -12 - i * 10); ctx.rotate(i % 2 ? -0.5 : 0.5);
      ellipse(i % 2 ? -8 : 8, -3, 12, 4, i % 2 ? '#9cab88' : '#b8c49d'); ctx.restore();
    }
    ctx.restore();
  }
  function background() {
    ctx.fillStyle = '#edf0e5'; ctx.fillRect(0, 0, 1000, 570);
    const glow = ctx.createRadialGradient(700, 130, 10, 700, 130, 500);
    glow.addColorStop(0, '#f9f7e9'); glow.addColorStop(1, '#edf0e500'); ctx.fillStyle = glow; ctx.fillRect(0, 0, 1000, 570);
    ellipse(825, 117, 37, 37, '#e6e9d2');
    ctx.beginPath(); ctx.moveTo(0, 380); ctx.bezierCurveTo(120, 330, 200, 385, 330, 359); ctx.bezierCurveTo(510, 310, 750, 393, 1000, 331); ctx.lineTo(1000, 510); ctx.lineTo(0, 510); ctx.fillStyle = '#e1e7d5'; ctx.fill();
    ctx.beginPath(); ctx.moveTo(0, 413); ctx.bezierCurveTo(250, 376, 520, 430, 740, 395); ctx.bezierCurveTo(890, 371, 940, 400, 1000, 392); ctx.lineTo(1000, 510); ctx.lineTo(0, 510); ctx.fillStyle = '#d9e1cb'; ctx.fill();
    ctx.fillStyle = '#d0d9bc'; ctx.fillRect(0, GROUND, 1000, 90);
    stroke([{ x: 0, y: GROUND }, { x: 1000, y: GROUND }], '#aebd98', 1);
    for (let i = 0; i < 90; i++) {
      const x = (i * 131 + 73) % 1000, y = 487 + (i * 19) % 70;
      stroke([{ x, y }, { x: x + 4, y: y - 2 }], '#bdc9a8', 1);
    }
    for (const [x, size, flip] of [[55, 1.2, 1], [88, 0.6, -1], [918, 1, -1], [950, 0.65, 1], [355, 0.38, 1]]) plant(x, GROUND, size, flip);
    // The visible rock uses the same circle as the collision primitive.
    if (sim.settings.collision) {
      ellipse(ROCK.x + 8, GROUND + 3, 65, 9, '#7c8a6d26');
      const shade = ctx.createLinearGradient(ROCK.x - 40, ROCK.y - 40, ROCK.x + 40, ROCK.y + 50);
      shade.addColorStop(0, '#c8cbb7'); shade.addColorStop(0.5, '#b3b9a2'); shade.addColorStop(1, '#98a38c');
      ellipse(ROCK.x, ROCK.y, ROCK.radius, ROCK.radius, shade);
      stroke([{ x: 477, y: 413 }, { x: 506, y: 397 }, { x: 533, y: 410 }], '#dde0cb', 2);
      text('碰撞石', 493, 516, '#6f7e5d', 10);
    }
    if (showNodes) {
      ctx.strokeStyle = '#65735b18'; ctx.lineWidth = 1;
      for (let x = 0; x < 1000; x += 40) { ctx.beginPath(); ctx.moveTo(x, 80); ctx.lineTo(x, GROUND); ctx.stroke(); }
      for (let y = 80; y < GROUND; y += 40) { ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(1000, y); ctx.stroke(); }
    }
    if (sim.wind() > 0.01) for (let i = 0; i < 6; i++) {
      const x = ((sim.time * (26 + sim.wind() * 45) + i * 193) % 1120) - 60, y = 120 + (i * 71) % 220;
      stroke([{ x, y }, { x: x + 22 + sim.wind() * 20, y }], '#b4c2a85c', 1);
    }
    text('2D · 非游戏画面', 855, 545, '#7b8c66', 10);
  }
  function fixtures() {
    const a = sim.points[0], b = sim.points[48];
    if (sim.mode === 'hemp') {
      for (const p of sim.settings.pin ? [a, b] : [a]) {
        ellipse(p.x + 7, GROUND + 2, 28, 5, '#75866126');
        ctx.fillStyle = '#9a8a68'; ctx.fillRect(p.x - 7, p.y - 30, 14, GROUND - p.y + 30);
        ctx.fillStyle = '#b6a37b'; ctx.fillRect(p.x - 7, p.y - 30, 5, GROUND - p.y + 30);
        stroke([{ x: p.x - 4, y: p.y - 27 }, { x: p.x + 6, y: p.y - 27 }], '#d4c4a0', 2);
      }
    } else if (sim.mode === 'hose') {
      stroke([{ x: a.x - 24, y: GROUND }, { x: a.x - 24, y: a.y }, { x: a.x, y: a.y }], '#788878', 12);
      stroke([{ x: a.x - 25, y: a.y - 23 }, { x: a.x - 25, y: a.y - 6 }], '#6b7867', 5);
      stroke([{ x: a.x - 38, y: a.y - 23 }, { x: a.x - 12, y: a.y - 23 }], '#ac8060', 5);
    } else {
      stroke([{ x: a.x - 8, y: GROUND + 4 }, { x: a.x + 7, y: a.y - 18 }], '#a0835c', 10);
      stroke([{ x: a.x - 10, y: a.y - 12 }, { x: a.x + 16, y: a.y - 3 }], '#c2a173', 5);
    }
  }
  function kite() {
    const p = sim.points[48];
    ctx.save(); ctx.translate(p.x, p.y); ctx.rotate(0.18 * Math.sin(sim.time * 1.8) + 0.16);
    stroke([{ x: 0, y: 24 }, { x: 6 * Math.sin(sim.time * 3), y: 43 }, { x: 12 * Math.sin(sim.time * 2 + 1), y: 63 }, { x: 4 * Math.sin(sim.time * 3 + 2), y: 82 }], '#b78260', 1);
    ctx.beginPath(); ctx.moveTo(0, -53); ctx.lineTo(35, -17); ctx.lineTo(0, 25); ctx.lineTo(-35, -17); ctx.closePath(); ctx.fillStyle = '#c77753'; ctx.fill();
    ctx.beginPath(); ctx.moveTo(0, -53); ctx.lineTo(0, 25); ctx.lineTo(-35, -17); ctx.closePath(); ctx.fillStyle = '#ddaa7c'; ctx.fill();
    stroke([{ x: 0, y: -53 }, { x: 0, y: 25 }], '#f3d7aa', 1);
    stroke([{ x: -35, y: -17 }, { x: 35, y: -17 }], '#f3d7aa', 1);
    ctx.restore();
  }
  function draw() {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    if (canvas.width !== 1000 * dpr) { canvas.width = 1000 * dpr; canvas.height = 570 * dpr; }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0); ctx.lineCap = 'round'; ctx.lineJoin = 'round';
    background(); fixtures();
    const ps = sim.points;
    ctx.save(); ctx.translate(3, 7); stroke(ps, '#4f624119', sim.settings.radius * 2 + 4); ctx.restore();
    if (sim.mode === 'hemp') {
      stroke(ps, '#846239', 9); stroke(ps, '#bd995f', 6); stroke(ps, '#e0c58e', 2.5, [2, 5]);
    } else if (sim.mode === 'hose') {
      stroke(ps, '#4e6a50', 17); stroke(ps, '#779673', 12); ctx.save(); ctx.translate(-1, -3); stroke(ps, '#aabc91', 3); ctx.restore();
      const p = ps[48], prev = ps[47];
      ctx.save(); ctx.translate(p.x, p.y); ctx.rotate(Math.atan2(p.y - prev.y, p.x - prev.x));
      ctx.fillStyle = '#bb9858'; ctx.fillRect(-8, -10, 19, 20); ctx.fillStyle = '#d5b875'; ctx.fillRect(-4, -10, 4, 20); ctx.fillStyle = '#576857'; ctx.fillRect(10, -8, 15, 16); ctx.restore();
    } else { stroke(ps, '#627563', 2); kite(); }
    if (showNodes) {
      stroke(ps, '#445a7199', 1);
      ps.forEach((p, i) => { ellipse(p.x, p.y, 2.8, 2.8, '#fffdf3'); if (i % 8 === 0) text(String(i), p.x + 6, p.y - 9, '#47614b', 9); });
      if (sim.settings.collision) { ctx.beginPath(); ctx.arc(ROCK.x, ROCK.y, ROCK.radius + sim.settings.radius, 0, Math.PI * 2); ctx.strokeStyle = '#718467'; ctx.lineWidth = 1; ctx.setLineDash([3, 5]); ctx.stroke(); ctx.setLineDash([]); }
    }
    for (const i of [0, 48]) {
      const p = ps[i];
      ellipse(p.x, p.y, 8, 8, p.w === 0 ? '#f4f4e8' : '#d9e7be'); ellipse(p.x, p.y, 3, 3, '#526d49');
      text(i === 0 ? '固定端' : p.w === 0 ? '固定端' : '自由端', p.x + 14, p.y + 4, '#56704d', 10);
    }
    if (sim.drag) {
      const p = ps[sim.drag.index]; stroke([p, sim.drag], '#b66e48', 1, [3, 4]);
      ctx.beginPath(); ctx.arc(sim.drag.x, sim.drag.y, 13, 0, Math.PI * 2); ctx.strokeStyle = '#b66e48'; ctx.lineWidth = 1; ctx.stroke();
    }
  }
  function metrics() {
    const d = sim.diagnostics();
    $('particleCount').textContent = d.points;
    $('strain').textContent = (d.maxStrain * 100).toFixed(1) + '%';
    $('strain').style.color = d.maxStrain > 0.1 ? '#a45135' : '';
    $('restLength').textContent = d.length.toFixed(0);
  }
  function frame(now) {
    if (last && !paused && !document.hidden) sim.advance((now - last) / 1000);
    last = now; draw();
    if (now - lastMetrics > 150) { metrics(); lastMetrics = now; }
    requestAnimationFrame(frame);
  }
  // Read-only diagnostic surface for real-browser checks; no alternate solver path.
  window.ropeLabDiagnostics = () => ({ ...sim.diagnostics(), paused, settings: { ...sim.settings }, points: sim.points.map(p => ({ x: p.x, y: p.y })), dragging: sim.drag?.index ?? null });
  syncControls(); updatePause(); metrics(); requestAnimationFrame(frame);
})();
