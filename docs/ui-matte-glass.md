# Pixel matte UI glass

The native material adopts the approved `re-flora/matte-glass-study/v1` browser
recipe from `experiments/matte-glass/`:

| Property | Approved value | Native interpretation |
| --- | --- | --- |
| Tint | Fern, sRGB `(217, 228, 194)` | Same encoded RGB |
| Body opacity | `0.18` | Source-over tint after frost/desaturation |
| Frost | `7` CSS px | Gaussian reference sigma of 7 egui logical points |
| Backdrop saturation | `0.5` | CSS saturation coefficients |
| Dither opacity | `0.09` | Same grain alpha of `140/255`, then 9% layer opacity |
| Pixel size | `1` CSS px | One logical point; two-step corner cuts |
| Texture | `seeded-monochrome-32x32-v1` | Exact 32×32 LCG sequence, evaluated procedurally |

This remains the default (unchecked) native surface. The approved material
color constants live in `shader/slang/ui_matte_material.slang`; the spatial frost
recipe and dispatch footprint live together in `matte_blur/region.rs`. The optional pixelated
A/B mode below is declaratively saved; no per-setting save hooks are used.

## Scope

Glass is explicit on the toolbar, wallet, garden shop, planting catalog, material
backpack, Debug Panel, soil inspector, FPS/VRAM telemetry, Frame Timing, the texture
lifecycle diagnostic window, and construction/carry/tree-kick hints. Controls use
complementary pale inset fills and dark text. Selection/hover/pressed fills were
updated together to avoid dark text on the old dark backgrounds. Existing panel
outline preferences, layout, input, and save behavior remain intact.

Ordinary egui text, textures, tooltips, and the startup splash keep the regular
renderer; they are not detected or recolored by the GPU.
The material is not applied to world glass/voxel materials.

## Ownership and render contract

- `src/egui_renderer/matte_glass.rs` owns explicit frame/window adapters and the
  reserved material `TextureId::User`. Non-overlapping strips make pixel corners.
  Ordinary frames reserve a background slot before painting content. egui windows
  do not expose that slot, so the window adapter replaces only its own outer-frame
  shape within its explicit layer/range, preserving the actual frame's faded alpha,
  clipping, outline, and paint order. No global color-key or texture-atlas heuristic
  selects glass surfaces. Each surface includes a typed backdrop-boundary callback
  immediately before its material mesh. This is a compositor marker, not an
  external render callback; it prevents egui from batching adjacent glass surfaces
  into a single backdrop evaluation.
- `composition.rs` owns a linear RGBA16F accumulation target and a load/store UI
  render pass. Each frame starts with a copy of the current tracer scene. The scene
  itself is never modified by UI, so tracer history and scene-only consumers remain
  independent. Framebuffer/image generations retire through the completion sink.
- `matte_pipeline.rs` owns the material pipeline and backdrop descriptor.
  `matte_blur.rs` owns two full-resolution frost images and the horizontal/vertical
  compute passes. Descriptors and images retain their source generation; replaced
  bindings, images, and pipelines use the completion-scoped retirement sink.
- At each glass boundary, the renderer finishes the previous load/store pass,
  blurs the **current accumulated scene + earlier UI**, then starts the next pass
  to draw this glass and its contents. Blur reads the accumulation target only
  outside its attachment write pass and writes separate horizontal/vertical images.
  The material samples the completed blur, never its active color attachment.
  Resource tracking covers transfer, color-attachment, compute and fragment uses,
  including repeated use in one frame and next-frame reuse. There is no temporal
  UI feedback. The final composite is blitted to the swapchain; its existing
  load/store/present-layout transition and screenshot path are preserved.
- Loading/splash retains the ordinary swapchain UI path without a scene dependency.
  The composition pipelines use a fixed format independent of swapchain recreation.
  The `egui.render` GPU scope includes composition, all blur passes, and final blit,
  rather than hiding preparation cost outside the UI scope.
- Material meshes and ordinary egui meshes preserve painter order and restore the
  appropriate pipeline, projection, and descriptor when switching between them.
