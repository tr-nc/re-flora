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

The contrast candidate is inspired by [PixelOE](https://github.com/KohakuBlueleaf/PixelOE/blob/7ce444b36d3876a151d845d4493240e904454d89/src/pixeloe/slang/shaders/downscale/contrast.slang). It tone-maps each source color, obtains D65 Lab statistics, chooses luminance using local median/mean/min/max, takes lower-median chroma, and converts back to display-linear RGB. Fallback luminance is the row-major middle sample, matching the referenced rule. Display-gamut output is clamped. No palette reduction, clustering or outline expansion is included.

The existing reference still averages **linear HDR before tone mapping**. This operation order is preserved for old saves. The candidate also changes tone-map order, so this compares complete resolves, not an isolated statistical rule with identical color-space inputs.

Contrast can make blocks harder, but can change hues, flicker, or remove minority thin details; it does not promise detail preservation. Observe wind, camera motion, grass/branches, sky boundaries and highlights. If useful, outline protection is a separate next experiment, not silently included here.

For aligned full 8×8 compute groups, the contrast shader resolves each coarse cell once into shared memory. All leaders write before a group barrier. Partial edge groups avoid that barrier entirely (some invocations are outside the window) and resolve independently. No extra coarse texture is needed. This is not a measured performance claim.

Changing only color resolve does not rebuild attachments or resize the swapchain. Source/grid changes drain submitted frames, replace scene resources and invalidate histories; they may briefly pause.

## Compatibility and validation

Older saves receive the HDR-average resolve default; old checkbox/density values survive, with legacy AA labels updated from the declarative schema. Internal legacy IDs remain compatible. New controls use generated fields, shared search and unified persistence.

- Rust tests cover presets, capping, no resource change for resolve-only switches, migration, saved disabled preferences and searchable ownership.
- `shader/tests/scene_pixel_style_test.slang` checks Lab round trips, uniform colors, contrast choices, minority-detail limitations and finite/gamut-bounded 4/16-pixel results.
- `shader/tests/scene_pixel_filter_test.slang` checks block mapping and sampling bounds.
- After `cargo build --release`, `node scripts/validate-scene-supersampling.mjs` runs Vulkan synchronization validation over both resolves/densities, all ratios, odd/tiny windows, depth outlines and resolve-only toggles. Saved-file hashes must remain unchanged. Artifacts: `target/scene-supersampling/`.

The internal `RE_FLORA_SCENE_PIXEL_TRYOUT` review preset starts the real controls at 64:1, four-to-one source density and contrast resolve, without writing the config. It is only for preparing a live review; normal use is through the panel. See [research](research/high-resolution-pixelization.md) for alternatives and evidence limits.
