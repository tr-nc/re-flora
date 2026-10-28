import test from 'node:test';
import assert from 'node:assert/strict';
import { DEFAULTS, PRESETS, TINTS, NEUTRAL_RGB, normalizeSettings, activePreset, materialProperties, materialRecipe } from './material.mjs';

test('missing, malformed, and foreign saved state becomes safe defaults', () => {
  for (const input of [null, undefined, [], 'invalid', { tint: '__proto__', scene: 'rain', tinted: 'true', compare: 1, frost: Infinity, pixel: NaN, opacity: '90', unknown: 42 }]) {
    assert.deepEqual(normalizeSettings(input), DEFAULTS);
  }
});

test('numeric controls are bounded and quantized at the storage boundary', () => {
  const settings = normalizeSettings({ opacity: -10, frost: 500, grain: 10.6, pixel: 100 });
  assert.equal(settings.opacity, 10);
  assert.equal(settings.frost, 30);
  assert.equal(settings.grain, 11);
  assert.equal(settings.pixel, 5);
});

test('turning tint off changes only the body color, and preserves the chosen tint', () => {
  for (const tint of Object.keys(TINTS)) {
    const tinted = { ...DEFAULTS, tint };
    const neutral = { ...tinted, tinted: false };
    assert.deepEqual(materialProperties(neutral), { ...materialProperties(tinted), '--glass-rgb': NEUTRAL_RGB });
    assert.equal(normalizeSettings(neutral).tint, tint);
    assert.equal(materialProperties({ ...neutral, tinted: true })['--glass-rgb'], TINTS[tint].rgb);
  }
});

test('presets describe material properties independently from scene and tint', () => {
  for (const [key, preset] of Object.entries(PRESETS)) {
    const settings = normalizeSettings({ ...DEFAULTS, ...preset, scene: 'night', tinted: false });
    assert.equal(activePreset(settings), key);
    assert.equal(activePreset({ ...settings, frost: settings.frost + 1 }), null);
  }
});

test('zero frost and dither are real off states, independent of body opacity', () => {
  const properties = materialProperties({ ...DEFAULTS, frost: 0, grain: 0 });
  assert.equal(properties['--frost'], '0px');
  assert.equal(properties['--grain'], '0');
  assert.equal(properties['--body'], '0.66');
});

test('export is serializable, normalized and matches the rendered material', () => {
  const settings = { ...DEFAULTS, tinted: false, scene: 'dusk', compare: true, pixel: 3, frost: 21 };
  const recipe = JSON.parse(JSON.stringify(materialRecipe(settings)));
  assert.deepEqual(recipe.css, materialProperties(settings));
  assert.deepEqual(recipe.material.colorSrgb, [225, 225, 220]);
  assert.equal(recipe.material.opacity, .66);
  assert.equal(recipe.material.frostCssPixels, 21);
  assert.equal(recipe.material.pixelCssPixels, 3);
  assert.deepEqual(recipe.preview, { scene: 'dusk', compare: true });
  assert.equal(recipe.css['--grain-size'], '96px');
  assert.match(recipe.note, /not an engine config/);
});
