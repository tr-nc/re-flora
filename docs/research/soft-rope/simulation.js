/* Standalone 2D exploration, not the game's physics implementation. */
(function (root) {
  'use strict';
  const STEP = 1 / 240;
  const GROUND = 480;
  const ROCK = { x: 510, y: 432, radius: 48 };
  const PRESETS = {
    hemp: { title: '麻绳', length: 750, bend: 12, wind: 24, damping: 1.4, radius: 4, start: [175, 190], end: [805, 190], sag: 170, pin: true },
    hose: { title: '软水管', length: 800, bend: 82, wind: 12, damping: 2.6, radius: 8, start: [140, 365], end: [820, 408], sag: -160, pin: true },
    kite: { title: '风筝线', length: 515, bend: 0, wind: 65, damping: 0.9, radius: 1.7, start: [210, 465], end: [590, 125], sag: 14, pin: false }
  };
  const clamp = (x, a, b) => Math.max(a, Math.min(b, x));
  class RopeSimulation {
    constructor(mode = 'hemp') { this.reset(mode); }
    reset(mode = this.mode) {
      if (!PRESETS[mode]) throw new Error('Unknown rope preset');
      this.mode = mode;
      this.settings = { ...PRESETS[mode], collision: true };
      this.length = this.settings.length;
      this.time = 0;
      this.accumulator = 0;
      this.droppedTime = 0;
      this.drag = null;
      this.gustTime = 0;
      this.points = [];
      const s = this.settings;
      this.anchors = [{ x: s.start[0], y: s.start[1] }, { x: s.end[0], y: s.end[1] }];
      // Sample the authored start curve uniformly by arc length, not parameter t.
      const samples = [], distances = [0];
      for (let i = 0; i <= 240; i++) {
        const t = i / 240;
        samples.push({ x: s.start[0] + (s.end[0] - s.start[0]) * t, y: s.start[1] + (s.end[1] - s.start[1]) * t + Math.sin(t * Math.PI) * s.sag });
        if (i) distances.push(distances[i - 1] + Math.hypot(samples[i].x - samples[i - 1].x, samples[i].y - samples[i - 1].y));
      }
      for (let i = 0; i <= 48; i++) {
        const d = distances[240] * i / 48;
        let j = 1;
        while (j < 240 && distances[j] < d) j++;
        const t = (d - distances[j - 1]) / (distances[j] - distances[j - 1]);
        const x = samples[j - 1].x * (1 - t) + samples[j].x * t;
        const y = samples[j - 1].y * (1 - t) + samples[j].y * t;
        this.points.push({ x, y, oldX: x, oldY: y, vx: 0, vy: 0, w: 1 });
      }
      this.stretchLambda = new Float64Array(48);
      this.bendLambda = new Float64Array(47);
      this.applyPins();
    }
    setPinned(value) {
      this.settings.pin = !!value;
      const p = this.points.at(-1);
      this.anchors[1] = { x: p.x, y: p.y };
      this.applyPins();
    }
    applyPins() {
      this.points.forEach((p, i) => { p.w = i === 0 || (i === 48 && this.settings.pin) ? 0 : (i === 48 && this.mode === 'kite' ? 0.08 : this.mode === 'kite' ? 10 : 1); });
      for (const [i, a] of [[0, this.anchors[0]], [48, this.anchors[1]]]) {
        if (this.points[i].w === 0) Object.assign(this.points[i], { x: a.x, y: a.y, vx: 0, vy: 0 });
      }
    }
    beginDrag(x, y) {
      let index = -1, nearest = 42;
      this.points.forEach((p, i) => {
        const d = Math.hypot(x - p.x, y - p.y);
        if (d < nearest) { nearest = d; index = i; }
      });
      if (index >= 0) this.drag = { index, x, y };
      return index;
    }
    moveDrag(x, y) {
      if (this.drag) { this.drag.x = clamp(x, 30, 970); this.drag.y = clamp(y, 55, GROUND - this.settings.radius); }
    }
    endDrag() { this.drag = null; }
    gust() { this.gustTime = 1.4; }
    wind() { return this.settings.wind / 100 + (this.gustTime > 0 ? Math.sin(Math.PI * this.gustTime / 1.4) * 0.8 : 0); }
    advance(seconds) {
      if (!Number.isFinite(seconds) || seconds <= 0) return;
      // Bounded catch-up: background tabs never create one giant physics step.
      const accepted = Math.min(seconds, 0.1);
      this.droppedTime += seconds - accepted;
      this.accumulator += accepted;
      while (this.accumulator + 1e-12 >= STEP) {
        this.step();
        this.accumulator = Math.max(0, this.accumulator - STEP);
      }
    }
    step() {
      const h = STEP, s = this.settings, ps = this.points;
      this.time += h;
      this.gustTime = Math.max(0, this.gustTime - h);
      // This is controlled rest-length adjustment, NOT topology-changing payout.
      const span = Math.hypot(this.anchors[1].x - this.anchors[0].x, this.anchors[1].y - this.anchors[0].y);
      const target = s.pin ? Math.max(s.length, span * 1.02) : s.length;
      this.length += clamp(target - this.length, -65 * h, 65 * h);
      if (this.drag && ps[this.drag.index].w === 0) {
        const which = this.drag.index === 0 ? 0 : 1;
        const a = this.anchors[which], other = this.anchors[1 - which];
        let x = this.drag.x, y = this.drag.y;
        if (s.pin) {
          const d = Math.hypot(x - other.x, y - other.y), max = this.length * 0.98;
          if (d > max) { x = other.x + (x - other.x) * max / d; y = other.y + (y - other.y) * max / d; }
        }
        const d = Math.hypot(x - a.x, y - a.y), speed = Math.min(1, 500 * h / Math.max(d, 1e-8));
        a.x += (x - a.x) * speed; a.y += (y - a.y) * speed;
      }
      this.applyPins();
      const wind = this.wind();
      for (let i = 0; i < ps.length; i++) {
        const p = ps[i];
        p.oldX = p.x; p.oldY = p.y;
        if (!p.w) continue;
        let ax = wind * (22 + 14 * Math.sin(this.time * 2.1 + i * 0.18));
        let ay = 180;
        if (this.mode === 'kite' && i === 48) {
          // Stylized wind-dependent lift/drag, not an aerodynamic model.
          ax += 800 * wind * wind;
          ay -= 1050 * wind * wind;
        }
        const decay = Math.exp(-s.damping * h);
        p.vx = p.vx * decay + ax * h; p.vy = p.vy * decay + ay * h;
        p.x += p.vx * h; p.y += p.vy * h;
      }
      this.stretchLambda.fill(0); this.bendLambda.fill(0);
      const dragLambda = [0, 0];
      const bendAlpha = Math.pow(10, -5 - 2 * s.bend / 100) / (h * h);
      for (let iteration = 0; iteration < 8; iteration++) {
        for (let k = 0; k < 48; k++) {
          const i = iteration % 2 ? 47 - k : k;
          const a = ps[i], b = ps[i + 1];
          const dx = b.x - a.x, dy = b.y - a.y, d = Math.hypot(dx, dy);
          const alpha = 1e-8 / (h * h);
          if (d < 1e-8 || a.w + b.w === 0) continue;
          const dl = (-(d - this.length / 48) - alpha * this.stretchLambda[i]) / (a.w + b.w + alpha);
          this.stretchLambda[i] += dl;
          a.x -= a.w * dx / d * dl; a.y -= a.w * dy / d * dl;
          b.x += b.w * dx / d * dl; b.y += b.w * dy / d * dl;
        }
        if (s.bend > 0) for (let i = 1; i < 48; i++) {
          const a = ps[i - 1], b = ps[i], c = ps[i + 1];
          const ux = b.x - a.x, uy = b.y - a.y, vx = c.x - b.x, vy = c.y - b.y;
          const u2 = ux * ux + uy * uy, v2 = vx * vx + vy * vy;
          if (u2 < 1e-8 || v2 < 1e-8) continue;
          const angle = Math.atan2(ux * vy - uy * vx, ux * vx + uy * vy);
          const ga = [-uy / u2, ux / u2], gc = [-vy / v2, vx / v2];
          const gb = [-ga[0] - gc[0], -ga[1] - gc[1]];
          const denom = a.w / u2 + c.w / v2 + b.w * (gb[0] ** 2 + gb[1] ** 2) + bendAlpha;
          const dl = (-angle - bendAlpha * this.bendLambda[i - 1]) / denom;
          this.bendLambda[i - 1] += dl;
          for (const [p, g] of [[a, ga], [b, gb], [c, gc]]) { p.x += p.w * g[0] * dl; p.y += p.w * g[1] * dl; }
        }
        if (this.drag && ps[this.drag.index].w) {
          const p = ps[this.drag.index], alpha = 2e-5 / (h * h);
          for (const [j, axis] of ['x', 'y'].entries()) {
            const dl = (-(p[axis] - this.drag[axis]) - alpha * dragLambda[j]) / (p.w + alpha);
            dragLambda[j] += dl; p[axis] += p.w * dl;
          }
        }
        if (s.collision) this.projectContacts();
      }
      for (const p of ps) {
        p.vx = p.w ? (p.x - p.oldX) / h : 0;
        p.vy = p.w ? (p.y - p.oldY) / h : 0;
        // Contact-only tangential damping: deliberately not full Coulomb friction.
        if (s.collision && p.y >= GROUND - s.radius - 0.1) p.vx *= Math.exp(-12 * h);
      }
    }
    projectContacts() {
      const ps = this.points, r = this.settings.radius;
      for (const p of ps) if (p.w) p.y = Math.min(p.y, GROUND - r);
      // Segment-circle contact avoids the spatial gaps of particle-only collision.
      for (let i = 0; i < 48; i++) {
        const a = ps[i], b = ps[i + 1], dx = b.x - a.x, dy = b.y - a.y;
        const t = clamp(((ROCK.x - a.x) * dx + (ROCK.y - a.y) * dy) / Math.max(dx * dx + dy * dy, 1e-8), 0, 1);
        const qx = a.x + t * dx - ROCK.x, qy = a.y + t * dy - ROCK.y;
        const d = Math.hypot(qx, qy), w = a.w * (1 - t) ** 2 + b.w * t * t;
        if (d < ROCK.radius + r && w > 1e-8) {
          const nx = d > 1e-8 ? qx / d : 0, ny = d > 1e-8 ? qy / d : -1;
          const dl = (ROCK.radius + r - d) / w;
          a.x += a.w * (1 - t) * nx * dl; a.y += a.w * (1 - t) * ny * dl;
          b.x += b.w * t * nx * dl; b.y += b.w * t * ny * dl;
        }
      }
    }
    diagnostics() {
      let error = 0;
      for (let i = 0; i < 48; i++) error = Math.max(error, Math.abs(Math.hypot(this.points[i + 1].x - this.points[i].x, this.points[i + 1].y - this.points[i].y) / (this.length / 48) - 1));
      return { mode: this.mode, points: this.points.length, time: this.time, maxStrain: error, length: this.length, finite: this.points.every(p => [p.x, p.y, p.vx, p.vy].every(Number.isFinite)), droppedTime: this.droppedTime };
    }
  }
  const api = { RopeSimulation, PRESETS, STEP, GROUND, ROCK };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.RopeLab = api;
})(globalThis);
