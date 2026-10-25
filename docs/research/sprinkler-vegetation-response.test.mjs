// Tests for the report's explanatory model, NOT the game's spray/terrain solver.
// Run: node --test docs/research/sprinkler-vegetation-response.test.mjs
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';

const report = new URL('./sprinkler-vegetation-response.html', import.meta.url);
const html = readFileSync(report, 'utf8');
const script = html.match(/<script id="response-model">([\s\S]*?)<\/script>/)?.[1];
assert.ok(script, 'report must contain its self-contained illustrative model');
const model = vm.runInNewContext(`${script}\nSprayResponseModel;`, {});
const modes = ['sweep', 'pulse', 'steady', 'opposed'];
const close = (a, b, tolerance = 1e-10) => assert.ok(Math.abs(a - b) <= tolerance, `${a} != ${b} within ${tolerance}`);

test('all scenarios start dry and still, remain finite, and keep wetness bounded', () => {
  for (const mode of modes) {
    for (const wetWeight of [false, true]) {
      const samples = model.simulate({ mode, wetWeight });
      close(samples[0].w, 0);
      close(samples.at(-1).t, 12);
      for (const state of samples) {
        assert.ok(Object.values(state).every(Number.isFinite));
        assert.ok(state.w >= 0 && state.w <= 1);
        assert.ok(Math.abs(state.grass) < 2 && Math.abs(state.apple) < 0.5);
        if (state.t <= 1) {
          close(state.w, 0); close(state.grass, 0); close(state.leaf, 0); close(state.apple, 0);
        }
      }
    }
  }
});

test('a passing stream pushes away, rebounds, settles, and retains wetness after shutoff', () => {
  const samples = model.simulate();
  assert.ok(model.at(samples, 1.2).grass > 0);
  assert.ok(samples.some(s => s.t > 2.2 && s.t < 6 && s.grass < -0.01));
  const end = samples.at(-1);
  assert.ok(Math.abs(end.grass) < 0.001 && Math.abs(end.gv) < 0.01);
  const wetAtStop = model.at(samples, 2.2).w;
  assert.ok(wetAtStop > 0.1);
  assert.ok(end.w > 0 && end.w < wetAtStop);
  close(model.input('sweep', 3, 4).dose, 0);
});

test('opposed streams cancel directional forcing, not wetting', () => {
  const opposed = model.simulate({ mode: 'opposed' });
  const steady = model.simulate({ mode: 'steady' });
  for (const s of opposed) { close(s.grass, 0); close(s.leaf, 0); close(s.apple, 0); }
  assert.ok(model.at(opposed, 3).w > model.at(steady, 3).w);
  close(model.input('opposed', 2, 3).dose, 2);
  close(model.input('opposed', 2, 3).push, 0);
});

test('wet-weight comparison changes grass/leaf response, not exposure or apple frequency', () => {
  const wet = model.simulate({ wetWeight: true });
  const dry = model.simulate({ wetWeight: false });
  let changed = false;
  for (let i = 0; i < wet.length; i++) {
    close(wet[i].w, dry[i].w);
    close(wet[i].apple, dry[i].apple);
    close(wet[i].av, dry[i].av);
    changed ||= Math.abs(wet[i].grass - dry[i].grass) > 0.05;
  }
  assert.ok(changed);
});

test('pulse and sweep dose integrate consistently at 30/60/120 Hz', () => {
  const expected = { sweep: 1.2, pulse: 2.1, steady: 7, opposed: 14 };
  for (const hz of [30, 60, 120]) {
    for (const mode of modes) {
      let dose = 0;
      for (let i = 0; i < 12 * hz; i++) {
        const a = i / hz, b = (i + 1) / hz;
        dose += model.input(mode, a, b).dose * (b - a);
      }
      close(dose, expected[mode], 1e-10);
    }
  }
});

test('constant-input analytical updates compose across time steps', () => {
  const direct = model.spring(0.2, -0.3, 2, 50, 1.4, 0.27, 0.2);
  const half = model.spring(0.2, -0.3, 2, 50, 1.4, 0.27, 0.1);
  const split = model.spring(half[0], half[1], 2, 50, 1.4, 0.27, 0.1);
  close(direct[0], split[0]); close(direct[1], split[1]);
  close(model.wetStep(0.2, 0.7, 0.2), model.wetStep(model.wetStep(0.2, 0.7, 0.1), 0.7, 0.1));
});

test('apple center follows a constant-length arc and time sampling clamps', () => {
  for (const angle of [-0.4, -0.1, 0, 0.1, 0.4]) {
    const p = model.applePosition(angle);
    close(Math.hypot(p.x - 735, p.y - 80), 105);
  }
  const samples = model.simulate();
  close(model.at(samples, -1).t, 0);
  close(model.at(samples, 50).t, 12);
  assert.throws(() => model.simulate({ dt: 0 }));
  assert.throws(() => model.simulate({ dt: NaN }));
  assert.throws(() => model.simulate({ mode: 'unknown' }));
});

test('report references resolve, IDs are unique, and presentation is self-contained', () => {
  const ids = Array.from(html.matchAll(/\bid="([^"]+)"/g), m => m[1]);
  assert.equal(new Set(ids).size, ids.length);
  for (const [, href] of html.matchAll(/\bhref="([^"]+)"/g)) {
    if (href.startsWith('#')) assert.ok(ids.includes(href.slice(1)), href);
    else if (!href.startsWith('https://')) assert.ok(existsSync(new URL(href, report)), href);
  }
  assert.doesNotMatch(html, /<(?:script|img|link)[^>]+(?:src|href)="https?:/i);
  assert.match(html, /不是游戏截图、生产模型或性能实测/);
  assert.match(html, /植物不互挡/);
  assert.match(html, /terrain 必须阻挡/);
});
