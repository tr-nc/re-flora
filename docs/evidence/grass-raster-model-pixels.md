# Hardware-raster model pixelization for grass

Runtime visual candidate, not a default migration or a demonstrated universal performance win.

**Historical measurements below describe the first pixel backend (`510b0304`), not
current smooth/pixelized ablations.** The follow-up
[HTML investigation](grass-pixelization-performance.html) contains the current
modes, repaired Vulkan test/synchronization gates, controlled analytic ablation,
backend phase measurements and cited released-game research. Taper/ribbon numbers
remain historical; those modes have been removed from the active implementation.

## Current runtime controls

Run `RE_FLORA_GRASS_STEM_TRYOUT=1 cargo run --release -- --authored-flora-bench`, press **R**, and enable experimental grass in **Stem Geometry & Color Bands**. Select **Analytic Reference** or **Square Color Bands**. In **Stem Model Pixelization**, toggle **Grass: model-grid pixelization (analytic and square stems)**. Unchecked uses continuous geometry sampling/depth; checked pixelizes the selected model's silhouette and sampled depth. This is the same saved checkbox for both remaining grass modes, not a geometry-mode change. Samples per height controls the model grid. No Save is necessary for comparison.

Disabling experimental grass restores voxel rendering. Analytic Reference now obeys this checkbox rather than always pixelizing. CPU climbing stems and flower geometry are not converted by it; existing flower controls remain independent. Legacy saved geometry choices 2/3 normalize to Square in memory without rewriting unrelated saved settings.

## Implementation

The triangle mesh is rasterized into bounded, streamed atlas tiles. Each plant uses its own camera-oriented, model-anchored perspective lattice, derived from the existing `StemModelGrid` semantics. The display pass nearest-samples coverage/color and reconstructs sampled eye depth on the display ray. Transparent cells fail the normal scene depth test. This pixelizes silhouettes, not just surface colors, and is not a fullscreen pixelation filter.

There are no fragment-stage rounded-cone, sphere, or stem-segment intersection loops. GPU preparation, Band color and geometry are shared with the cached mesh candidate. Simulation/growth/wind remain GPU-owned. No per-plant draw calls or CPU readback are introduced: each species/chunk draw is partitioned into bounded batches. The original CPU path adapter remains separate.

An eye-plane singularity uses continuous mesh fallback for that plant, rather than dropping it or allocating unbounded tiles. Subpixel plants use a distance-dependent sampling LOD. These deliberate approximations mean this is not pixel/depth-identical to Analytic Reference. Different geometry also changes the shape. Flat per-Band shading is preserved; coarse sampling can select one neighbouring Band at a block boundary.

The current atlas is 2048²: 64 MiB RGBA32F color/eye-depth plus 16 MiB D32 depth, allocated lazily and retained after toggling off. Active rectangles are cleared, and neighbouring tiles are clipped. Atlas/view buffers are bounded; framebuffer replacements are retired through frame fences on resize. Plain mesh pose reuse can be disabled independently, but pixelized mode requires prepared GPU poses.

## Historical validation at the first pixel-backend stage

The particle-artifact failure and acquire hazards listed here were subsequently
reproduced and repaired, not suppressed. Current Vulkan tests and synchronization
fixtures are documented in the [HTML follow-up](grass-pixelization-performance.html).


- `cargo fmt --check`, `cargo check`, Release build.
- `cargo test`: 1,364 passed, four ignored; Vulkan shared-push-range regression test passed separately.
- Full Vulkan crate suite: 53 passed, one pre-existing failure: `particle_vertex_shader_reflects_one_compact_mesh_input_before_instances` requests the retired `shader/particles/particle_lod_textured.vert` artifact (`shader_module.rs:1173`). The test is unchanged from before this work, and that logical shader was already absent from the baseline manifest. No unrelated repair was made.
- Slang CPU tests: 34 passed, including lattice-to-raster cell mapping, sampled-depth equivalence to the existing grid remapping, eye-plane finiteness and atlas slot separation.
- Native near/low/inside Release fixtures, and repeated runtime toggles at resolutions 32/45/192/512, partial/restarted growth, and three resizes after tiles had been rendered: no Vulkan validation errors or panics after fixes; shutdown failures=0.
- Fixed Vulkan pipeline-layout merging to union vertex/fragment push-constant stage visibility. Shader vertex IDs use Vulkan semantics and transparent misses use far-depth rejection, avoiding unenabled draw-parameter/demote capabilities.

An optional **synchronization-validation** sweep is not clean: plain-mesh baseline and pixel lifecycle runs each reported 196 identical swapchain-acquire `WRITE_AFTER_READ` hazards during loading. Duplicate-message suppression was raised to 10,000. No atlas/view-buffer-specific hazards were reported. This reproduces a baseline problem; it is not a blanket synchronization-clean claim. Logs: `target/stem-raster-pixels/sync-{baseline,lifecycle-full}.log`.

## Historical same-binary Release comparison

These analytic measurements had built-in pixelization; they were **not** a
smooth analytic control. Current analytic pixelization on/off measurements must
come from the follow-up suite, not this table.