- Glass filters everything already painted below it, including lower panel tint,
  grain, text and controls. Its **own** text/controls are drawn afterward and stay
  crisp unless another glass panel covers them. Overlap is recursive: each layer
  contributes its material and another blur. Reordering, visibility fades and clip
  rectangles follow egui painter order; no panel count limit or topmost-only shortcut
  selects the backdrop.
- The resolved glass surface has opaque coverage except for normal egui visibility
  fades. The **18% body** is applied inside the shader; setting output alpha to 18%
  would incorrectly mix the unblurred scene back in a second time.
- Tint, desaturation, and grain composition use encoded sRGB like the browser
  reference. The resolved color is returned to linear space for Vulkan blending.
  UI scale/DPI scales blur and grain together with the geometry.

## Fidelity and cost

The filter is a normalized, separable Gaussian with sigma 7 logical points,
truncated at ±ceil(3 sigma) physical pixels. Both passes sample **every physical
texel**; DPI changes the weights and radius, never the sample spacing. Scene
colors are converted to encoded sRGB before the horizontal filter; both float
intermediates retain that encoding until material composition. Edge coordinates
clamp to the viewport. Each glass boundary refreshes the blur from the preceding
composite; its material fragments need one backdrop sample each. This is not a
claim of browser pixel-identical filtering or ray-traced volumetric glass.

This replaces the original sparse 7×7 kernel (introduced in `7631929b`,
2026-10-28). At 1× scale its samples were seven pixels apart: a single bright line
produced values `0.3991, 0, 0, 0, 0, 0, 0, 0.2420, ...`, i.e. visible displaced
copies rather than a smooth lobe. The dense filter removes that undersampling
mechanism instead of hiding it with a more opaque tint or temporal noise.

The accumulation target plus two reusable full-resolution RGBA16F blur images
cost 24 bytes per viewport pixel combined (about 47.5 MiB at 1920×1080), independent
of panel count, excluding in-flight retired generations. Each surface now filters
only its actual clipped glass footprint, with the full dependency halo/cell support
(see the performance fix below). Work still grows with the total covered area and
number of overlapping surfaces; full-screen stacked glass remains expensive.
Allocations are unchanged. No asynchronous captures or stale-frame readbacks are
involved. The measured improvement below applies to the tested ordinary panels,
not a universal performance-budget acceptance.

At 18% body opacity, backdrop brightness still strongly affects contrast, as it
would in the approved web preset. Selected and hovered inset controls have a
contrast regression test; arbitrary scene-backed text is not claimed to meet a
contrast threshold on every possible night scene.

No external image/audio resources were added. The procedural grain uses the
browser's documented seed `1837`, multiplier `1664525`, increment `1013904223`,
RGB levels `90/220`, and alpha `140`. Shader affine skip-ahead produces the same
texels as the sequential browser generator; no generated texture file is needed.

## 10 FPS regression: bounded filter dispatch (2026-10-29)

**Impact.** Release on the RTX 3060 Ti at 5120×2880, ordinary initial garden with
Debug Panel searching UI Glass: Gaussian mode reproduced **104–106 ms/frame,
about 9.5 FPS**. Closing Debug Panel still left ~96 ms/frame because other panels
also refreshed the full viewport. Pixelated mode was already ~60 FPS. The
`egui.render` GPU scope isolated the bottleneck: ~98.4 ms of ~102 ms total GPU
time. No second game process was running. These were native Release measurements,
not debug tests or inferred operation counts.

**Cause and introduction.** The painter-order composition change `d5415831`
(2026-10-29) refreshed two full-viewport Gaussian passes at every glass boundary.
Even small panels processed the entire 5K image, repeatedly. Dense sampling and
DPI-scaled radius amplified that unnecessary work. The observed baseline is the
pre-fix `1955a3e5` Release binary; the introducing change is identified by source
history, not an independently benchmarked historical build. The later pixelated
switch exposed the slow unchecked path rather than causing it.

