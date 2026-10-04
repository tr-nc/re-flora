# Hardware-raster model pixelization for grass

Runtime visual candidate, not a default migration or a demonstrated universal performance win.

## Try it

Run `RE_FLORA_GRASS_STEM_TRYOUT=1 cargo run --release -- --authored-flora-bench`, press **R**, and enable experimental grass in **Stem Geometry & Color Bands**. Select Square or Tapered Square. In **Stem Model Pixelization**, toggle **Grass: model-grid pixelization of raster stems (A/B)**. Unchecked is the existing plain mesh; checked is the new candidate. Samples per height controls the model grid. No Save is necessary for comparison.

Analytic Reference keeps its old path; disabling experimental grass restores voxel rendering. CPU climbing stems and flower geometry are not converted by this checkbox. Ribbons also support the raster sampling path.

## Implementation

The triangle mesh is rasterized into bounded, streamed atlas tiles. Each plant uses its own camera-oriented, model-anchored perspective lattice, derived from the existing `StemModelGrid` semantics. The display pass nearest-samples coverage/color and reconstructs sampled eye depth on the display ray. Transparent cells fail the normal scene depth test. This pixelizes silhouettes, not just surface colors, and is not a fullscreen pixelation filter.

There are no fragment-stage rounded-cone, sphere, or stem-segment intersection loops. GPU preparation, Band color and geometry are shared with the cached mesh candidate. Simulation/growth/wind remain GPU-owned. No per-plant draw calls or CPU readback are introduced: each species/chunk draw is partitioned into bounded batches. The original CPU path adapter remains separate.

An eye-plane singularity uses continuous mesh fallback for that plant, rather than dropping it or allocating unbounded tiles. Subpixel plants use a distance-dependent sampling LOD. These deliberate approximations mean this is not pixel/depth-identical to Analytic Reference. Different geometry also changes the shape. Flat per-Band shading is preserved; coarse sampling can select one neighbouring Band at a block boundary.

The initial atlas is 2048²: 64 MiB RGBA32F color/eye-depth plus 16 MiB D32 depth, allocated lazily and retained after toggling off. Active rectangles are cleared, and neighbouring tiles are clipped. Atlas/view buffers are bounded; framebuffer replacements are retired through frame fences on resize. Plain mesh pose reuse can be disabled independently, but pixelized mode requires prepared GPU poses.

## Initial validation

- `cargo fmt --check`, `cargo check`, Release build.
- `cargo test`: 1,363 passed, four ignored; Vulkan shared-push-range regression test passed separately.
- Slang CPU tests: 34 passed, including lattice-to-raster cell mapping, sampled-depth equivalence to the existing grid remapping, eye-plane finiteness and atlas slot separation.
- Native near/low/inside Release fixtures, and repeated runtime toggles at resolutions 32/45/192/512 with resize lifecycle testing: no Vulkan validation errors or panics after fixes; shutdown failures=0.
- Fixed Vulkan pipeline-layout merging to union vertex/fragment push-constant stage visibility. Shader vertex IDs use Vulkan semantics and transparent misses use far-depth rejection, avoiding unenabled draw-parameter/demote capabilities.

Artifacts: `target/stem-raster-pixels/`. Early experiments before capability/layout fixes are not acceptance measurements. Final same-binary comparisons and limitations are recorded below when completed.
