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
backpack, Debug Panel, soil inspector, and construction/carry hints. Controls use
complementary pale inset fills and dark text. Selection/hover/pressed fills were
updated together to avoid dark text on the old dark backgrounds. Existing panel
outline preferences, layout, input, and save behavior remain intact.

Ordinary egui text, textures, tooltips, diagnostic-only windows, and the startup
splash keep the regular renderer; they are not detected or recolored by the GPU.
The material is not applied to world glass/voxel materials.

## Ownership and render contract

- `src/egui_renderer/matte_glass.rs` owns explicit frame/window adapters and the
  reserved material `TextureId::User`. Non-overlapping strips make pixel corners.
  Ordinary frames reserve a background slot before painting content. egui windows
  do not expose that slot, so the window adapter replaces only its own outer-frame
  shape within its explicit layer/range, preserving the actual frame's faded alpha,
  clipping, outline, and paint order. No global color-key or texture-atlas heuristic
  selects glass surfaces.
- `matte_pipeline.rs` owns the material pipeline and backdrop descriptor. Each
  backdrop descriptor retains its source texture generation. Replacement bindings
  and pipelines use the existing completion-scoped retirement sink.
- The renderer prepares sampled-image resources **before** entering the UI render
  pass. The source is the current tracer `screen_output_tex`, which is now sampled
  as well as used for the swapchain blit. It is not the UI render target.
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

The 7×7, 49-sample Gaussian reference kernel covers ±3 sigma. It is a sparse
approximation, not a claim of pixel-identical CSS blur or the browser's filtering
implementation. It runs only on visible glass fragments, not full-screen. There
are no extra full-screen blur targets, asynchronous captures, or stale-frame
readbacks. Native performance has not been accepted against a budget; compare the
native look first, then measure/optimize the material separately if needed.

At 18% body opacity, backdrop brightness still strongly affects contrast, as it
would in the approved web preset. Selected and hovered inset controls have a
contrast regression test; arbitrary scene-backed text is not claimed to meet a
contrast threshold on every possible night scene.

No external image/audio resources were added. The procedural grain uses the
browser's documented seed `1837`, multiplier `1664525`, increment `1013904223`,
RGB levels `90/220`, and alpha `140`. Shader affine skip-ahead produces the same
texels as the sequential browser generator; no generated texture file is needed.

## Validation

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