**Fix and ownership.** Each compositor draw pass derives its backdrop demand
from actual glass mesh bounds intersected with egui clipping; ordinary text and
controls do not enlarge it. `matte_blur/region.rs` owns the sigma/radius and
dispatch plan together, and passes both to the shader:
- Gaussian vertical outputs cover that footprint plus one conservative sampler
  texel. Horizontal outputs include every row the vertical kernel can read,
  clamped only at the viewport boundary. Horizontal input still reads the full
  kernel from the authoritative current composite.
- Pixelated dispatch rounds outward to complete cells (now panel-anchored), including
  pixels outside the panel/clip within each cell; only viewport-edge cells are
  cropped. Packed intermediate coordinates remain unchanged.
- Rounded-up workgroups are checked against exact dispatch extent before applying
  origin. Empty/offscreen requests do not dispatch. Every sampled intermediate
  is written for the current surface; unrelated stale pixels cannot leak in.

No downsampling, reduced radius, sparse taps, opacity workaround or skipped
lower-panel content. Existing material, painter order, images and GPU retirement
are unchanged. Runtime region tests cover fractional DPI, clip bounds, offscreen
surfaces, partial cells, and every horizontal texel required by vertical reads.

**Measured results and validation.**

| Native case, Debug open | Before frame / UI GPU | After frame / UI GPU |
| --- | --- | --- |
| 5120×2880, Gaussian | 104.48 / 98.426 ms | 16.88 / 7.171 ms |
| 5120×2880, pixelated | 16.77 / 3.354 ms | 17.38 / 1.526 ms |
| 4096×2560, Gaussian | 69.03 / 62.667 ms | 17.09 / 5.771 ms |
| 4096×2560, pixelated | 17.03 / 2.521 ms | 17.22 / 1.615 ms |

At the default present mode, restored frame delivery is around 58–60 FPS; frame
intervals are not uncapped throughput. The script keeps the last 30 frame samples
and up to three GPU scope samples per short run, so these are diagnostic results,
not broad scene/population acceptance. Larger actually covered areas still cost
more. Initial evidence: `target/matte-perf/{before,after}.json` and matching logs.
A longer alternating run in `paired-3SDNkj/` detected a live monitor-resolution
change (5120×2880 → 4096×2560) and correctly aborted rather than aggregate unmatched
cases. Its first cold baseline was also slower; that incomplete run is not used
for the table. A fresh complete four-case pair at 4096×2560 is retained in
`target/matte-perf/paired-ymga5q/`.

Reproduce matched measurements with two built Release binaries:

```sh
node scripts/benchmark-ui-glass.mjs --before /path/to/before --after target/release/re-flora
```

Stop other GPU workloads; do not build concurrently or change monitor resolution.
The script alternates binaries, checks dimensions/quality, isolates config, records
input hashes and preserves raw logs. It does not set a universal FPS threshold.

- Both native transmission suites pass under Vulkan synchronization validation:
  `target/matte-stack/run-fDZwY6/` (Gaussian, eight cases) and
  `target/matte-stack/run-NdmkaA/` (pixelated, ten including resize and area means).
  Responses and pixel-cell errors are unchanged from the unbounded implementation.
- Compared Gaussian overlap crops to pre-fix screenshots: reversed order is exact;
  remaining tested red/blue/fade/clip crops differ by at most one 8-bit channel
  step, with mean difference below 0.00000014. No new edge contamination observed.
- `cargo fmt --check`, `cargo check`, `cargo build --release`, hidden/muted
  smoke and three Slang UI contracts pass. Full Rust: **1,743 passed, 9 failed,
  13 ignored**, plus four collision tests. The same pre-existing config-default
  and raster-tree assertions fail. Twelve renderer tests pass.
- Shader-derived push structs regenerated by `cargo check`; no saved config
  changes or image/audio assets.

## Panel-local pixel grid (2026-10-29)

The pixelated material now belongs to the panel, not the screen. Its grid follows
the full, unclipped surface rectangle carried by the typed paint boundary. Real
window dragging and egui layer translations move this rectangle with the geometry.
The renderer snaps that origin to the nearest physical pixel (ties toward positive
infinity, including offscreen negative positions), then uses an equivalent bounded
grid phase for packed intermediate indexing. The same phase reaches the region
planner, both compute passes and the material fragment; there is no second origin
derived from visible/clipped bounds.

