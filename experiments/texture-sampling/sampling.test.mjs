import test from 'node:test';
import assert from 'node:assert/strict';
import { neighbors, nearest, bilinear, bicubic, area, makeTexture, srgbEncode, makeMipmaps, mipSelection, sampleMip, trilinear, lodForScale } from './sampling.mjs';
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
test('mip chain dimensions and linear averages are correct', () => {
  const levels = makeMipmaps(makeTexture('checker'));
  assert.deepEqual(levels.map(t => t.width), [16, 8, 4, 2, 1]);
  for (const level of levels.slice(1)) for (const c of level.data) close(c, [0.4675, 0.4675, 0.4675]);
  close(makeMipmaps(t)[1].data[0], [0.5, 0.5, 0.5]);
  const row = { width: 4, height: 1, data: [[0, 0, 0], [0, 0, 0], [1, 1, 1], [1, 1, 1]] };
  assert.deepEqual(makeMipmaps(row).map(t => [t.width, t.height]), [[4, 1], [2, 1], [1, 1]]);
  assert.throws(() => makeMipmaps({ width: 3, height: 1, data: [] }), /power-of-two/);
});
test('mip sampling preserves UV centers and clamps edges', () => {
  const levels = makeMipmaps(makeTexture('colors'));
  const level = levels[1];
  // Base x=4.5/y=8.5 maps to mip x=2/y=4, not x=2.25/y=4.25.
  close(sampleMip(levels, 1, 4.5, 8.5), level.data[4 * level.width + 2]);
  close(sampleMip(levels, 1, -100, -100), level.data[0]);
});
test('trilinear endpoints, fractional blend, continuity and LOD bounds', () => {
  const levels = makeMipmaps(makeTexture('stem')), x = 7.2, y = 6.8;
  close(trilinear(levels, x, y, 0), bilinear(levels[0], x, y));
  close(trilinear(levels, x, y, -3), bilinear(levels[0], x, y));
  for (let level = 0; level < levels.length; level++) close(trilinear(levels, x, y, level), sampleMip(levels, level, x, y));
  const a = sampleMip(levels, 1, x, y), b = sampleMip(levels, 2, x, y);
  close(trilinear(levels, x, y, 1.25), a.map((v, k) => v * 0.75 + b[k] * 0.25));
  close(trilinear(levels, x, y, 100), levels.at(-1).data[0]);
  const left = trilinear(levels, x, y, 2 - 1e-8), right = trilinear(levels, x, y, 2 + 1e-8);
  assert.ok(left.every((v, k) => Math.abs(v - right[k]) < 1e-7));
  assert.deepEqual(mipSelection(levels, 100), { lod: 4, lower: 4, upper: 4, blend: 0 });
  assert.equal(lodForScale(4), 0); assert.equal(lodForScale(0.5), 1); assert.equal(lodForScale(0.25), 2);
});
test('display conversion clamps overshoot; patterns are valid linear RGB', () => {
  assert.equal(srgbEncode(-0.1), 0); assert.ok(Math.abs(srgbEncode(1.1) - 255) < 1e-10);
  for (const kind of ['colors', 'stem', 'checker']) {
    const texture = makeTexture(kind); assert.equal(texture.data.length, 256);
    assert.ok(texture.data.every(c => c.length === 3 && c.every(v => v >= 0 && v <= 1)));
  }
});
