import test from 'node:test';
import assert from 'node:assert/strict';
import { neighbors, nearest, bilinear, bicubic, area, makeTexture, srgbEncode } from './sampling.mjs';
const close = (a, b) => a.forEach((v, i) => assert.ok(Math.abs(v - b[i]) < 1e-10, `${a} != ${b}`));
const t = { width: 2, height: 2, data: [[1, 0, 0], [0, 1, 0], [0, 0, 1], [1, 1, 1]] };
test('bilinear weights sum to one and match the four-way midpoint', () => {
  for (const [x, y] of [[0, 0], [0.25, 0.75], [-0.3, 3.2], [15, 15]]) {
    assert.ok(Math.abs(neighbors(x, y).reduce((s, p) => s + p.weight, 0) - 1) < 1e-12);
  }
  close(bilinear(t, 0.5, 0.5), [0.5, 0.5, 0.5]);
  close(bilinear(t, 0.25, 0.75), [0.375, 0.25, 0.75]);
});
test('all point filters reproduce texel centers and clamp exterior coordinates', () => {
  for (const filter of [nearest, bilinear, bicubic]) {
    close(filter(t, 0, 0), t.data[0]); close(filter(t, 1, 1), t.data[3]);
    close(filter(t, -5, -5), t.data[0]); close(filter(t, 5, 5), t.data[3]);
  }
  close(nearest(t, 0.49, 0), t.data[0]); close(nearest(t, 0.5, 0), t.data[1]);
});
test('Catmull–Rom preserves constants and exposes negative-lobe overshoot', () => {
  const constant = { width: 2, height: 2, data: Array(4).fill([0.2, 0.4, 0.6]) };
  close(bicubic(constant, 0.35, 0.82), [0.2, 0.4, 0.6]);
  const edge = { width: 4, height: 1, data: [[0, 0, 0], [0, 0, 0], [1, 1, 1], [1, 1, 1]] };
  assert.ok(bicubic(edge, 0.5, 0)[0] < 0);
  assert.ok(bicubic(edge, 2.5, 0)[0] > 1);
});
test('area averages a full 2×2 footprint and small footprints preserve a texel', () => {
  close(area(t, 0.5, 0.5, [2, 0], [0, 2]), [0.5, 0.5, 0.5]);
  close(area(t, 0, 0, [0.2, 0], [0, 0.2]), [1, 0, 0]);
  close(area(t, 0.5, 0.5, [0, 2], [-2, 0]), [0.5, 0.5, 0.5]);
});
test('display conversion clamps overshoot; patterns are valid linear RGB', () => {
  assert.equal(srgbEncode(-0.1), 0); assert.ok(Math.abs(srgbEncode(1.1) - 255) < 1e-10);
  for (const kind of ['colors', 'stem', 'checker']) {
    const texture = makeTexture(kind); assert.equal(texture.data.length, 256);
    assert.ok(texture.data.every(c => c.length === 3 && c.every(v => v >= 0 && v <= 1)));
  }
});