Each panel has an independent grid. Upper cells therefore average the actual
already-composited lower cells even when their boundaries no longer align.
Colors update from the current scene as the panel moves; grid edges stay attached
to the panel. Own text/controls remain sharp. Partial cells at either viewport
edge count only valid pixels. The area-limited performance fix is preserved, with
no extra images, full-screen fallback or changes to saved controls.

Validation:
- Real egui pointer press/move/release drags a window by (7, 5) and moves its paint
  boundary origin by exactly (7, 5). Phase tests cover fractional DPI, negative
  origins and integer-pixel translation across zero. Region tests prove shifted
  cell reads remain covered after clipping.
- The Slang area-mean contract covers every possible grid phase at sizes 1, 4, 9,
  12, 21, 24 and 96 against brute-force 2D sums, including both viewport edges.
  The Gaussian and material contracts also pass.
- Fourteen native pixelated captures under Vulkan synchronization validation pass:
  original/reversed/faded/clipped stacks plus 12/21-pixel patterned grids, moved
  middle panels, clipped moved panels and startup resize. The oracle independently
  averages each panel's differently aligned lower layer. Tested cells are exactly
  uniform and match expected color within **0.48 of one 8-bit step**.
  Evidence: `target/matte-stack/run-KukP3Y/`; inspected moved native screenshot.
  The eight Gaussian cases retain their original transmission responses:
  `target/matte-stack/run-z3mlyD/`.
- Matched native Release measurements at 4096×2560:
  Gaussian frame **17.28 → 17.18 ms**, UI GPU **5.769 → 5.787 ms**;
  pixelated frame **17.12 → 17.07 ms**, UI GPU **1.621 → 1.238 ms**.
  This confirms no return of the full-screen filtering regression in this scene,
  not a universal speedup claim. Evidence: `target/matte-perf/paired-Bkikfy/`.
- `cargo fmt --check`, `cargo check`, Release build, hidden/muted smoke and
  15 renderer tests pass. Full Rust: **1,746 passed, 9 failed, 13 ignored**, plus
  four collision tests; failures remain the same two config-default and seven
  raster-tree baseline assertions. Shader-derived structs regenerated normally.
  No saved settings or image/audio assets changed.

## Pixelated glass A/B (2026-10-29)

Debug Panel → **UI Glass** owns two saved declarations:
- **Panels: pixelated glass (A/B)**: unchecked retains the original Gaussian
  frost and grain, checked selects area-averaged square cells. Default is off.
- **Panels: glass cell size (points)**: 4–48 logical points, default 12; enabled
  only in pixelated mode. The renderer rounds points × UI scale to whole physical
  pixels. Each panel's unclipped top-left anchors its own grid; moving the panel
  moves the cells. Clipping and overlapping panels do not reset or share that anchor.

The experimental mode averages **every physical pixel** of the current lower
composite within each cell, in the same encoded-sRGB reference space as the
original material. It does not Gaussian-blur first or pick one nearest pixel.
Desaturation and fern tint are retained; per-pixel grain is omitted to keep the
fully covered glass background exactly one RGB color per cell. Own text/controls
remain sharp and are not part of that uniform-background promise; normal egui
visibility fades and antialiased boundaries still blend with the lower composite.

The existing two compute passes now also support compact separable area means:
horizontal produces one mean per needed cell-column/source-row; vertical produces
one mean per needed final cell. Both results are packed into the upper-left region of the
existing float images. Fragments fetch the exact texel center of their cell.
This reads each source pixel once in the horizontal pass rather than repeating
NxN samples for every output fragment. Partial viewport-edge cells use only their
actual pixels. Toggling changes dispatch and sampling, not descriptor ownership,
scene history, resources, or painter-order dependencies. Intermediate allocation
remains the same 24 bytes/viewport pixel including composition; no new image is
allocated for the experiment. No measured frame-time speedup or performance-budget
acceptance is claimed.

