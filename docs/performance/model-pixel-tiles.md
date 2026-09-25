# Shared model pixel tiles

**Apple promotion:** apples now always use this pipeline. The voxel-appearance
checkbox, old render meshes and old fruit color pipeline have been removed;
canonical collision/attachment metadata remains. The original-mode measurements
below are historical and require the corresponding older checkout/config/script
to reproduce. The current benchmark compares 8/32/64px, using 8px as its cadence
reference (or 128 views with per-texel lighting for the stage-one suite).

For the subsequent per-object-lighting and adjustable discrete-view controls, see
[Stage-one preview and measurements](model-pixel-stage-one.md). It retains live
tile generation; no persistent atlas has been implemented.

## One production algorithm

Butterflies, falling leaves, and both attached/fallen new apples now use:

1. A model adapter supplies the mesh range, current rigid pose, fixed framing size,
   independent N, and material shading.
2. `shader/slang/model_pixel_surface.slang::sampleModelPixelGeometry` performs
   center-ray nearest-hit selection, near/far clipping, conservative projected
   cell coverage, and supporting-surface depth selection.
3. Compute writes the N×N color/depth tile once per frame.
4. `shader/slang/model_pixel_display.slang` reads that tile in screen fragments.
   There is no screen-fragment triangle traversal or model-specific repair loop.

The particle and apple compute entry points are pose/material/resource adapters,
not different sampling algorithms. Attached apples still use the existing GPU
wind and tree-attachment pose. Fallen apples still use the rigid-body stream.
Leaf sprite A/B, collision, growth and flight physics are unchanged. The original
voxel apple render path was retained for these measurements and subsequently retired. Resolution settings remain independent; apple resolution covers both
attached and fallen fruit. No color quantization was introduced.

The legacy CPU planner first creates/prunes eight-neighbor bridge expressions,
then fills all unoccupied conservatively covered cells. That second pass replaces
all visible bridge expressions with geometry seeds. The GPU computes this final
result directly. The complete CPU planner remains the explicit native-review
oracle, alongside a test that its visible outputs supersede the bridge graph.
Center RGBA/depth remain untouched. Coverage may thicken a silhouette and does
not connect through occupied cells/occluders.

## Storage and publication

`src/tracer/model_pixel_tiles.rs` owns demand-sized, frame-slot-safe buffers.
Particle tiles are packed at each visible instance's actual N; inactive/offscreen
slots do not reserve maximum-size tiles. Contiguous draw batches are at most
64 MiB, below Vulkan's portable storage-buffer binding limit. Packing and compute
follow the same back-to-front draw-index stream. A regression test deliberately
uses reversed instance order across the 64 MiB boundary.

Instances, triangles and draw order are published once, after tile offsets and
visibility are final. An earlier upload must not overwrite these fields with
zero-initialized preparation metadata while a GPU frame is consuming them.
Unused batch/tree allocations are retired when the corresponding frame slot is
safe to reuse. Apple buffers cover the submitted tree/fruit draw batches and are
also checked against the storage binding limit. No production tile readback or
maximum-resolution allocation for all 16,384 particle slots is performed.

A shader stage using only transient set 1 needs a valid empty set-0 layout.
The Vulkan pipeline-layout builder now handles such holes correctly rather than
passing null descriptor-set-layout handles.

## Correctness checks

```sh
cargo fmt --check
cargo check
cargo test
node scripts/validate-leaf-model.mjs --seconds 9
python3 scripts/validate_butterfly_mesh.py --seconds 12
node scripts/validate-apple-model.mjs --seconds 12
cargo run --release -- --hidden --mute --auto-exit 0.5
```

The leaf/butterfly review checks the actual GPU tiles against an independent CPU
geometry oracle, including 8/16/22/64px, rotating poses, original RGBA/depth,
conservative coverage and connectivity. Diagnostic-only captures include the
exact GPU rays and triangle ownership. Their frame transform is independently
checked, then CPU ray/triangle tests verify hit depth. This avoids mistaking
CPU/GPU unprojection cancellation on tiny leaves for a missing triangle. The
cell-overlap epsilon is expressed in pixel units on both CPU and GPU.

