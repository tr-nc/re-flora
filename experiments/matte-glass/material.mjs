export const TINTS = Object.freeze({
  fern: { name: 'Fern', rgb: '217 228 194' },
  honey: { name: 'Honey', rgb: '240 222 185' },
  rose: { name: 'Rose', rgb: '236 210 210' },
  mist: { name: 'Mist', rgb: '208 226 234' },
});
export const NEUTRAL_RGB = '225 225 220';
export const PRESETS = Object.freeze({
  soft: { name: 'Soft frost', opacity: 66, frost: 16, grain: 12, pixel: 2 },
  chalk: { name: 'Chalk', opacity: 85, frost: 24, grain: 18, pixel: 2 },
  clear: { name: 'Thin glass', opacity: 28, frost: 5, grain: 6, pixel: 2 },
});
export const RANGES = Object.freeze({ opacity: [10, 95], frost: [0, 30], grain: [0, 30], pixel: [1, 5] });
export const DEFAULTS = Object.freeze({ tinted: true, tint: 'fern', opacity: 66, frost: 16, grain: 12, pixel: 2, scene: 'day', compare: false });

// Storage is an untrusted boundary: accept known keys, finite numbers and valid enums only.
export function normalizeSettings(value) {
  const input = value && typeof value === 'object' ? value : {};
  const result = { ...DEFAULTS };
  for (const [key, [min, max]] of Object.entries(RANGES)) {
    if (typeof input[key] === 'number' && Number.isFinite(input[key])) {
      result[key] = Math.round(Math.min(max, Math.max(min, input[key])));
    }
  }
  for (const key of ['tinted', 'compare']) {
    if (typeof input[key] === 'boolean') result[key] = input[key];
  }
  if (typeof input.tint === 'string' && Object.hasOwn(TINTS, input.tint)) result.tint = input.tint;
  if (['day', 'dusk', 'night'].includes(input.scene)) result.scene = input.scene;
  return result;
}

export function activePreset(settings) {
  return Object.keys(PRESETS).find(key => Object.keys(RANGES).every(field => settings[field] === PRESETS[key][field])) ?? null;
}

export function materialProperties(value) {
  const settings = normalizeSettings(value);
  return {
    '--glass-rgb': settings.tinted ? TINTS[settings.tint].rgb : NEUTRAL_RGB,
    '--body': String(settings.opacity / 100),
    '--frost': `${settings.frost}px`,
    '--grain': String(settings.grain / 100),
    '--pixel': `${settings.pixel}px`,
    '--grain-size': `${32 * settings.pixel}px`,
  };
}

export function materialRecipe(value) {
  const settings = normalizeSettings(value);
  return {
    schema: 're-flora/matte-glass-study/v1',
    material: {
      tinted: settings.tinted,
      tint: settings.tint,
      colorSrgb: materialProperties(settings)['--glass-rgb'].split(' ').map(Number),
      opacity: settings.opacity / 100,
      frostCssPixels: settings.frost,
      backdropSaturation: 0.5,
      ditherOpacity: settings.grain / 100,
      pixelCssPixels: settings.pixel,
      texture: 'seeded-monochrome-32x32-v1',
    },
    preview: { scene: settings.scene, compare: settings.compare },
    css: materialProperties(settings),
    note: 'Browser visual reference, not an engine config. CSS blur pixels are not calibrated to a game shader.',
  };
}