Old config files gain the declarations in memory without being rewritten.
The real egui checkbox test clicks on/off/on in one UI context, saves/reloads the
checked state and cell size, and checks that old settings remain Gaussian.

Validation:
- `cargo fmt --check`, `cargo check`, `cargo build --release`, hidden/muted
  Release smoke passed. Full Rust tests: **1,739 passed, 9 failed, 13 ignored**,
  plus 4 passing collision tests. The same two config-default and seven raster-tree
  baseline assertions fail; no new failures.
- Direct Slang CPU executions of `ui_matte_pixel_contract`,
  `ui_matte_blur_contract` and `ui_matte_material_contract` pass. Area means match
  brute-force 2D sums, including odd viewport dimensions, partial edge cells,
  one-pixel cells, cells larger than the viewport, and DPI-rounded sizes.
- `node scripts/validate-matte-stack.mjs --pixelated` passes ten native captures
  with Vulkan synchronization validation. These cover red/blue lower contents,
  paint-order reversal, fades, clips, and a patterned source at 12- and 21-pixel
  cell sizes, the latter with startup resize. Every tested cell is byte-for-byte
  uniform; one/two-layer results match the independently calculated area mean
  plus tint to within **0.49 of one 8-bit channel step**. No ERROR/VUID/hazard/panic;
  shutdown `failures=0`. Evidence: `target/matte-stack/run-Kipyd0/`.
- The default eight-case script retains exactly the Gaussian transmission
  responses **0.764706 / 0.339209 / 0.164689**. Evidence:
  `target/matte-stack/run-ZJ3Pwa/`. Native pixelated captures were inspected.
- Generated GUI fields and shader push structs were regenerated with `cargo check`.
  Native validation uses isolated config copies and verifies the user's config hash.
  No imported/generated image or audio assets; screenshots are local test artifacts.

## Stacked-panel transmission validation (2026-10-29)

- **Impact/cause:** since the original material (`7631929b`, 2026-10-28), every
  surface sampled the scene alone. Opaque resolved coverage therefore erased
  previously drawn UI in overlap regions; the dense-blur change did not alter this
  original limitation. Red/blue bottom-panel native captures reproduced it: the
  red-channel response behind two upper panels was exactly **0**, despite a large
  change to the bottom panel. Lowering output alpha would merely mix in sharp lower
  contents, not implement frosted transmission.
- **Fix:** painter-order composition and explicit surface boundaries make the
  backdrop authoritative at each surface. Every earlier primitive is rendered
  exactly once before later surfaces sample it. Adjacent identical-texture glass
  meshes cannot lose their individual boundaries during tessellation.
- `node scripts/validate-matte-stack.mjs` captures eight real hidden/muted Release
  cases: red/blue bottom panels, reversed draw order, half-opacity upper glass,
  and clipped upper glass. Pixel assertions require transmission and successive
  attenuation through one/two upper layers. Red-channel differences across zero,
  one and two layers are **0.764706 / 0.339209 / 0.164689**; reversed order restores
  **0.764706**, half-opacity gives **0.246345**, and outside the upper clip gives
  **0.339163** (the lower panel). Inspected the native captures: lower material and
  contents show through, each panel's own title stays sharp.
- Pixel evidence: `target/matte-stack/run-EFmOjX/{*.png,*.log,summary.json}`;
  original failing captures: `target/matte-stack/before-{red,blue}.png`.
- Real-egui tests cover three adjacent otherwise-mergeable surfaces, nested
  surfaces with intervening text, ordinary/no-UI plans, and exact vertex/index
  offsets without omitted/duplicated draws. Existing collapsed/faded/closed window,
  frame geometry, toolbar layout/input and panel-style tests pass. The toolbar
  shape check now traverses grouped shapes while preserving all original checks.
- `cargo fmt --check`, `cargo check`, `cargo build --release`, ordinary hidden muted
  smoke and three live resizes with stacked glass pass. Vulkan synchronization
  validation reports no ERROR/VUID/hazard/panic and shutdown `failures=0` for
  1152×648, 960×600 and 1280×720. Logs: `target/matte-stack/{sync,live-resize,smoke}.log`.
