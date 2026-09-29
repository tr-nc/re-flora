// Coordinates use texel centers at integer x/y; addressing is clamp-to-edge.
export const clamp = (x, lo, hi) => Math.max(lo, Math.min(hi, x));
export function texel(texture, x, y) {
  return texture.data[clamp(y, 0, texture.height - 1) * texture.width + clamp(x, 0, texture.width - 1)];
}
export function neighbors(x, y) {
  const ix = Math.floor(x), iy = Math.floor(y), fx = x - ix, fy = y - iy;
  return [
    { x: ix, y: iy, weight: (1 - fx) * (1 - fy) },
    { x: ix + 1, y: iy, weight: fx * (1 - fy) },
    { x: ix, y: iy + 1, weight: (1 - fx) * fy },
    { x: ix + 1, y: iy + 1, weight: fx * fy },
  ];
}
export function nearest(t, x, y) { return texel(t, Math.floor(x + 0.5), Math.floor(y + 0.5)); }
export function bilinear(t, x, y) {
  const out = [0, 0, 0];
  for (const p of neighbors(x, y)) {
    const c = texel(t, p.x, p.y);
    for (let k = 0; k < 3; k++) out[k] += c[k] * p.weight;
  }
  return out;
}
// Demo textures have power-of-two dimensions. Generate each mip in linear RGB.
export function makeMipmaps(base) {
  const powerOfTwo = n => Number.isInteger(n) && n > 0 && Number.isInteger(Math.log2(n));
  if (!powerOfTwo(base.width) || !powerOfTwo(base.height)) throw new Error('Mip demo requires power-of-two dimensions');
  const levels = [base];
  while (levels.at(-1).width > 1 || levels.at(-1).height > 1) {
    const previous = levels.at(-1);
    const width = Math.max(1, previous.width / 2), height = Math.max(1, previous.height / 2), data = [];
    for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
      const color = [0, 0, 0];
      for (let j = 0; j < 2; j++) for (let i = 0; i < 2; i++) {
        const c = texel(previous, x * 2 + i, y * 2 + j);
        for (let k = 0; k < 3; k++) color[k] += c[k] / 4;
      }
      data.push(color);
    }
    levels.push({ width, height, data });
  }
  return levels;
}
export function mipSelection(levels, lod) {
  const value = clamp(lod, 0, levels.length - 1), lower = Math.floor(value);
  return { lod: value, lower, upper: Math.min(lower + 1, levels.length - 1), blend: value - lower };
}
// The same normalized UV must be sampled at both mip levels, not the same texel index.
export function sampleMip(levels, level, x, y) {
  const base = levels[0], t = levels[level];
  return bilinear(t, (x + 0.5) * t.width / base.width - 0.5, (y + 0.5) * t.height / base.height - 0.5);
}
export function trilinear(levels, x, y, lod) {
  const { lower, upper, blend } = mipSelection(levels, lod);
  const a = sampleMip(levels, lower, x, y), b = sampleMip(levels, upper, x, y);
  return a.map((v, k) => v * (1 - blend) + b[k] * blend);
}
// For this demo's uniform scale + rotation, footprint length is 1 / scale.
export function lodForScale(scale) { return Math.max(0, Math.log2(1 / scale)); }
export function cubicWeight(x) {
  const a = Math.abs(x);
  return a < 1 ? 1.5 * a ** 3 - 2.5 * a ** 2 + 1 : a < 2 ? -0.5 * a ** 3 + 2.5 * a ** 2 - 4 * a + 2 : 0;
}
export function bicubic(t, x, y) {
  const out = [0, 0, 0], ix = Math.floor(x), iy = Math.floor(y);
  for (let j = -1; j <= 2; j++) for (let i = -1; i <= 2; i++) {
    const w = cubicWeight(x - ix - i) * cubicWeight(y - iy - j);
    const c = texel(t, ix + i, iy + j);
    for (let k = 0; k < 3; k++) out[k] += c[k] * w;
  }
  return out; // Keep overshoot until display conversion.
}
// Approximate a box footprint with 8×8 stratified midpoint samples.
// This is spatial supersampling of piecewise-constant texels, not a mipmap.
export function area(t, x, y, dx, dy) {
  const out = [0, 0, 0];
  for (let j = 0; j < 8; j++) for (let i = 0; i < 8; i++) {
    const a = (i + 0.5) / 8 - 0.5, b = (j + 0.5) / 8 - 0.5;
    const c = nearest(t, x + a * dx[0] + b * dy[0], y + a * dx[1] + b * dy[1]);
    for (let k = 0; k < 3; k++) out[k] += c[k] / 64;
  }
  return out;
}
export function srgbEncode(value) {
  const v = clamp(value, 0, 1);
  return 255 * (v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055);
}
export function makeTexture(kind) {
  const width = 16, height = 16, data = [];
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    let c;
    if (kind === 'checker') c = (x + y) % 2 ? [0.92, 0.92, 0.92] : [0.015, 0.015, 0.015];
    else if (kind === 'colors') c = x < 8 ? (y < 8 ? [0.9, 0.08, 0.035] : [0.025, 0.12, 0.9]) : (y < 8 ? [0.07, 0.8, 0.15] : [0.95, 0.7, 0.06]);
    else {
      const stem = (x === 8 && y >= 5) || (y >= 4 && y <= 9 && x === y - 1) || (y >= 3 && y <= 8 && x === 16 - y);
      c = stem ? [0.28, 0.68, 0.08] : [0.012, 0.022, 0.035];
    }
    data.push(c);
  }
  return { width, height, data };
}