The apple review initializes attached fruit before startup physics, exercises
live 8/32/64px transitions in both life states, triggers actual
fruit drops through the normal GUI lifecycle path, and exercises native resize
publication. It does not save the temporary settings. Logs and scene captures
are under `target/apple-model-review/`. Apple shape/wind/occlusion screenshots
were inspected; apple-specific numerical GPU/CPU depth readback is not added by
this change (the common sampler is covered by the particle oracle).

A separate production-path run with 1,100 rotating 64px leaves and 21 butterflies
exercised multiple tile batches without native-review readbacks or Vulkan errors.
`target/model-pixel-cross-batch.png` is its scene capture. This is a cross-batch
correctness check, not a performance guarantee for that population.

## Release measurements

```sh
cargo build --release
node scripts/benchmark-model-pixels.mjs --seconds 8
node scripts/benchmark-model-pixels.mjs --seconds 8 --stress-leaves 256 \
  --output target/model-pixel-stress-2880
```

Close other game instances first. The script temporarily edits/restores GUI
configuration, saves a recovery copy, detects concurrent edits, checks runtime
logs, and retains screenshots and JSON measurements. On Linux it pins X11/XWayland
scale to get **2880×1620** physical output instead of allowing hidden Wayland
windows to move between fractional-scale monitors. The unchanged game's scene
render scale was 1440×810 in these runs. Default automatic present-mode selection
is retained; no artificial immediate-present override is used.

Hardware: **NVIDIA RTX 3060 Ti**. Scene: default tree, **17 fruit**, fixed startup
camera; attached cycle 0.7 or falling cycle 1.0. Build mode: Release. Warmup and
screenshot capture are excluded. Frame intervals use timestamp/frame-number
differences between GPU scope log samples, not the slow-frame-biased `PERF/FRAME`
log. Reported FPS is the inverse of the median measured interval, not 1000/GPU-ms.

Baseline is commit **4e08a3d2**, before convergence. It was rebuilt in an isolated
worktree **with its own target directory**. Sharing target directories between
these revisions reused incompatible shader artifacts and was rejected; those
failed starts are not performance samples.

| 32px apples | Old GPU frame p50 | Shared tiles GPU frame p50 | Old measured FPS | Shared tiles measured FPS |
|---|---:|---:|---:|---:|
| Attached | 25.417 ms | 6.808 ms | 31.68 | 58.37 |
| Fallen | 22.303 ms | 6.790 ms | 35.13 | 58.37 |

The original-voxel controls ran at approximately **58.25–58.37 FPS** under this
same desktop/presentation setup. Shared tiles return the scene to that baseline
cadence; this is not a claim of an uncapped 147 FPS. Apple tile generation at
32px costs about **1.5 ms**; at 64px about **3.4–3.8 ms**. At 64px the measured total
GPU p50 was **8.734 ms attached / 9.009 ms fallen**, still at baseline cadence.

Final mixed renderer stress adds **256 rotating leaves + 21 butterflies**, both
16px, with no CPU coverage oracle or GPU readback:

| Apple mode | GPU frame p50 | GPU frame p95 | Measured FPS |
|---|---:|---:|---:|
| Attached, 32px | 7.604 ms | 7.621 ms | 58.48 |
| Fallen, 32px | 7.307 ms | 7.424 ms | 58.25 |
| Attached, 64px | 9.404 ms | 9.482 ms | 58.37 |
| Fallen, 64px | 9.783 ms | 9.869 ms | 58.37 |

The stress fixture is explicitly **renderer-only**, not simulated leaf flight;
real-flight correctness is tested separately. Artifacts:

- `target/model-pixel-baseline-2880/summary.json`
- `target/model-pixel-converged-2880/summary.json`
- `target/model-pixel-stress-2880/summary.json`

Acceptance for this reference scene is default-32px **GPU p95 < 16.67 ms** and
median frame interval within 5% of the original mode or 60 Hz, whichever is slower.
Baseline fails; the shared implementation passes, including the mixed stress.
These are short reference-scene runs, not universal FPS or maximum-population
acceptance. Extremely large visible populations still scale with N² and consume
real tile memory; batching makes bindings safe, not that work free. Other GPUs,
platforms, large gardens and startup shader-compilation stalls require separate
measurements.