- Full Rust tests: **1,737 passed, 9 failed, 13 ignored**, plus 4 passing collision
  tests. The failures are the previously documented two config-default and seven
  raster-tree assertions, not new failures. No generated or saved-config changes.
  Native appearance is ready for user review; no performance-budget acceptance.

## Dense-blur and panel-consistency validation (before stacked composition)

- Before the fix, `cargo test telemetry_tabs_use_scene_backed_glass` failed for the
  actual FPS/VRAM panel. Both tabs now pass; Frame Timing has matching material
  and dark-value-text coverage. Existing material/window and toolbar tests pass.
- `shader/tests/ui_matte_blur_contract.slang` executes the same generic filter loop
  as the GPU shader: single-lobe impulse response without gaps/replicas, symmetry,
  unit energy, variance, constant-color preservation, and monotonic step edges at
  0.75×, 1×, 1.75×, and 2× scales. This and the existing material contract pass
  when compiled/run directly with Slang 2025.23.2.
- `cargo fmt --check`, `cargo check`, and `cargo build --release` pass. Full Rust
  tests: **1,732 passed, 9 failed, 13 ignored**, plus 4 passing collision tests.
  The nine failures match the pre-existing list recorded below.
- The full Slang runner stops at `sprinkler_contact_contracts` with
  `shared spray/contact failures=1`; reproduced from unchanged `861dfbe7` shader
  sources. The two UI contracts were therefore also run independently.
- Hidden muted Release startup succeeds. The default Wayland run logs an XDG
  settings portal timeout for `color-scheme`; X11 validation runs are clean.
- Debug Panel open, Vulkan synchronization validation: startup resize and three
  **post-render** resizes (1152×648, 960×600, 1280×720) pass with shutdown
  `failures=0`, no ERROR/VUID/hazard/panic diagnostics. The existing grass pixel
  lifecycle fixture supplies post-render resizes, with `--perf` as required by
  that fixture; these runs are safety checks, not performance benchmarks.
- Native screenshot inspected at `target/matte-glass-validation/panels.png`:
  Debug Panel, backpack, toolbar and FPS/VRAM use the same material; text stays
  sharp. User visual acceptance remains pending. Logs and local-only baseline
  artifacts are alongside it. No saved GUI/camera config changed.

## Initial material validation (before dense blur)

- `cargo fmt --check`, `cargo check`, `cargo build --release`.
- Real-egui tests cover material identity, corner area, text/background order,
  unchanged frame layout, collapsed windows, fade/close lifecycle, toolbar layout
  and input, outline preferences, and selected/hovered control contrast.
- `shader/tests/ui_matte_material_contract.slang` checks all 1,024 grain texels,
  tiling, approved constants, source-over composition, and sRGB conversion. It was
  compiled/run directly using the locally available Slang 2025.11 compiler. The
  repository-wide pinned Slang test runner requires 2025.23 and is blocked on this
  machine's compiler version; it was not reported as passing.
- Hidden muted Release smoke and screenshot captures; Vulkan synchronization
  validation on startup and multi-generation resize with the Debug Panel open.
  Both validation runs exited successfully with `failures=0`, without
  ERROR/VUID/hazard/panic diagnostics.
- Final full Rust tests: **1,713 passed, 9 failed, 13 ignored**, plus 4 passing
  collision tests. The nine failures are pre-existing: two saved-default assertions
  in `gui_config_loader` and seven `tracer::raster_tree` tests. All nine were also
  reproduced in an unchanged detached worktree at `e2a7b5c9`; that temporary
  worktree was removed. They were not silenced or fixed as part of the material.

Local evidence (not shipped assets): `target/matte-glass-native.png`,
`target/matte-glass-debug.png` (resize), `target/matte-glass-debug-full.png`,
`target/vulkan-sync/startup.log`, and the
per-worktree run logs under `target/re-flora-logs/`.