Measured revision: `510b0304678b3b7c9cb048781c7bacff4e1f0a24`, RTX 3060 Ti, 2560×1440. The lifecycle/logging follow-ups at that stage did not change ordinary rendering. The later mode removal, shared analytic checkbox and correctness repairs are covered separately in the HTML follow-up. **56 successful serial runs**, two reverse-order repeats, **592 sampled frames per metric**, resolution 45, pose reuse enabled. Saved GUI/camera file hashes stayed unchanged during the suite. All runs passed normal Vulkan validation/shutdown checks.

Historical invocation at the measured revision: `cargo build --release && node scripts/validate-stem-band-candidates.mjs --suite gpu --pixels`. The current script produces the remaining five smooth/pixelized controls, not this retired seven-candidate matrix. Raw data/screenshots: `target/stem-band-trials/full-gpu-pixels/{runs.json,summary.json}`. Contact sheet: `target/stem-raster-pixels/near-comparison.png`. Early experiments before capability/layout fixes are not acceptance measurements.

### Grass drawing/backend, p50 µs

The pixelized `graphics.flora` scope includes grid preparation, tile clearing/rasterization, sample display, fallback and batch pass transitions—not just the final display draw.

| Residents / camera | Voxel | Analytic | Square | Taper | Ribbons | Square pixels | Taper pixels |
|---|---:|---:|---:|---:|---:|---:|---:|
| 4,098 / near | 104 | 1,948 | 142 | 102 | 90 | 1,285 | 1,125 |
| 82,454 / wide | 610 | 3,080 | 1,172 | 1,204 | 570 | 6,584 | 6,052 |
| 82,454 / far | 868 | 4,635 | 1,170 | 1,169 | 580 | 9,586 | 8,234 |
| 82,454 / low | 1,399 | 1,782 | 1,169 | 1,169 | 484 | 4,491 | 4,438 |

### Drawing/backend + shared lighting/pose preparation, p50 µs

`grass.render_total` is the **per-frame paired sum** of `graphics.flora` and `graphics.flora_lighting_cache`, not a sum of independent medians. Preparation includes shared flora work. Small initial render-pass setup overhead remains outside this sum and inside full-frame timing.

| Residents / camera | Voxel | Analytic | Square | Taper | Ribbons | Square pixels | Taper pixels |
|---|---:|---:|---:|---:|---:|---:|---:|
| 4,098 / near | 247 | 2,054 | 303 | 262 | 251 | 1,416 | 1,260 |
| 82,454 / wide | 1,243 | 3,626 | 1,800 | 1,831 | 1,256 | 7,208 | 6,677 |
| 82,454 / far | 1,417 | 5,182 | 1,796 | 1,795 | 1,233 | 10,213 | 8,860 |
| 82,454 / low | 1,944 | 2,328 | 1,796 | 1,795 | 1,195 | 5,115 | 5,063 |

### CPU submission and whole GPU frame

For 82,454/wide, p50 µs:

| Scope | Analytic | Plain taper | Square pixels | Taper pixels |
|---|---:|---:|---:|---:|
| Shared lighting/pose preparation (GPU) | 546 | 626 | 625 | 624 |
| CPU tracer recording (`render.trace_record`) | 594 | 579 | 4,672 | 4,660 |
| CPU submit/present | 142 | 142 | 167 | 166 |
| Whole GPU `frame.render` | 7,936 | 6,241 | 11,560 | 11,018 |

CPU recording is not isolated grass time. Submit/present is separate from recording and acquisition. These are not uncapped whole-game FPS measurements. GPU scope savings do not translate proportionally to frame gains, especially with shared/asynchronous work.

## Historical result and remaining limits

This section records conclusions at the measured revision, before the later
analytic ablation and phase diagnostics. See the HTML follow-up for current evidence.


**Visual candidate delivered; mass-grass performance acceptance failed.** Small-near tapered pixelization is ~39% cheaper than analytic including shared preparation, but still ~4.8× plain taper's paired cost. Large-wide tapered pixelization is ~84% more expensive than analytic; paired p95 is 7,419 µs versus analytic 3,903 µs. Large-far and low also regress. Original/plain modes remain available and defaults have not migrated.

The shader avoids analytic intersections, but atlas work, coverage overdraw, repeated pass transitions and CPU descriptor/batch recording can more than consume that saving. The large CPU recording increase is measured; exact GPU cost attribution among tile rasterization, composition and pass/bandwidth overhead has not been isolated. No claim is made that pixelization alone caused the original analytic cost: an analytic ray-quantization ablation has not been implemented.

After visual approval, a separate performance stage should profile those phases and consider visibility/resolution binning, fewer main-pass restarts and cheaper sample storage. Increasing atlas capacity trades VRAM for fewer batches and is not a free fix. High requested resolutions also increase batch count even when per-plant sampling LOD limits the active rectangle. CPU stems/flowers, cross-platform GPU behavior and universal dynamic pixel/depth equivalence are not established by this grass-only comparison.
