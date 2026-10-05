# Scene pixel sampling

Open **Debug → Scene Pixel Sampling** (or search its name). Controls are saved through normal **Save**, not game-startup flags. The contrast candidate is ready for visual comparison, not approved visual/performance acceptance.

## Controls

- **Final pixel resolution**: 1:1, 4:1, 16:1, 64:1 (original/default).
- **Higher-resolution pixelization (A/B)**: unchecked is original direct low-resolution rendering; checked renders a denser source for the selected color resolve.
- **Source pixel density**: four-to-one (2×2 source pixels) or sixteen-to-one (4×4 source pixels). Independent of displayed block size.
- **Pixel color resolve**: HDR average (previous reference), or Contrast-aware (experimental). Only one artistic candidate is implemented.

At 2560×1440:

| Final resolution | Physical pixels per scene pixel | Final scene grid |
| --- | --- | --- |
| 1:1 | 1×1 | 2560×1440 |
| 4:1 | 2×2 | 1280×720 |
| 16:1 | 4×4 | 640×360 |
| 64:1 | 8×8 | 320×180 |

The grid follows the actual game window's physical resolution (screen resolution when fullscreen). Dimensions round up; only partial edge blocks are cropped, never stretched. UI remains native.

Source density is capped at native-equivalent resolution, with edge padding: 1:1 bypasses denser rendering/contrast resolve; 4:1 uses at most four source pixels; 16:1 and 64:1 support both densities. Requested controls remain saved when bypassed or disabled.

## Candidate and reference

The contrast candidate borrows the luminance rule from [PixelOE](https://github.com/KohakuBlueleaf/PixelOE/blob/7ce444b36d3876a151d845d4493240e904454d89/src/pixeloe/slang/shaders/downscale/contrast.slang), not its chroma reconstruction. It tone-maps each source color, computes perceptual D65 L* statistics (median/mean/min/max), then selects **one complete, untouched source RGB tuple**. Min/max ties use the first matching sample; fallback uses the row-major middle sample. It does not reconstruct or clamp a new color. Standard display dither and storage/presentation quantization still follow. No palette reduction, clustering or outline expansion is included.

The existing reference still averages **linear HDR before tone mapping**. This operation order is preserved for old saves. The candidate also changes tone-map order, so this compares complete resolves, not an isolated statistical rule with identical color-space inputs.

Contrast can make blocks harder, select a different existing source color, flicker, or remove minority thin details; it does not promise detail preservation. Observe wind, camera motion, grass/branches, sky boundaries and highlights. If useful, outline protection is a separate next experiment, not silently included here.

For aligned full 8×8 compute groups, the contrast shader resolves each coarse cell once into shared memory. All leaders write before a group barrier. Partial edge groups avoid that barrier entirely (some invocations are outside the window) and resolve independently. No extra coarse texture is needed. This is not a measured performance claim.

Changing only color resolve does not rebuild attachments or resize the swapchain. Source/grid changes drain submitted frames, replace scene resources and invalidate histories; they may briefly pause.

## Cyan fringe regression

The initial candidate independently selected L*, a* and b*. At foliage/sky boundaries it combined foliage's green-axis chroma with sky's blue-axis chroma, synthesizing cyan not present in either source. The four-source-color CPU shader regression returned `16` (cyan detected) before the fix. Selecting complete source tuples makes it pass and prevents the broader class of channel-splicing artifacts, not merely one cyan hue. Lab-to-RGB reconstruction is no longer part of this resolve. Whole-frame screenshots remain useful to identify any already-present lighting/outline colors; source membership alone does not claim all artistic issues are solved.

The 5120×2880 Release captures under `target/scene-supersampling/cyan-fix/` use the same camera, 64:1 final grid and four-to-one source density. In the crown ROI `(2100,200)–(4100,1600)`, the fixed sRGB cyan-like predicate (`r < 0.65g`, `r < 0.65b`, `g,b > 0.35`) counted **83,520 before / 0 after**. The old fringe is visibly absent in `before-after.png`. Animation/capture times are not frame-identical, and this check is neither a performance benchmark nor final artistic approval. CPU regressions, all 35 Slang CPU tests, 1339 app + 4 library Rust tests (3 ignored), and native Vulkan synchronization/lifecycle validation passed.

## Compatibility and validation

Older saves receive the HDR-average resolve default; old checkbox/density values survive, with legacy AA labels updated from the declarative schema. Internal legacy IDs remain compatible. New controls use generated fields, shared search and unified persistence.

- Rust tests cover presets, capping, no resource change for resolve-only switches, migration, saved disabled preferences and searchable ownership.
- `shader/tests/scene_pixel_style_test.slang` checks exact source-color membership, the green-foliage/blue-sky cyan regression, 4/16-pixel coverage patterns, actual per-source HDR tone mapping, deterministic mixed-color sweeps, preserved legitimate blue colors, uniform blocks, contrast choices and minority-detail limitations.
- `shader/tests/scene_pixel_filter_test.slang` checks block mapping and sampling bounds.
- After `cargo build --release`, `node scripts/validate-scene-supersampling.mjs` runs Vulkan synchronization validation over both resolves/densities, all ratios, odd/tiny windows and resolve-only toggles. Scene-depth outlines were removed with the `spl` integration and are no longer part of this fixture. Saved-file hashes must remain unchanged. Artifacts: `target/scene-supersampling/`.

The internal `RE_FLORA_SCENE_PIXEL_TRYOUT` review preset starts the real controls at 64:1, four-to-one source density and contrast resolve, without writing the config. It is only for preparing a live review; normal use is through the panel. See [research](research/high-resolution-pixelization.md) for alternatives and evidence limits.
