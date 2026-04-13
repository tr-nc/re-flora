# shadow map fix plan

## goal

Replace the invalid `D32_SFLOAT + STORAGE` shadow path with a cross-platform layout that keeps raster depth rendering and compute shadow processing separate, while preserving one unified soft-shadow path on all platforms.

## problem summary

- `shadow_map_tex` is currently created as `D32_SFLOAT` with `DEPTH_STENCIL_ATTACHMENT | STORAGE | SAMPLED | TRANSFER_DST`.
- macOS reports that `D32_SFLOAT` does not support `VK_FORMAT_FEATURE_STORAGE_IMAGE_BIT`.
- the same image is used both as a depth attachment for foliage shadow rendering and as a storage image for compute passes.
- this likely breaks terrain shadow writes and VSM generation on macOS, causing harder shadows.

## target design

Introduce two shadow textures with distinct responsibilities:

- `shadow_map_depth_tex: D32_SFLOAT`
  - used only for the raster foliage shadow pass
  - bound only as a depth attachment and sampled input if needed
- `shadow_map_tex: R32_SFLOAT`
  - used as the unified compute-readable and compute-writable raw shadow texture
  - receives converted foliage depth data
  - receives terrain shadow writes from `tracer_shadow.comp`
  - serves as the source for VSM creation

Keep existing VSM ping-pong textures as they are.

## frame flow after change

1. Clear both `shadow_map_depth_tex` and `shadow_map_tex` to `1.0`.
2. Render foliage shadow pass into `shadow_map_depth_tex`.
3. Run a small conversion pass that copies depth values from `shadow_map_depth_tex` into `shadow_map_tex`.
4. Run `tracer_shadow.comp` to merge terrain depth into `shadow_map_tex` via `min(terrain_depth, existing_depth)`.
5. Run VSM creation and blur passes from `shadow_map_tex` into the existing VSM ping-pong textures.
6. Continue sampling filtered VSM in flora and particle shading.

## implementation steps

1. Add a dedicated raster shadow depth texture.
   - update `TracerResources` to store both `shadow_map_depth_tex` and `shadow_map_tex`
   - keep `shadow_map_depth_tex` as `D32_SFLOAT`
   - change `shadow_map_tex` to `R32_SFLOAT` with storage-compatible usage

2. Update render pass and framebuffer wiring.
   - make the depth-only shadow render pass use `shadow_map_depth_tex`
   - keep the compute path and VSM path bound to `shadow_map_tex`

3. Add a depth-to-float conversion pass.
   - add a tiny compute or fullscreen pass that reads `shadow_map_depth_tex` and writes `shadow_map_tex`
   - keep the implementation minimal and local
   - prefer a compute pass if it fits current pipeline infrastructure cleanly

4. Update shadow compute shaders.
   - keep `tracer_shadow.comp` writing to `shadow_map_tex` as `r32f`
   - keep `vsm_creation.comp` reading from `shadow_map_tex` as `r32f`
   - ensure no shader binds the depth texture as a storage image

5. Update clear and barrier sequencing.
   - clear `shadow_map_depth_tex` as depth
   - clear `shadow_map_tex` as color float `1.0`
   - insert the required barriers between raster depth write, conversion pass, terrain shadow pass, and VSM passes

6. Validate descriptor usage on macOS.
   - confirm the `D32_SFLOAT` storage-image validation error is gone
   - re-check whether the per-stage storage image limit errors remain
   - if they remain, reduce storage-image bindings in affected compute pipelines as a separate follow-up

## files likely involved

- `src/tracer/resources.rs`
- `src/tracer/mod.rs`
- `src/tracer/pipeline_builder.rs`
- `shader/tracer/tracer_shadow.comp`
- `shader/tracer/vsm_creation.comp`
- new shader for depth-to-float conversion

## expected outcome

- no `D32_SFLOAT` storage-image validation error on macOS
- same shadow architecture on all platforms
- soft flora shadows restored through the VSM path
- terrain shadow data merged into the same raw float shadow texture before filtering

## follow-up checks

- compare flora, grass, particles, and terrain shadow softness across macOS and non-macOS
- verify god ray or any direct raw shadow sampling path still reads the intended texture
- inspect whether `tracer.comp` should remain on PCSS from raw shadow data or be moved to filtered VSM for more consistent softness
