# Pixel matte glass study

A dependency-free browser playground for Re: Flora's UI material. This is a visual
reference, not a native UI implementation, shader, or game config change.

## Open

From the repository root:

```sh
python3 -m http.server 4173 --bind 127.0.0.1
```

Visit <http://127.0.0.1:4173/experiments/matte-glass/>. Serve the repository root,
not only this directory: the prototype references the existing garden screenshot
and font by relative URL. No package install, external CDN, or build is needed.
ES modules require HTTP rather than opening `index.html` as a file.

## Try

- Start with **Soft frost**. **Chalk** is more opaque; **Thin glass** reveals more
  background detail.
- Turn **Tint the glass** off for neutral gray glass, or choose Fern, Honey, Rose,
  or Mist. Turning tint back on restores the previous color.
- **Body** adjusts opacity; **Frost** blurs the backdrop; **Dither** controls static
  monochrome pixel grain; **Pixel size** changes the grain scale and stepped corners.
  Zero frost/grain turns that component off. Frost does not pixelate the scene.
- **Compare with untinted** keeps the small right-hand specimen neutral while
  everything else uses the chosen color. Opacity, blur, grain, and edge scale are
  identical. The samples occupy different backdrop regions, so this is an
  in-context comparison, not a controlled same-pixel A/B capture.
- **Daylight / Dusk / Night** apply color grading to the screenshot only, as rough
  contrast checks. These are not captures of the game's lighting states. Daylight
  is deliberately softened too; see the CSS for the exact filters.
- **Try your screenshot** accepts a local PNG, JPEG, or WebP (up to 20 MB and
  40 megapixels). It stays in browser memory: no upload, storage, or network request.
  **Use garden backdrop** restores the repository image. Reload also restores it.
- Select seeds and plant them to exercise selection, action, disabled, and toast
  states. This is a UI mock: planting updates the sample inventory, not the garden
  image. The toolbar changes selection only; keys 1–4 also select tools when not
  interacting with a control.
- **Export material** opens a JSON recipe with copy and download actions. Its
  properties match the current rendered surface. CSS blur units are not calibrated
  to engine shader units; the recipe is not accepted by the game.

Material and scene controls persist under the browser-local key
`re-flora.matte-glass.v1`. **Reset** restores those controls to their defaults;
it does not reset the mock inventory or a custom screenshot. Corrupt stored data
falls back to defaults; unavailable browser storage does not prevent editing.
Low opacity and extreme settings intentionally permit poor contrast for material
exploration; this is not a guarantee of accessible text on every backdrop.

## Implementation

- `index.html`: semantic controls and representative game UI.
- `style.css`: the shared `.glass` material, pixel sprites, and responsive layout.
- `material.mjs`: authoritative defaults, presets, safe settings normalization,
  CSS properties, and JSON export.
- `app.mjs`: live controls, browser persistence, deterministic grain generation,
  local backdrop handling, and mock UI interactions.
- `material.test.mjs`: pure behavior tests with Node's built-in test runner.

Every surface uses the same material. A clipped `::before` layer owns the flat
tint, backdrop blur/desaturation, and subtle rim; `::after` adds static pixel grain.
Neither layer blurs the text or receives pointer events. Neutral mode replaces
only the tint RGB; it does not change opacity, frost, or grain. The comparison
specimen overrides only that same RGB property. Pixel corners are two-step cuts,
not rounded glossy bevels.

Use a current browser with `backdrop-filter` support. A notice appears when blur
is unavailable; tint and grain still work. Native engine integration and runtime
performance measurements are deliberately outside this prototype's scope.

## Validation

```sh
node --check experiments/matte-glass/app.mjs
node --test experiments/matte-glass/material.test.mjs
```

Browser smoke checks: toggle tint off/on, select each preset, set frost and grain
to zero, adjust pixel scale, compare neutral glass, change lighting, select/plant
seeds, change tools, export, reload to verify saved settings, reset, and load/remove
a local screenshot. Check desktop and narrow layouts for overlapping panels and
horizontal overflow. Chrome automation was used during initial implementation;
Safari and Firefox have not been separately verified.

## Visual resources and provenance

No image, audio, or font files were imported, copied, or modified for this study.
There are no image-generation service outputs or externally fetched resources.

- **Garden backdrop:** references the original project capture
  [`../../demo/img/splash.png`](../../demo/img/splash.png), unchanged. Its creator,
  checksum, known usage terms, and unavailable capture details are documented in
  [the source bundle note](../../demo/img/README.md). CSS uses centered `cover`
  cropping, pixelated resampling, and lighting filters; no derivative file is saved.
- **Pixel font:** references the existing
  [`PixelifySans-VariableFont_wght.ttf`](../../assets/font/PixelifySans-VariableFont_wght.ttf),
  unchanged. See [its provenance](../../assets/font/README.md) and
  [SIL OFL license](../../assets/font/OFL.txt). The inspector uses system fonts.
- **Seed sprites:** original procedural CSS drawings authored for this prototype
  in `style.css`, using rectangles and box shadows. No external reference artwork,
  model, generation service, image file, or conversion step was used.
- **Surface grain:** runtime-generated by `makeGrain()` in `app.mjs`. This is an
  original deterministic code texture, not a generative-model asset. A 32×32 RGBA
  canvas uses a 32-bit LCG seeded with `1837`, multiplier `1664525`, increment
  `1013904223`. Each pixel is RGB 90 or 220 according to the high-byte threshold
  128, with alpha 140. It is encoded as an in-memory PNG data URL, tiled, scaled
  with nearest-neighbor pixel rendering, and composited at the chosen dither
  opacity. The export identifies it as `seeded-monochrome-32x32-v1`. No persistent
  texture file is produced.
- **Symbols:** Unicode glyphs rendered with the bundled font or system fallbacks;
  their exact appearance can vary by platform.

If copying this experiment elsewhere, also include the referenced screenshot and
font bundles with their README/license files and preserve or update the relative
paths. Existing source-bundle rights apply; this note does not relicense them.
