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

This is the default native surface, not an A/B experiment. The approved material
constants live in `shader/slang/ui_matte_material.slang`; there are no new Debug
settings or save hooks. Existing user configuration files are unchanged.

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
  selects glass surfaces.
- `matte_pipeline.rs` owns the material pipeline and backdrop descriptor.
  `matte_blur.rs` owns two full-resolution frost images and the horizontal/vertical
  compute passes. Descriptors and images retain their source generation; replaced
  bindings, images, and pipelines use the completion-scoped retirement sink.
- The renderer runs the blur once **before** entering the UI render pass, only
  when an explicit glass mesh is present. The source is the current tracer
  `screen_output_tex`, not the UI render target. Resource tracking declares the
  scene-read, horizontal-write/read, vertical-write, and fragment-read dependencies,
  including next-frame reuse. No previous-frame image is used as filter input.
- Material meshes and ordinary egui meshes preserve painter order and restore the
  appropriate pipeline, projection, and descriptor when switching between them.
- Only scene pixels are frosted. Text and controls are rendered afterward and stay
  crisp. Overlapping glass panels independently sample the scene: this is not
  recursive panel-on-panel backdrop filtering.
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
clamp to the viewport. Panels share the completed blur and need one backdrop
sample each. This is not a claim of browser pixel-identical filtering.

This replaces the original sparse 7×7 kernel (introduced in `7631929b`,
2026-10-28). At 1× scale its samples were seven pixels apart: a single bright line
produced values `0.3991, 0, 0, 0, 0, 0, 0, 0.2420, ...`, i.e. visible displaced
copies rather than a smooth lobe. The dense filter removes that undersampling
mechanism instead of hiding it with a more opaque tint or temporal noise.

The two full-resolution RGBA16F images cost 16 bytes per viewport pixel combined
(about 31.6 MiB at 1920×1080). The compute passes cover the full viewport when glass
is visible, trading shared continuous filtering for extra bandwidth and storage.
No asynchronous captures or stale-frame readbacks are involved. This change is a
visual correction, **not a measured performance win or budget acceptance**;
performance optimization follows native visual approval.

At 18% body opacity, backdrop brightness still strongly affects contrast, as it
would in the approved web preset. Selected and hovered inset controls have a
contrast regression test; arbitrary scene-backed text is not claimed to meet a
contrast threshold on every possible night scene.

No external image/audio resources were added. The procedural grain uses the
browser's documented seed `1837`, multiplier `1664525`, increment `1013904223`,
RGB levels `90/220`, and alpha `140`. Shader affine skip-ahead produces the same
texels as the sequential browser generator; no generated texture file is needed.

## Dense-blur and panel-consistency validation

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
