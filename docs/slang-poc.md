# Slang shader proof of concept

For migration status, completion criteria, the 76-entry-point checklist, and next tasks, see [`slang-migration-roadmap.md`](slang-migration-roadmap.md). This document is the operator guide and technical evidence record.

This experiment incrementally replaces selected GLSL compute shaders with equivalent Slang implementations. The normal build remains GLSL-only, and each replacement can be enabled independently for matched comparison.

The first pass, `shader/tracer/post_processing.comp`, was selected because it runs every frame at the full output resolution, already has a Vulkan timestamp scope, and exercises a uniform buffer, formatted storage images, bounds checks, and a reusable dither module. The surface and contree-leaf passes cover difficult shared-memory, synchronization, atomic, and structured-buffer paths. The main tracer first established a GLSL-through-Slang backend baseline and now also has a complete native Slang implementation.

## Requirements

Install a Slang distribution that includes both `slangc` and the compiler shared library. For the checksum-pinned v2025.23 release used by CI:

```bash
python scripts/install_slang.py
export SLANGC="$PWD/.tools/slang-2025.23/bin/slangc"
```

The installer supports Linux x86_64/aarch64, macOS x86_64/aarch64, and Windows x86_64. It verifies platform-specific SHA-256 digests before extraction; setting `SLANGC` is sufficient for the build to locate the adjacent shared library. The build locates the library through:

1. the `SLANG_LIB` environment variable,
2. a library beside or under the installation containing `SLANGC`,
3. `$VULKAN_SDK`, or
4. the installation containing `slangc` on `PATH`.

The build dynamically loads the compiler API only when a Slang feature is enabled. A default build does not locate or load Slang. The initial validation used Vulkan SDK 1.4.321.0 and Slang `2025.11-12-gc5295eae2` on macOS; the shared-session build-cost validation used Vulkan SDK Slang `2025.23.2` on Linux. The portable CI contract pins official Slang v2025.23 archives, which also pass the complete local aggregate build. Native Slang sources pin the Slang 2025 language rules and column-major matrices. The GLSL frontend uses row-major lowering to preserve the existing GLSL std140 matrix bytes. Both paths emit SPIR-V 1.6 with Vulkan GL-compatible buffer layout.

## Build and validation

The default build does not invoke Slang:

```bash
cargo check
```

Enable an individual replacement or all completed validation candidates with:

```bash
cargo check --features slang-post-processing
cargo check --features slang-chunk-writer-buffer-setup
cargo check --features slang-chunk-writer-heightmap
cargo check --features slang-chunk-writer-init
cargo check --features slang-chunk-writer-model-voxelize
cargo check --features slang-chunk-writer-modify
cargo check --features slang-chunk-writer-modify-sample
cargo check --features slang-chunk-writer-solid-sample
cargo check --features slang-chunk-writer-voxel-property-sample
cargo check --features slang-chunk-writer
cargo check --features slang-chunk-writer-terrain-smooth-heights
cargo check --features slang-chunk-writer-terrain-smooth-target
cargo check --features slang-chunk-writer-terrain-smooth-apply
cargo check --features slang-chunk-writer-terrain-smooth
cargo check --features slang-chunk-writer-terrain-moisture-brush
cargo check --features slang-chunk-writer-terrain-fertility-brush
cargo check --features slang-chunk-writer-terrain-moisture-dry
cargo check --features slang-chunk-writer-terrain-moisture-spread
cargo check --features slang-chunk-writer-terrain-soil-mix
cargo check --features slang-chunk-writer-terrain-smooth-mbo-init
cargo check --features slang-chunk-writer-terrain-smooth-mbo-diffuse-ab
cargo check --features slang-chunk-writer-terrain-smooth-mbo-diffuse-ba
cargo check --features slang-chunk-writer-terrain-smooth-mbo-score
cargo check --features slang-chunk-writer-terrain-smooth-mbo-apply
cargo check --features slang-chunk-writer-terrain-smooth-mbo
cargo check --features slang-scene-accel
cargo check --features slang-sprinkler
cargo check --features slang-terrarium-glass-vert
cargo check --features slang-terrarium-glass-frag
cargo check --features slang-terrarium-glass
cargo check --features slang-composition
cargo check --features slang-composition-backend
cargo check --features slang-surface-active-to-flora-instances
cargo check --features slang-surface-clear-occupancy
cargo check --features slang-surface-edit-occupancy-capsule
cargo check --features slang-surface-instances-to-occupancy
cargo check --features slang-surface-make
cargo check --features slang-surface-make-sparse
cargo check --features slang-surface-occupancy-to-flora-instances
cargo check --features slang-surface-prepare-active-flora-dispatch
cargo check --features slang-surface-prepare-sparse-dispatch
cargo check --features slang-surface-update-flora-growth
cargo check --features slang-surface
cargo check --features slang-contree-leaf
cargo check --features slang-denoiser-spatial
cargo check --features slang-denoiser-temporal
cargo check --features slang-denoiser
cargo check --features slang-egui
cargo check --features slang-flora
cargo check --features slang-foliage-flora-lod
cargo check --features slang-foliage-leaves-lod
cargo check --features slang-foliage-leaves-shadow-frag
cargo check --features slang-foliage-leaves-shadow-vert
cargo check --features slang-foliage-leaves-shadow
cargo check --features slang-foliage-leaves-vert
cargo check --features slang-foliage
cargo check --features slang-particles-lod-textured-frag
cargo check --features slang-particles-lod-textured-vert
cargo check --features slang-particles-lod-textured
cargo check --features slang-particles-water-droplet
cargo check --features slang-particles
cargo check --features slang-player-collider
cargo check --features slang-tracer
cargo check --features slang-tracer-backend
cargo check --features slang-tracer-leaf-shadow-mask
cargo check --features slang-tracer-leaf-shadow-temporal
cargo check --features slang-tracer-leaf-shadow
cargo check --features slang-tracer-lens-flare-downsample
cargo check --features slang-tracer-lens-flare-generate
cargo check --features slang-tracer-lens-flare-sun-visible
cargo check --features slang-tracer-lens-flare
cargo check --features slang-tracer-wind-volume
cargo check --features slang-tracer-terrain-query
cargo check --features slang-tracer-god-ray
cargo check --features slang-tracer-cloud-shadow-temporal
cargo check --features slang-tracer-cloud-temporal
cargo check --features slang-tracer-cloud
cargo check --features slang-tracer-cloud-shadow
cargo check --features slang-tracer-clouds
cargo check --features slang-tracer-shadow
cargo check --features slang-tracer-shadow-depth-copy
cargo check --features slang-tracer-vsm-blur-h
cargo check --features slang-tracer-vsm-blur-v
cargo check --features slang-tracer-vsm-creation
cargo check --features slang-tracer-vsm
cargo check --features slang-validation
```

`slang-poc` remains a backward-compatible alias for `slang-post-processing`.

The build reports each frontend separately, for example:

```text
precompiled 0 shaderc GLSL, 0 Slang GLSL, and 76 native Slang shaders into SPIR-V artifacts
```

The logical shader path remains the runtime identity for both languages. Enabling an override compiles the original GLSL reflection artifact as a reference and fails the build if the replacement changes the pipeline ABI: stage, workgroup size, descriptor contract, top-level buffer member byte layout and array stride, push constant ranges/member layout, stage IO locations/formats/arrays, or interpolation decorations. Override configuration is also checked for duplicate and missing paths, stage mismatches, and missing source/include paths. Nested compiler-specific matrix wrappers and the final Rust resource mapping are still validated by the existing reflection/resource path at runtime.

The Slang compiler validates generated SPIR-V by default. Runtime validation additionally covers SPIR-V reflection, descriptor names and bindings, uniform layout lookup, Vulkan pipeline creation, dispatch, and MoltenVK execution.

## Comparing GPU time

Run matched release-mode samples from the same worktree and note the two run-log paths:

```bash
cargo run --release -- --hidden --mute --auto-exit 8 --perf
cargo run --release --features slang-poc -- --hidden --mute --auto-exit 8 --perf
```

Then compare the existing GPU timestamp scope while discarding startup frames:

```bash
scripts/compare_gpu_scope.py <glsl-log> <slang-log> \
  --scope post_processing.pass --min-frame 120
```

## Initial result

Test hardware and workload:

- Apple M4 Pro through MoltenVK
- 5120x2880 hidden swapchain
- normal release-mode render configuration
- eight-second runs
- samples before frame 120 discarded

| Frontend | Samples | Mean | Median | P95 | Range |
| --- | ---: | ---: | ---: | ---: | ---: |
| GLSL / shaderc | 15 | 713.67 us | 713 us | 717.90 us | 711-720 us |
| Slang / slangc | 13 | 712.38 us | 712 us | 715.20 us | 709-717 us |

The measured mean delta was `-0.18%`, far below run-to-run noise. This simple pass therefore shows performance parity on the tested machine, not evidence that either frontend is faster.

A second matched run disabled variable rendering features and captured the 5120x2880 output after one second. The GLSL and Slang screenshots were byte-identical across all 14,745,600 RGBA pixels, providing a deterministic output check in addition to successful dispatch.

The optimized SPIR-V was 1,632 bytes from shaderc and 2,028 bytes from Slang. After stripping debug names, the Slang module still contained about 8% more SPIR-V instructions, primarily conversion and bitcast operations. The driver produced equivalent measured GPU time for this memory-bandwidth-dominated pass.

A 20-run warm compiler-process measurement produced median times of approximately 47.7 ms for `glslc -O` and 178.9 ms for `slangc -O3`. This is not the full Cargo build cost, but it confirmed that launching one `slangc` process per artifact would scale poorly. The build now uses the compiler API through one dynamically loaded global session instead.

## Shared compiler-session result

The previous build launched `slangc` twice for each selected entry point: once for reflection and once for optimized output. The current build creates one Slang global session, then performs those compile requests through the in-process API. The default GLSL build still does not locate or load the Slang library.

Three local Linux samples of `cargo check --features slang-validation` produced:

| Aggregate check | Separate `slangc` median | Shared API median | Delta |
| --- | ---: | ---: | ---: |
| Package-clean `re-flora-vkn` rebuild | 6.29 s | 5.05 s | -19.7% |
| Shader-touched incremental rebuild | 5.83 s | 4.66 s | -20.1% |

The package-clean ranges were 6.24-6.38 s before and 5.05-5.10 s after. The shader-touched ranges were 5.82-5.83 s before and 4.60-4.71 s after. All 16 reflection and optimized artifacts from that API-path snapshot were byte-identical to artifacts generated by equivalent standalone `slangc` commands. The current aggregate submits 152 selected artifacts through the shared session, and all 152 aggregate artifacts pass `spirv-val --target-env vulkan1.3`. The current 76-entry aggregate hidden release smoke run completes on an NVIDIA RTX 3060 Ti through native Vulkan; the preceding 16-entry aggregate also passed through MoltenVK. Cargo logs the loaded compiler build tag, and the cross-platform CI installer pins official v2025.23 archives by SHA-256; hosted matrix results remain pending.

## Complete aggregate native-Vulkan performance gate

The completed 76-entry aggregate was measured against default GLSL on an NVIDIA RTX 3060 Ti. Four eight-second hidden muted release runs used a fixed `player-default` camera and order `GLSL, Slang, Slang, GLSL`; samples before frame 120 were discarded. The combined scope statistics were:

| GPU scope | Frontend | Samples | Mean | Median | P95 | Range |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `frame.render` | GLSL | 128 | 2902.84 us | 2842.5 us | 3208 us | 2797-3625 us |
| `frame.render` | Native Slang | 127 | 2946.80 us | 2853 us | 3301 us | 2813-4540 us |
| `tracer.render` | GLSL | 128 | 1622.72 us | 1590 us | 1815 us | 1571-1889 us |
| `tracer.render` | Native Slang | 127 | 1647.28 us | 1601 us | 1867 us | 1586-2269 us |
| `tracer.pass` | GLSL | 128 | 466.22 us | 466 us | 475 us | 460-486 us |
| `tracer.pass` | Native Slang | 127 | 472.54 us | 471 us | 485 us | 466-501 us |
| `graphics.pass` | GLSL | 128 | 36.35 us | 36 us | 37 us | 35-39 us |
| `graphics.pass` | Native Slang | 127 | 35.88 us | 36 us | 37 us | 35-47 us |
| `composition.pass` | GLSL | 128 | 62.80 us | 62 us | 63 us | 61-89 us |
| `composition.pass` | Native Slang | 127 | 62.36 us | 62 us | 63 us | 61-65 us |
| `post_processing.pass` | GLSL | 128 | 96.09 us | 96 us | 97 us | 95-108 us |
| `post_processing.pass` | Native Slang | 127 | 95.30 us | 95 us | 96 us | 94-115 us |

The complete aggregate moved the frame-render median by `+10.5 us` (`+0.37%`) and `tracer.render` by `+11 us` (`+0.69%`). The known main-tracer delta remained small at `+5 us` (`+1.07%`), while graphics and composition medians were unchanged and post-processing was 1 us lower. Per-run frame medians were 2844/2840 us for GLSL and 2845/2855 us for native Slang, so no material frame-level regression is present on this driver. Broader vendor coverage remains a Phase 5 requirement.

The Phase 2 reassessment measured the current 11-entry aggregate against default GLSL on Apple M4 Pro with Slang `2025.11-12-gc5295eae2`. Package-clean samples ran `cargo clean -p re-flora-vkn` before the root check; shader-touched samples updated `shader/include/core/definitions.glsl` before each check. Three samples per frontend used order `default, aggregate, aggregate, default, default, aggregate`:

| Build case | Default GLSL median (range) | Aggregate median (range) | Delta |
| --- | ---: | ---: | ---: |
| Package-clean `re-flora-vkn` rebuild | 3.72 s (3.63-4.57) | 6.37 s (6.20-7.31) | +2.66 s / +71.5% |
| Any-shader-touched rebuild | 3.13 s (3.09-4.22) | 5.54 s (5.47-6.08) | +2.41 s / +76.9% |

At reassessment time, every shader-tree touch still recompiled all 76 entry points and all selected native artifacts. The resulting scaling risk made dependency-aware caching a blocker before complete-family migration.

## Dependency-aware artifact cache

Each logical shader now has an `OUT_DIR` cache manifest for its reflection and optimized SPIR-V. shaderc's include callback records resolved transitive GLSL includes, while Slang's compile-request dependency API records resolved transitive module imports. A selected override caches the union of its GLSL ABI-reference graph and replacement graph, so a fallback ABI change cannot bypass comparison. The context includes the target, frontend configuration, Slang build tag, build script, crate manifest, and lockfile. BLAKE3 digests cover dependency contents and both artifacts; the manifest is removed before recompilation and written only after both outputs succeed.

Three Apple M4 Pro samples per case with Slang `2025.11-12-gc5295eae2` produced:

| Aggregate check | Median (range) | Entries compiled/reused | Delta from pre-cache 5.54 s shader-touch median |
| --- | ---: | ---: | ---: |
| Package-clean rebuild | 6.73 s (6.66-6.98) | 76 / 0 | not comparable; cache is empty |
| Mtime-only shader-tree trigger | 2.29 s (2.25-2.34) | 0 / 76 | -58.7% |
| Edit one GLSL entry (`cloud.comp`) | 2.42 s (2.40-2.67) | 1 / 75 | -56.4% |
| Edit shared native `color.slang` | 4.03 s (3.98-4.10) | 4 / 72 | -27.2% |

Changing the transitively included `core/viridis.glsl` invalidated only its native-tracer logical entry, proving that GLSL reference dependencies participate. Changing `color.slang` invalidated exactly composition, egui vertex, flora vertex, and the main tracer. Corrupting one cached optimized artifact also invalidated and restored only that entry. A clean rebuild's 152 SPIR-V files were byte-identical to the pre-cache aggregate. This closes the incremental-build blocker while preserving GLSL as the default.

Both matched run logs contained the same pre-existing validation warning that one pipeline layout exposes nine storage images on hardware reporting an eight-image per-stage limit. The Slang replacement did not introduce or change that warning.

## Surface extraction and normal generation

The second candidate replaces `shader/builder/surface/make_surface_sparse.comp` with `shader/slang/make_surface_sparse.slang` under `slang-surface-make-sparse`. The `slang-surface` feature aggregates all accepted surface entries. This candidate validates:

- a 512-invocation 8x8x8 workgroup;
- a 12x12x12 three-dimensional `groupshared` tile;
- `GroupMemoryBarrierWithGroupSync`;
- storage-buffer `InterlockedAdd` and `InterlockedOr`;
- std140 uniform and std430 storage blocks with runtime arrays;
- read-only and write-only formatted 3D storage images;
- the nested 5x5x5 normal extraction kernel and bit-packed output.

Slang's `GLSLShaderStorageBuffer<T, Std430DataLayout>` preserves the existing GLSL block ABI and reflection type names. Explicit `readonly` and `writeonly` qualifiers emit the matching SPIR-V `NonWritable` and `NonReadable` decorations. This avoids changing Rust resource allocation while still using Slang modules and entry-point syntax.

Correctness validation used two 50-sample hidden tree benchmarks with each frontend. Every one of the 316 surface dispatch workloads in each matched run had identical chunk, active-voxel, active-brick, and solid-workgroup counts. A 5120x2880 screenshot comparison was identical outside the bottom-right dynamic performance text; all 21 scene differences from the first comparison were confined to that UI text.

For performance comparison, dispatches below 10,000 solid workgroups were excluded so empty upper chunks did not dominate the result. Each run retained 208 heavy dispatches. Two order-reversed matched pairs produced:

| Run order | Mean delta | Median delta | P95 delta |
| --- | ---: | ---: | ---: |
| Slang then GLSL | +5.60% | +0.36% | +9.14% |
| GLSL then Slang | -2.58% | +0.10% | -1.80% |

The stable median is within `+0.4%`, while mean and tail results move substantially with run order because of occasional GPU/OS stalls. The current MoltenVK evidence therefore establishes compatibility and typical-time parity, but it is not yet strong enough to rule out a small tail-latency regression. Native Vulkan measurements and more isolated repeated dispatches remain required.

Reproduce a pair with:

```bash
RUST_LOG=info,re_flora::builder::surface=debug,re_flora::builder::contree=debug \
  cargo run --release -- --hidden --mute --tree-bench --tree-bench-samples 50 \
  --no-tracer --no-shadows --no-denoise --no-god-rays --no-lens-flare \
  --no-clouds --no-flora --no-particles

RUST_LOG=info,re_flora::builder::surface=debug,re_flora::builder::contree=debug \
  cargo run --release --features slang-surface-make-sparse -- --hidden --mute \
  --tree-bench --tree-bench-samples 50 --no-tracer --no-shadows \
  --no-denoise --no-god-rays --no-lens-flare --no-clouds --no-flora \
  --no-particles

scripts/compare_surface_pass.py <glsl-log> <slang-log>
```

The initial optimized surface snapshot contained 498 SPIR-V instructions from GLSL and 508 from Slang after debug stripping. With the dense/sparse shared extraction module, the current Linux artifacts contain 497 and 528 instructions respectively. The remaining difference is small enough that generated-code inspection alone does not explain the run-order-sensitive tail, and the post-refactor runtime regression check remains within the accepted timing envelope.

## Dense surface extraction

`slang-surface-make` replaces the compiled `shader/builder/surface/make_surface.comp` artifact. Dense and sparse entries now share `surface_extraction.slang` as the single owner of 12x12x12 halo preload, atlas bounds, occlusion tests, 5x5x5 normal extraction, voxel packing, and brick indexing. Their entry points retain separate orchestration: dense extraction derives its base directly from `SV_GroupID` and has a two-field result buffer, while sparse extraction decodes a compacted workgroup list and has the additional `solid_workgroup_len` field.

The independent feature and aggregates pass the six-descriptor, 8x8x8 workgroup, buffer-layout, and formatted-image ABI gates. GLSL and native optimized modules request the same `Shader` and `StorageImageExtendedFormats` capabilities and pass Vulkan 1.3 validation. After debug stripping, shaderc emits 467 instructions/7,772 bytes and native Slang emits 494/8,140.

`SurfaceBuilder` currently loads only `make_surface_sparse.comp`, so the dense artifact has no production dispatch or meaningful GPU timing gate. A five-sample native-Vulkan tree benchmark exercised the refactored shared core through the sparse entry: all 44 workloads matched GLSL in chunk identity, active voxels, active bricks, and solid workgroups. The 28 heavy sparse passes had 613 us GLSL and 574 us native medians in this regression pair; the prior order-reversed evidence remains authoritative, and this single pair is used only to rule out a refactor regression.

## Sparse surface dispatch preparation

`slang-surface-prepare-sparse-dispatch` replaces `shader/builder/surface/prepare_sparse_surface_dispatch.comp`. Shared `surface_build.slang` layouts keep its make-surface state identical to the accepted sparse extraction entry, while resource-independent `solid_workgroups.slang` helpers own atlas-grid and linear-index math. The native entry preserves six descriptors, a 128x1x1 workgroup, the `r8ui` atlas dimension query, solid-workgroup flag filtering, atomic compacted-index allocation, and atomic maximum used to build the indirect dispatch.

The independent feature, complete ten-entry `slang-surface` aggregate, and full `slang-validation` aggregate pass the automatic GLSL-reference ABI gate. All 152 aggregate reflection and optimized artifacts pass `spirv-val --target-env vulkan1.3`; an aggregate hidden release smoke run completed on an NVIDIA RTX 3060 Ti without validation errors or panics.

Four five-sample tree benchmarks used order `GLSL, Slang, Slang, GLSL`. Every run produced the same 44 chunk, active-voxel, active-brick, and solid-workgroup workloads. After filtering to the 28 workloads with at least 10,000 solid workgroups per run, the combined results were:

| Frontend | Samples | Mean | Median | P95 range across runs | Overall range |
| --- | ---: | ---: | ---: | ---: | ---: |
| GLSL / shaderc | 56 | 7.04 us | 7 us | 7.65-8 us | 6-8 us |
| Native Slang | 56 | 6.77 us | 7 us | 7-8 us | 6-8 us |

The one-microsecond timestamp resolution dominates this pass, so the result is a correctness and regression guard rather than evidence of a speedup. After stripping debug data, both optimized modules contain 167 SPIR-V instructions; shaderc is 2,788 bytes and native Slang is 2,824 bytes.

## Flora occupancy clearing

`slang-surface-clear-occupancy` replaces `shader/builder/surface/clear_occupancy.comp`. It preserves the two-descriptor ABI, 8x8x8 workgroup, three-component chunk bound, read/write `r32ui` occupancy image, and zero store. The native optimized module is smaller after debug stripping: 52 instructions/792 bytes versus shaderc's 56/864.

The existing authored-flora benchmark normally selects an authored species and bypasses occupancy regeneration. For validation only, its selection was temporarily changed to Grass Mix, and temporary result logging captured the production flora-edit pipeline's before, after, and appended instance counters; neither temporary change remains in the tree. A 25-edit native-then-GLSL pair matched every counter, ending at 16,759 instances through both frontends. Clear-pass timings were:

| Frontend | Samples | Mean | Median | P95 | Range |
| --- | ---: | ---: | ---: | ---: | ---: |
| GLSL / shaderc | 25 | 170.60 us | 169 us | 197.80 us | 160-200 us |
| Native Slang | 25 | 166.68 us | 161 us | 189.40 us | 160-228 us |

Two earlier five-edit order-reversed pairs had 162 us GLSL and 166.5 us native combined medians, with isolated outliers shifting each short run. The larger matched pair and smaller native instruction count show no regression, but the noisy tails do not support a speedup claim.

## Active-surface flora dispatch preparation

`slang-surface-prepare-active-flora-dispatch` replaces `shader/builder/surface/prepare_active_surface_flora_dispatch.comp`. A shared `surface_result.slang` module owns the two-field make-surface result layout used by this entry and dense extraction. The entry preserves two storage-buffer descriptors, its 1x1x1 workgroup, 64 flora invocations per active brick, groups of 128, and the minimum-one-group command required by empty chunks.

Initial loading currently disables `place_flora`, so validation temporarily enabled it and logged the existing five-species result readback; neither temporary Rust change remains. Four runs used order `GLSL, Slang, Slang, GLSL`, each covering eight chunks including four empty chunks. Every non-empty chunk produced identical species counts through all runs, for example `[8217, 19234, 214, 16, 0]` for `UVec3(0, 0, 0)` and `[8014, 18970, 222, 31, 0]` for `UVec3(1, 0, 1)`.

Both frontends had a 3 us combined median across 16 preparation samples; GLSL ranged from 2-3 us with a 2.62 us mean, and native Slang ranged from 2-3 us with a 2.81 us mean. This is timestamp-resolution correctness evidence, not a meaningful performance difference. Debug-stripped artifacts contain 57 instructions/944 bytes from shaderc and 56/956 from native Slang.

## Flora instance restoration to occupancy

`slang-surface-instances-to-occupancy` replaces `shader/builder/surface/instances_to_occupancy.comp`. The existing native `flora_types.slang` module now owns all shared instance packing, growth, seed, spawn-age, and species-buffer offset helpers. A new `flora_occupancy.slang` module owns the transient occupancy encoding for this and the remaining flora construction entries. The entry preserves its uniform block, read/write `r32ui` image, readonly instance storage buffer, and 128x1x1 workgroup ABI.

Validation temporarily changed the authored-flora benchmark from Lavender to Grass Mix and logged production flora-edit results; neither change remains. Two order-reversed pairs covered 35 edits per frontend. Every before/after/appended counter matched, including the full run's final `16,133 -> 16,759`, appending 626 instances. The first edit begins with no instances, so the restoration pass is omitted; the 25-edit runs therefore produced 24 timestamp samples:

| Frontend | Samples | Mean | Median | Range |
| --- | ---: | ---: | ---: | ---: |
| GLSL / shaderc | 24 | 4.29 us | 4 us | 3-8 us |
| Native Slang | 24 | 4.33 us | 4 us | 3-7 us |

The reverse ten-edit pair again had 4 us medians, with one isolated 51 us native timestamp outlier. After debug stripping, shaderc emits 100 instructions/6,020 bytes and native Slang emits 95/5,988. The matched semantic outputs and stable median establish parity; the timestamp tails are too coarse for a speed claim.

## Flora growth updates

`slang-surface-update-flora-growth` replaces `shader/builder/surface/update_flora_growth.comp`. New shared `flora_surface_build.slang` and `grass_growth_potential.slang` modules own the occupancy-result/instance buffer layouts and packed four-bit grass competition field. The entry preserves two descriptor sets, its fixed five-element species result array, mutable instance storage, 128x1x1 workgroup, growth-limit clamp, and atomic growing flag.

Runtime validation temporarily changed the authored-flora benchmark to Grass Mix. Each of 25 steps added instances, trimmed them below maturity, dispatched a five-tick growth update, and read back a canonical hash of sorted `(species, packed_local_position_and_growth)` values. This removes expected atomic output ordering and spawn-clock differences while retaining every position and growth byte. All 25 hashes, before/after counts, and `has_growing_flora` results matched between frontends; neither the benchmark hook nor readback remains in the tree.

Both frontends had 6 us pass medians in the final Slang-then-GLSL pair. Native Slang had a 5.60 us mean and 5-7 us range; shaderc's normal samples were comparable but isolated 322 us stalls raised its mean to 30.92 us. This supports parity, not a native speed claim. After debug stripping, shaderc emits 87 instructions/5,376 bytes and native Slang emits 96/5,636.

## Capsule occupancy editing

`slang-surface-edit-occupancy-capsule` replaces `shader/builder/surface/edit_occupancy_capsule.comp`. Shared modules now own gradient noise, flora placement/paint masks, surface planting policy, Wellons hashing, occupancy encoding, instance packing, grass competition, and voxel decoding. `flora_noise.slang` was changed to consume the same gradient-noise and hash modules instead of retaining duplicate implementations. The entry preserves its 96-byte uniform, two `r32ui` storage images, packed competition storage buffer, and 8x8x8 workgroup ABI.

Validation temporarily switched the authored-flora benchmark to Grass Mix and followed every add with a trim. Each edit read back an order-independent hash of sorted species/position/growth values, excluding only expected atomic ordering and spawn-clock differences. Four 25-step runs used order `GLSL, Slang, Slang, GLSL`; all 200 add/trim hashes and all trim counts/growing flags matched. Neither temporary benchmark hook nor readback remains.

Run order dominated timing. In the first pair, shaderc add/trim medians were 127/126 us and native medians were 143/143 us. In the reverse pair, native medians were 126/125 us and shaderc medians were 127/126 us. The stable first-in-order result and swapped second-in-order penalty establish parity rather than a frontend regression. Debug-stripped artifacts contain 200 instructions/15,048 bytes from shaderc and 248/15,988 from native Slang.

## Occupancy-to-instance regeneration

`slang-surface-occupancy-to-flora-instances` replaces `shader/builder/surface/occupancy_to_flora_instances.comp`. A shared `flora_instance_placement.slang` module owns plantability, natural biome selection, Grass Mix fallback, and explicit paint selection for this entry and the remaining active-surface output pass. The entry preserves its fixed-array uniform/result layouts, two readonly `r32ui` storage images, write-only instance buffer, packed growth-potential buffer, storage atomics, and 8x8x8 workgroup.

Validation temporarily switched the authored-flora benchmark to Grass Mix and read back canonical per-species position/growth hashes after each edit. Four 25-step runs used order `GLSL, Slang, Slang, GLSL`; every hash, before/after count, and growing flag matched. Pass medians were 213 us GLSL and 219 us native in the first pair, then 208 us native and 211 us GLSL in the reverse pair. Run order is larger than the frontend delta.

A separate ten-step pair temporarily invoked growth-potential refresh after every add to exercise `preserve_existing_placements`. All 20 add/preserve hashes per frontend matched, and the preserve pass had a 211 us median through both frontends. All benchmark/readback hooks were removed. Debug-stripped artifacts contain 171 instructions/15,284 bytes from shaderc and 196/15,832 from native Slang.

## Active-surface flora generation

`slang-surface-active-to-flora-instances` replaces `shader/builder/surface/active_surface_to_flora_instances.comp` and completes the ten-entry surface family. It reuses the shared placement-selection, instance, growth-potential, voxel-data, surface-result, and active-brick buffer modules. The entry preserves seven descriptors across two sets, a five-element result array, readonly `r32ui` surface image, runtime buffers, storage atomics, active-brick/voxel decoding, and its 128x1x1 workgroup.

Initial loading normally suppresses procedural flora. Validation temporarily enabled it and read back canonical sorted position/growth hashes after each non-empty chunk; neither temporary change remains. Four runs used order `GLSL, Slang, Slang, GLSL`. Every species count and hash matched across all 16 non-empty chunk results. Example counts were `[8214, 19218, 213, 16, 0]` for `UVec3(0, 0, 0)` and `[8007, 18957, 221, 31, 0]` for `UVec3(1, 0, 1)`.

The combined eight timestamps per frontend had identical 46 us medians. Shaderc averaged 47.25 us and native Slang 47.38 us; both ranged from 44-53 us. Debug-stripped artifacts contain 140 instructions/13,852 bytes from shaderc and 148/13,968 from native Slang.

With all ten entries enabled, a final 25-step Grass Mix run matched every canonical hash and before/after/growing result from the default reference. A five-sample tree benchmark matched all workloads; the 28 heavy sparse extraction passes had 600 us shaderc and 601 us aggregate-native medians (`+0.17%`), with P95 differing by `+0.19%`. This closes surface construction at 10/10 native entries.

## Scene acceleration

`slang-scene-accel` replaces `shader/builder/scene_accel/update_scene_tex.comp` and completes the one-entry scene-acceleration family. It preserves the 24-byte update uniform, write-only `rg32ui` 3D storage image, 1x1x1 workgroup, zero invalid encoding, and one-based valid node/leaf offsets.

A five-sample tree benchmark matched every surface and contree workload while exercising both populated and cleared scene entries. Fixed-camera 2880x1620 captures remained visually equivalent; residual low-level differences were consistent with independently timed dynamic rendering rather than the exact integer scene-offset write. After debug stripping, both frontends emit 25 instructions; shaderc is 1,196 bytes and native Slang is 1,184 bytes.

## Temporal denoising

`slang-denoiser-temporal` replaces `shader/denoiser/temporal.comp`. Shared `packer.slang` now owns the missing normal and RGBE unpack operations. The entry preserves its two-float uniform, eleven combined-sampler/storage-image descriptors, 8x8x1 workgroup, bilinear motion reprojection, consistency rejection, history-length update, and temporal blend. Integer combined samplers use explicit `[[vk::image_format("unknown")]]` annotations so native Slang retains GLSL's sampled-image contract rather than inferring storage-only formats.

Matched six-second native-Vulkan runs retained 54 `denoiser.pass` samples per frontend after frame 120. GLSL measured 412.28 us mean/413 us median; native temporal measured 410.93 us mean/410 us median (`-0.73%`). Independent 2880x1620 fixed-camera captures had frontend RMSE `0.00565`, comparable to the `0.00549` RMSE between two GLSL captures, so no frontend-specific visual difference was measured. After debug stripping, shaderc emits 74 instructions/6,360 bytes and native Slang emits 78/6,548.

## Spatial denoising

`slang-denoiser-spatial` replaces `shader/denoiser/spatial.comp` and completes the denoiser pair. It preserves the four-byte iteration push constant, 32-byte spatial uniform, twelve resources over three descriptor sets, integer sampled-image contracts, `r32ui` and paired `r11f_g11f_b10f` storage images, 8x8x1 workgroup, disabled-pass copy path, à-trous ping-pong iteration, and color/normal/position/depth/voxel weighting.

Two six-second order-reversed pairs retained 52-55 post-startup samples per run. GLSL/native medians were 413/407 us in the first pair and 411/408 us in the reverse pair, placing native Slang 0.7-1.5% lower without a regression. With both denoiser entries native, the aggregate median was 410 us versus the matched 411 us GLSL reference (`-0.24%`).

The complete pair's fixed-camera RMSE against GLSL was `0.00219`, below the `0.00549` RMSE between two independent GLSL captures. Debug-stripped spatial artifacts contain 123 instructions/9,420 bytes from shaderc and 144/10,052 from native Slang.

## Shadow depth copy

`slang-tracer-shadow-depth-copy` replaces `shader/tracer/shadow_depth_copy.comp`. It preserves the combined depth sampler, write-only `r32f` storage image, 8x8x1 workgroup, bounds check, and exact raster-depth copy consumed by later VSM stages.

Matched six-second native-Vulkan runs retained 54 GLSL and 53 native post-startup samples. Both had 18 us medians; shaderc had one 184 us outlier while native ranged from 17-19 us. A fixed-camera capture had RMSE `0.00237` against GLSL, below the `0.00549` same-frontend repeat RMSE. Debug-stripped artifacts contain 20 instructions through both frontends, at 1,004 bytes for shaderc and 1,028 for native Slang.

## EVSM creation

`slang-tracer-vsm-creation` replaces `shader/tracer/vsm_creation.comp`. A shared `vsm.slang` module now owns EVSM exponents, depth warping, and Chebyshev bounds; existing tracer and flora shadow modules consume it instead of maintaining duplicate math. The entry preserves three storage-image descriptors, `r32f` depth input, `rgba32f` moment outputs, 8x8x1 workgroup, and four-moment conversion.

Matched six-second native-Vulkan runs retained 53 GLSL and 54 native post-startup `vsm_filtering.pass` samples. Medians were 345 and 345.5 us (`+0.14%`), with both ranges ending at 379 us. Fixed-camera RMSE was `0.00250`, below same-frontend repeat noise. Both stripped artifacts contain 21 instructions; shaderc is 1,252 bytes and native Slang 1,204 bytes.

## Horizontal EVSM filtering

`slang-tracer-vsm-blur-h` replaces `shader/tracer/vsm_blur_h.comp`. Shared `vsm_filtering.slang` owns the capped 64-texel radius, zero-radius behavior, Gaussian weighting, edge clipping, and normalization that the vertical pass will reuse. The entry preserves the 12-byte push constant, three storage images, 8x8x1 workgroup, and ping-to-pong write direction.

A matched six-second run retained 53 GLSL and 54 native post-startup `vsm_filtering.pass` samples. Medians were 345 and 342 us; native's range was 340-412 us versus 342-379 us for GLSL, so the result shows no regression rather than a speed claim. Fixed-camera RMSE was `0.00250`, below repeat noise. Stripped artifacts contain 51 instructions/2,516 bytes from shaderc and 57/2,704 from native Slang.

## Vertical and complete EVSM filtering

`slang-tracer-vsm-blur-v` replaces `shader/tracer/vsm_blur_v.comp`. It reuses `vsm_filtering.slang` for vertical Gaussian filtering and temporal-history blending while preserving four storage images, the 12-byte push constant, reset behavior, alpha clamping, ping/pong direction, and 8x8x1 workgroup. `slang-tracer-vsm` aggregates depth copy, moment creation, and both blur directions.

The isolated native vertical pass produced a 343 us `vsm_filtering.pass` median versus 345 us GLSL. With all four VSM entries native, the median was 341 us (`-1.16%`) and the fixed-camera RMSE was `0.00231`, below same-frontend repeat noise. Stripped vertical artifacts contain 60 instructions/2,960 bytes from shaderc and 66/3,180 from native Slang.

## Leaf-shadow temporal accumulation

`slang-tracer-leaf-shadow-temporal` replaces `shader/tracer/leaf_shadow_temporal.comp`. It preserves two combined opacity samplers, the write-only `rgba8` blended map, 8-byte alpha/reset push constant, 8x8x1 workgroup, and independent depth-times-opacity and opacity blending.

Matched six-second native-Vulkan runs retained 49 GLSL and 48 native post-startup samples. Both had 90 us medians and identical 89-90 us ranges. Stripped artifacts contain 32 instructions/1,792 bytes from shaderc and 29/1,692 from native Slang.

## Leaf-shadow influence mask

`slang-tracer-leaf-shadow-mask` replaces `shader/tracer/leaf_shadow_mask.comp` and completes the two-pass leaf-shadow compute subfamily. It preserves the combined opacity sampler, write-only `rgba8` mask, 8x8x1 workgroup, 3x3 neighboring-cell dilation, four subcell samples per neighbor, and conservative opacity threshold. `slang-tracer-leaf-shadow` aggregates mask and temporal accumulation.

The isolated mask run had identical 62 us medians through both frontends. With both entries native, temporal remained at 90 us and mask at 62 us; mean deltas were under 0.8%. Stripped mask artifacts contain 39 instructions/2,244 bytes from shaderc and 48/2,464 from native Slang.

## Lens-flare downsample

`slang-tracer-lens-flare-downsample` replaces `shader/tracer/lens_flare_downsample.comp`. It preserves the read-only and write-only `r11f_g11f_b10f` storage images, 8x8x1 workgroup, source-to-destination footprint calculation, clamping, and box average.

Same-extent native-Vulkan runs produced 7 us GLSL and 6 us native medians. The fixed-camera frontend RMSE was `0.00876`, effectively matching native repeat variation (`0.00872`); the residual is concentrated in animated authored flora. Stripped artifacts contain 31 instructions/2,116 bytes from shaderc and 36/2,352 from native Slang.

## Lens-flare sun visibility

`slang-tracer-lens-flare-sun-visible` replaces `shader/tracer/lens_flare_sun_visible.comp`. It reuses native camera-ray and tracer uniform modules while preserving the two uniform buffers, sampled graphics depth/sun sprite, read-only `r32f` compute depth, `r32ui` atomic visibility counter, and 8x8x1 workgroup.

Same-extent native-Vulkan runs produced 7 us medians through both frontends. Fixed-camera RMSE was `0.00870`, below the measured repeat envelope (`0.00872`). Explicit LOD-zero sprite sampling avoids requesting unsupported compute-derivative capabilities. Stripped artifacts contain 77 instructions/4,348 bytes from shaderc and 76/5,560 from native Slang.

## Lens-flare generation

`slang-tracer-lens-flare-generate` replaces `shader/tracer/lens_flare.comp` and adds the scalar Murmur hash variant to the shared native hash module. It preserves GUI/sun/camera buffers, the read-only `r32ui` visibility count, write-only `r11f_g11f_b10f` full-resolution target, procedural flare pattern, visibility normalization, edge attenuation, and 8x8x1 workgroup. `slang-tracer-lens-flare` aggregates all three entries.

At a fixed sun-facing time of day, isolated generation medians were 13 us through both frontends. With all three entries native, medians were 9 us visibility, 13 us generation, and 6 us downsample versus 8, 13, and 7 us GLSL. Aggregate fixed-camera RMSE was `0.00929`, below same-frontend repeat variation (`0.00937`). Stripped generation artifacts contain 152 instructions/10,952 bytes from shaderc and 142/11,400 from native Slang.

## Wind volume

`slang-tracer-wind-volume` replaces `shader/tracer/wind_volume.comp`; `wind_volume_sample.slang` owns the source buffer, seeded FBM, directional bias, turbulence, and source accumulation. The entry preserves the wind-volume uniform, write-only `rg16f` image, GUI/source bindings, 8-byte time/bucket push constant, four-way x bucketing, and 4x4x4 workgroup.

A forced rapid-tick run retained 49 GLSL and 38 native active-dispatch samples after filtering no-dispatch scope markers; both had 7 us medians. Fixed-camera RMSE was `0.00621`, below native repeat variation (`0.00641`). Stripped artifacts contain 136 instructions/8,964 bytes from shaderc and 146/9,304 from native Slang.

## Terrain queries

`slang-tracer-terrain-query` replaces `shader/tracer/terrain_query.comp` and directly reuses `scene_marching.slang`, `contree_marching.slang`, and the native ray/result types. It preserves the query-count uniform, input/output runtime-array buffers, contree buffers, read-only `rg32ui` scene image, zero-direction guard, and 64x1x1 workgroup.

The dormant startup validation hook was enabled temporarily for four reference rays, then restored before commit. Both frontends exactly matched CPU hit positions at `(0.500, 0.426, 0.500)` and `(1.500, 0.547, 1.500)` with zero reported delta, and both missed the two out-of-scene rays. Synchronized per-query wall medians were about 347 us through both frontends. Stripped artifacts contain 175 instructions/12,108 bytes from shaderc and 192/12,528 from native Slang.

## God rays

`slang-tracer-god-ray` replaces `shader/tracer/god_ray.comp`; `noise_tex.slang` centralizes the blue-noise dimensions, seed calculation, and typed samples previously embedded in tracer shadowing. The entry preserves camera/shadow/environment/god-ray uniforms, raster/compute depth, shadow map, scalar blue noise, write-only `r32f` output, depth-limited integration, and 8x8x1 workgroup. Shadow sampling uses explicit LOD zero to avoid unsupported compute-derivative capabilities.

Order-reversed same-extent native-Vulkan runs produced 47 us GLSL and 49 us native medians (`+4.3%`, `+2 us`); frame impact is negligible. Fixed-camera RMSE was `0.00368`, below the GLSL repeat envelope (`0.00390`). Stripped artifacts contain 93 instructions/4,932 bytes from shaderc and 75/5,896 from native Slang.

## Cloud-shadow temporal resolve

`slang-tracer-cloud-shadow-temporal` replaces `shader/tracer/cloud_shadow_temporal.comp`. It preserves the GUI uniform, raw/history combined samplers, write-only `r16f` transmittance output, 4-byte reset push constant, current-neighborhood envelope, adaptive temporal weight, fast clear path, and 8x8x1 workgroup.

The runtime cloud flag was temporarily re-enabled for A/B validation and restored before commit. Grouped `cloud_shadow.pass` medians remained 9 us through both frontends. Fixed-camera frontend RMSE was `0.00870`, effectively matching native repeat variation (`0.00864`, a `0.00006` difference). Stripped artifacts contain 144 instructions/6,220 bytes from shaderc and 158/6,700 from native Slang.

## Visible-cloud temporal resolve

`slang-tracer-cloud-temporal` replaces `shader/tracer/cloud_temporal.comp`. It preserves GUI/current/previous-camera buffers, raw/history samplers, write-only `rgba16f` output, 4-byte reset push constant, slab representative-point reprojection, 3x3 current envelope, adaptive disocclusion weighting, and 8x8x1 workgroup. History sampling uses explicit LOD zero to avoid unsupported compute-derivative capabilities.

Both the dormant render flag and cloud uniform enables were temporarily restored, producing visibly nonzero clouds, then reverted before commit. Grouped `cloud.pass` medians were 132.5 us GLSL and 133 us native (`+0.4%`). Fixed-camera RMSE was `0.00290`, below native repeat variation (`0.00917`). Stripped artifacts contain 197 instructions/8,968 bytes from shaderc and 186/10,112 from native Slang.

## Visible-cloud generation

`slang-tracer-cloud` replaces `shader/tracer/cloud.comp` and delegates density, lighting, and ray integration to the previously accepted `clouds.slang` module. It preserves GUI/sun/camera/environment buffers, scalar blue noise, write-only `rgba16f` raw output, animated jitter, and 8x8x1 workgroup.

With the dormant cloud controls temporarily re-enabled, both paths rendered visibly nonzero cloud cover. Grouped `cloud.pass` medians improved from 132.5 us GLSL to 130 us native (`-1.9%`). Fixed-camera RMSE was `0.00374`, below native repeat variation (`0.00905`). Stripped artifacts are nearly identical: 310 instructions/52,324 bytes from shaderc and 314/52,344 from native Slang.

## Cloud-shadow generation

`slang-tracer-cloud-shadow` replaces `shader/tracer/cloud_shadow.comp` and completes all 21 tracer entries. It reuses public layer-boundary and low-quality density functions from `clouds.slang`, plus native ray and blue-noise helpers. The entry preserves GUI/sun/shadow-camera/environment uniforms, scalar noise, write-only `r16f` output, slab relocation, bounded Beer integration, horizon fade, and 8x8x1 workgroup. `slang-tracer-clouds` aggregates all four cloud entries.

With real cloud rendering and uniforms temporarily re-enabled, isolated cloud-shadow medians improved from 32 us GLSL to 31 us native (`-3.1%`). The complete native cloud aggregate retained 31 us shadow and 131 us visible-cloud medians versus 32 and 132.5 us GLSL. Aggregate fixed-camera RMSE was `0.00891`, below native repeat variation (`0.00922`). Stripped shadow artifacts contain 192 instructions/18,824 bytes from shaderc and 188/19,216 from native Slang.

## Chunk-writer region dispatch setup

`slang-chunk-writer-buffer-setup` replaces `shader/builder/chunk_writer/buffer_setup.comp` and starts the chunk-writer family. `chunk_writer_types.slang` owns the shared region uniform and indirect-dispatch layout. The entry preserves the 1x1x1 workgroup, 32-byte `U_RegionInfo`, 12-byte write-only `B_RegionIndirect`, and independent four-wide divide-round-up for all axes.

Five matched release tree workloads retained exact compile counts, region offsets/dimensions, and voxel counts through downstream indirect dispatches, from `UVec3(60, 89, 97)` / 517,980 voxels through `UVec3(172, 247, 280)` / 11,895,520 voxels. Stripped artifacts contain 25 instructions/1,180 bytes from shaderc and 24/996 from native Slang.

## Chunk terrain heightmap

`slang-chunk-writer-heightmap` replaces `chunk_heightmap.comp`. `gradient_noise.slang` now publicly exposes the amplitude-normalized seeded FBM helper. The entry preserves the 32-byte region uniform, runtime two-float heightmap, large/medium/fine terrain frequencies, retained coastline helpers, identical surface/base output behavior, and 8x8x1 workgroups.

With GLSL classification downstream, a full 256³ startup readback exactly matched all 16 voxel-type counts and atlas FNV hash `997d9aba9cc5831f`. Stripped artifacts contain 57 instructions/18,312 bytes from shaderc and 77/18,560 from native Slang.

## Chunk atlas initialization

`slang-chunk-writer-init` replaces `chunk_init.comp`. `chunk_writer_types.slang` now owns the two-float heightmap entry and runtime buffer. The entry preserves the 32-byte region uniform, read-only heightmap, write-only `r8ui` atlas, deterministic topsoil/transition/rock classification, cleared moisture/fertility initialization, dirty solid-workgroup marking, and 4x4x4 workgroups.

Full 256³ startup A/B readbacks exactly matched all 16 voxel-type counts and atlas FNV hash `997d9aba9cc5831f`. Stripped artifacts contain 57 instructions/3,652 bytes from shaderc and 60/3,640 from native Slang.

## Model voxelization

`slang-chunk-writer-model-voxelize` replaces `model_voxelize.comp`. `chunk_writer_types.slang` owns the 48-byte model uniform and runtime triangle stream. The entry preserves per-voxel winding solid-angle accumulation, all point-triangle closest-region tests, configurable surface shell, atlas-state initialization, dirty solid-workgroup marking, and 8x8x8 workgroups.

A/B voxelization of the same closed four-triangle tetrahedron exactly matched 1,016 filled voxels and regional FNV hash `d2f1c029626a874d`. Stripped artifacts contain 99 instructions/6,820 bytes from shaderc and 145/7,864 from native Slang.

## Chunk modification

`slang-chunk-writer-modify` replaces `chunk_modify.comp`. `chunk_writer_types.slang` now owns the 80-byte modify uniform plus BVH, round-cone, cuboid, and sphere layouts. The entry preserves nine bindings, fixed-stack BVH traversal, all three primitive tests, surface-only removal/placement, target and per-type/write limits, atomic reservation rollback, removal-candidate output, soil-state-aware fill, dirty solid-workgroup marking, and 8x8x8 workgroups.

A/B runs covering startup round-cone construction plus explicit cuboid and surface-sphere paths exactly matched 204 removed dirt voxels, 204 inserted empty voxels, and full-atlas FNV hash `ff2c473cd7fffe6b`. Stripped artifacts contain 293 instructions/13,316 bytes from shaderc and 334/15,148 from native Slang.

## Chunk modification removal sampling

`slang-chunk-writer-modify-sample` replaces `chunk_modify_sample.comp`. It preserves the 4-byte edit-seed push constant, descriptors at bindings 6-8, 50-invocation workgroup, fixed 50-position output, sample-count clamping and zero-fill, and Murmur-hashed selection from the atomically appended removal candidates. `chunk_writer_types.slang` now owns the edit-stat and removal-sample layouts, and `hash.slang` publicly exposes the existing integer `murmurHash12` implementation.

Matched surface-removal runs both reported 187 removed dirt voxels, 187 inserted empty voxels, and 50 valid sampled positions within the brush. The exact sample sequence is not a stable oracle because the producer's candidate list is appended by unordered atomics. Stripped artifacts contain 47 instructions/2,152 bytes from shaderc and 55/2,528 from native Slang.

## Chunk solidity sampling

`slang-chunk-writer-solid-sample` replaces `chunk_solid_sample.comp`. It preserves the 48-byte three-`uvec3` uniform, read-only `r8ui` storage image, runtime uint output, source-block center mapping, atlas bounds handling, flattened sample indexing, and 8x8x8 workgroup. `chunk_writer_types.slang` owns the shared sampling uniform and output layouts.

A full-atlas 256³ to 32³ A/B readback exactly matched all 32,768 values, 13,790 solid samples, and FNV hash `f19bf9e9d5dcbc7b`. Stripped artifacts contain 40 instructions/2,092 bytes from shaderc and 42/2,348 from native Slang.

## Voxel property sampling

`slang-chunk-writer-voxel-property-sample` replaces `voxel_property_sample.comp`. It preserves the 64-byte query uniform, 32-byte result, read-only `r8ui` atlas, spherical and target-mask gating, moisture/fertility extraction, two groupshared counters, barriers, one global reduction per 8x8x8 workgroup, and atomic result accumulation.

A/B queries over the same terrain sphere exactly matched moisture count/sum 4,447/0 and fertility count/sum 4,447/4,447. Stripped artifacts contain 65 instructions/3,248 bytes from shaderc and 70/3,280 from native Slang.

## Legacy terrain height extraction

`slang-chunk-writer-terrain-smooth-heights` replaces `terrain_smooth_heights.comp`. `terrain_smooth.slang` owns the legacy three-pass smoother's 64-byte uniform, runtime column/result layouts, terrain classification, and flattened indexing. The first entry preserves the read-only `r8ui` atlas, top-down terrain search, initial height/target/valid tuple, and 8x8x1 workgroup.

A temporarily restored complete legacy smoothing dispatch exactly matched GLSL's 1,457 changed voxels on a nontrivial brush. Stripped artifacts contain 50 instructions/2,472 bytes from shaderc and 63/2,756 from native Slang.

## Legacy terrain smoothing target

`slang-chunk-writer-terrain-smooth-target` replaces `terrain_smooth_target.comp`. It preserves the shared 64-byte uniform and column buffer, valid/brush gating, bounded two-dimensional Gaussian kernel, target blend and brush falloff, maximum delta and deadband, and 8x8x1 workgroup.

The restored complete legacy pipeline again exactly matched GLSL's 1,457 changed voxels. Stripped artifacts contain 77 instructions/4,848 bytes from shaderc and 91/4,988 from native Slang.

## Legacy terrain smoothing application

`slang-chunk-writer-terrain-smooth-apply` completes the legacy three-pass pipeline; `slang-chunk-writer-terrain-smooth` enables all three entries. The apply pass preserves five bindings, brush/deadband and height-range gating, raise/lower material rules, atlas state clearing/preservation, dirty solid-workgroup marking, changed-count atomics, and the 8x8x8 workgroup.

Both isolated-apply and complete-native restored-pipeline runs exactly matched GLSL's 1,457 changed voxels. Stripped apply artifacts contain 88 instructions/5,516 bytes from shaderc and 125/6,108 from native Slang.

## Terrain moisture brushing

`slang-chunk-writer-terrain-moisture-brush` replaces `terrain_moisture_brush.comp`. `terrain_soil.slang` owns the 64-byte brush push constant plus shared soil classification, atlas lookup, stroke distance, dither hash, and pair-axis helpers. The entry preserves swept-sphere and directional-pair paths, five-step near-surface absorption, two-scale noisy falloff, stochastic two-bit moisture quantization, state-preserving atlas writes, and 8x8x8 workgroups.

A/B brush dispatches followed by native property readback exactly matched 7,778 sampled soil voxels, moisture sum 1,101, and average 0.1415531. Stripped artifacts contain 93 instructions/7,200 bytes from shaderc and 111/7,536 from native Slang.

## Terrain fertility brushing

`slang-chunk-writer-terrain-fertility-brush` replaces `terrain_fertility_brush.comp`. It preserves the shared 64-byte brush push constant, swept-sphere distance, exposed dirt/sand test, six-neighbor seeded local-maximum granules, center-to-edge coverage, two-level fertility boost, state-preserving atlas writes, and 8x8x8 workgroups.

A/B brush dispatches followed by native property readback exactly matched 12,440 sampled soil voxels, fertility sum 12,480, and average 1.0032154. Stripped artifacts contain 92 instructions/6,440 bytes from shaderc and 108/6,780 from native Slang.

## Terrain moisture spread

`slang-chunk-writer-terrain-moisture-spread` replaces `terrain_moisture_spread.comp`. It preserves the 64-byte spread push constant, axis/parity non-overlapping pair partition, dirt/sand and two-level-gradient gating, saturation-based mobility, gravity/capillary vertical bias, seeded transfer probability, conservative two-voxel moisture exchange, and 8x8x8 workgroups.

After a matched watering brush, six phased spread dispatches over the same 40x48x40 region exactly retained moisture sum 1,101 and matched atlas FNV hash `06e50bc4c49179d7`. Stripped artifacts contain 77 instructions/5,108 bytes from shaderc and 87/5,172 from native Slang.

## Terrain soil mixing

`slang-chunk-writer-terrain-soil-mix` replaces `terrain_soil_mix.comp`. It preserves the shared 64-byte brush push constant, six axis/parity phases, swept-sphere pair falloff, dirt/sand gating, independent moisture and fertility transfer rolls, conservative two-voxel state exchange, and 8x8x8 workgroups.

After matched seeded moisture/fertility brushes, the six-phase tiller dispatch exactly retained soil-state sum 138,601 and matched atlas FNV hash `c72cd9f2f9ebe857`. Stripped artifacts contain 124 instructions/8,224 bytes from shaderc and 142/8,384 from native Slang.

## Terrain moisture drying

`slang-chunk-writer-terrain-moisture-dry` completes the chunk-writer family; `slang-chunk-writer` enables all 21 entries. The final pass preserves the 80-byte push constant, ten descriptors, compact surface-leaf lookup, two-bit moisture/residual rules, packed surface normals, terrain ray fallback, VSM/leaf/cloud exposure, state-preserving atlas writes, and 64-thread workgroups. `tracer_shadowing.slang` now exposes a VSM/leaf/cloud-only direct-shadow helper so this pass shares the authoritative shadow math without binding unused PCSS resources.

A forced no-shadow drying pass after matched watering exactly reduced the regional moisture sum to 740 and matched FNV hash `3ae07d53cd0b5f87`; the complete-native 21-entry aggregate produced the same result. Stripped artifacts contain 351 instructions/18,592 bytes from shaderc and 383/20,652 from native Slang.

## MBO terrain density initialization

`slang-chunk-writer-terrain-smooth-mbo-init` replaces `terrain_smooth_mbo_init.comp`. `terrain_smooth_mbo.slang` ports the shared indexing, atlas classification, brush, scoring, hashing, and fill-material helpers for the remaining MBO entries. The entry preserves the 80-byte uniform, two runtime float buffers, read-only `r8ui` atlas, and 8x8x8 workgroup.

A temporary startup smoothing call was restored after validation. Both frontends produced a 68³ workload with 35,860 candidates, 18,482 target solids, threshold bin 518, tie count 1/1, four additions, four removals, zero volume delta, and identical changed bounds `UVec3(120, 108, 122)..UVec3(136, 109, 133)`. Stripped artifacts contain 38 instructions/2,000 bytes from shaderc and 49/2,204 from native Slang.

## MBO terrain A-to-B diffusion

`slang-chunk-writer-terrain-smooth-mbo-diffuse-ab` replaces the first ping-pong diffusion entry. It preserves the 80-byte MBO uniform, read-only A and write-only B runtime arrays, read-only `r8ui` atlas, brush/mutability gating, six-neighbor sampling with center fallback, and 8x8x8 workgroup.

The matched 68³ startup smoothing workload retained all deterministic outputs from the GLSL run: 35,860 candidates, 18,482 target solids, threshold 518, eight changes, zero volume delta, and identical bounds. Stripped artifacts contain 109 instructions/6,492 bytes from shaderc and 126/6,756 from native Slang.

## MBO terrain B-to-A diffusion

`slang-chunk-writer-terrain-smooth-mbo-diffuse-ba` replaces the mirrored ping-pong diffusion entry, swapping the read-only and write-only density buffers while retaining the same uniform, atlas, edge fallback, brush/mutability gating, six-neighbor kernel, and 8x8x8 workgroup.

The same 68³ end-to-end smoothing workload exactly matched GLSL's 35,860 candidates, 18,482 target solids, threshold/tie values, additions/removals, zero volume delta, and changed bounds. Stripped artifacts match the A-to-B diagnostics: 109 instructions/6,492 bytes from shaderc and 126/6,756 from native Slang.

## MBO terrain scoring

`slang-chunk-writer-terrain-smooth-mbo-score` replaces the candidate-score and histogram pass. It preserves the 80-byte uniform, diffused-density input, score/histogram/result runtime buffers, read-only `r8ui` atlas, brush/deadband logic, three atomic accumulations, and 8x8x8 workgroup.

The matched 68³ smoothing run exactly retained 35,860 candidate and 18,482 solid counts, threshold bin 518, tie 1/1, four additions/removals, zero volume delta, and changed bounds. Stripped artifacts contain 72 instructions/4,460 bytes from shaderc and 107/5,248 from native Slang.

## MBO terrain application

`slang-chunk-writer-terrain-smooth-mbo-apply` completes the MBO pipeline; `slang-chunk-writer-terrain-smooth-mbo` enables all five entries. The apply pass preserves score/threshold/tie decisions, neighboring fill-material selection, read-write `r8ui` atlas updates, atomic change counts and bounds, solid-workgroup bit marking, and 8x8x8 workgroups. `voxel_data.slang` now owns all atlas state-preserving/clearing pack helpers needed by the remaining chunk-writer entries.

Both isolated and complete-native 68³ runs exactly matched GLSL's 35,860 candidates, 18,482 target solids, threshold 518, tie 1/1, four additions, four removals, zero volume delta, and changed bounds. Stripped apply artifacts contain 117 instructions/8,496 bytes from shaderc and 154/9,244 from native Slang.

## Terrarium glass vertex path

`slang-terrarium-glass-vert` replaces `shader/terrarium/glass.vert`. It preserves five vertex attributes, eight varyings with flat face/side/alpha/part fields, the 32-byte box/alpha push constant, camera projection, normalized world normals, near-side face classification, and world-space view direction.

Fixed-preset screenshot RMSE was 0.007116 versus 0.006819 same-GLSL repeat variation, with visual inspection showing equivalent terrarium geometry and glass layering. Stripped artifacts contain 61 instructions/2,660 bytes from shaderc and 47/2,776 from native Slang.

## Surface-flora LOD vertex path

`slang-foliage-flora-lod` completes all 76 production entries; `slang-foliage` enables the full seven-entry foliage family. The LOD entry reuses the accepted surface-flora instance, competition/moisture growth, spawn/wind, palette, shadow, lighting, and preview logic with the camera-facing `sqrt(3/2)` area-matched billboard.

LOD-only and complete-family screenshot RMSE were 0.008674 and 0.006726 versus 0.006649 same-GLSL repeat variation, with visual equivalence. Debug-stripped artifacts contain 590 instructions/54,320 bytes from shaderc and 617/55,892 from native Slang.

## Leaf LOD vertex path

`slang-foliage-leaves-lod` ports the billboard LOD path for tree leaves and fruit. It reuses the accepted shared leaf instance, mature growth, wind, color, shadow, lighting, and preview logic, replacing cube corners with the camera-facing `sqrt(3/2)` area-matched billboard.

Fixed-preset screenshot RMSE was 0.009184 versus 0.008430 same-GLSL repeat variation, with visual equivalence in the dynamic tree scene. Debug-stripped artifacts contain 520 instructions/50,380 bytes from shaderc and 535/51,488 from native Slang.

## Full-resolution leaf vertex path

`slang-foliage-leaves-vert` ports full-resolution tree-leaf and fruit voxel rendering. Tree-leaf instance reconstruction, mature/inactive growth inputs, wind preparation, exact voxel corners, hashed depth offset, palette, stylized lighting, and edit-preview tint pass. To keep the three remaining main foliage entries DRY, shared lookup/growth/wind/color/lighting/projection logic was extracted from `flora.vert.slang` into `flora_vertex.slang`; surface-only competition and moisture resources live in `surface_flora_vertex.slang`. The existing `slang-flora` feature retains its automatic ABI match after the refactor.

Fixed-preset screenshot RMSE was 0.008667 versus 0.008127 same-GLSL repeat variation, with visual equivalence. Debug-stripped artifacts contain 520 instructions/50,280 bytes from shaderc and 535/51,840 from native Slang.

## Leaf-shadow vertex path

`slang-foliage-leaves-shadow-vert` completes the pair; `slang-foliage-leaves-shadow` enables both stages. The entry preserves packed corner and tree-leaf instance decoding, tree/fruit voxel lookup, wind-volume sampling, apple swing and leaf paddling, shadow-camera billboard geometry, configurable opacity, and the stable palette-seed depth tie-break. Shared tree-leaf instance types and decoding now live in `flora_types.slang`.

Vertex-only and complete-pair screenshot RMSE were 0.008853 and 0.008955 versus a 0.006925 same-GLSL repeat in the dynamic foliage scene; visual inspection showed equivalent geometry and shadowing. Debug-stripped vertex artifacts contain 254 instructions/18,132 bytes from shaderc and 250/18,776 from native Slang.

## Leaf-shadow fragment path

`slang-foliage-leaves-shadow-frag` ports the premultiplied leaf-shadow opacity/depth fragment entry. Its opacity varying, fragment-coordinate depth clamp, depth-premultiplied red channel, and accumulated alpha output pass the automatic interface gate.

Fixed-preset screenshot RMSE was 0.009281 versus 0.009179 same-GLSL repeat variation, with visual equivalence across moving foliage. Debug-stripped artifacts each contain 10 instructions; shaderc emits 544 bytes and native Slang 512 bytes.

## Textured-particle vertex path

`slang-particles-lod-textured-vert` completes the pair; `slang-particles-lod-textured` enables both stages and `slang-particles` the full three-entry family. The entry preserves five inputs, packed voxel corner decoding, camera-facing billboard geometry, per-instance hashed depth offset, VSM/leaf/cloud shadowing, directional voxel shading, ambient and sun light, sprite flip/index packing, and three outputs.

Vertex-only and complete-family screenshot RMSE were 0.006424 and 0.006351 versus 0.007198 same-GLSL repeat variation. Debug-stripped vertex artifacts contain 230 instructions/11,612 bytes from shaderc and 217/13,016 from native Slang.

## Textured-particle fragment path

`slang-particles-lod-textured-frag` ports the discard-free alpha-tested textured-particle fragment entry. The exact three-varying interface, flat texture-array index, combined sampler, 0.5 alpha mask, premultiplied blend output, and masked far-depth write pass the automatic ABI gate.

Fixed-preset screenshot RMSE was 0.005994, comparable to 0.005939 same-GLSL repeat variation, with visual equivalence. Debug-stripped artifacts each contain 18 instructions; shaderc emits 1,316 bytes and native Slang 1,144 bytes.

## Water-droplet fragment path

`slang-particles-water-droplet` ports the premultiplied water-droplet sprite fragment entry. The exact three-varying interface, flat texture-array index, combined sampled-image binding, color modulation, and `ONE / ONE_MINUS_SRC_ALPHA` output contract pass the automatic ABI gate.

A matched 35,000-particle fixed-preset screenshot produced RMSE 0.007693 versus 0.009458 same-GLSL repeat variation. Debug-stripped artifacts each contain 14 instructions; shaderc emits 1,040 bytes and native Slang 860 bytes.

## Terrarium glass fragment path

`slang-terrarium-glass-frag` completes the pair; `slang-terrarium-glass` enables both stages. The fragment entry preserves the optimized six-varying interface and flat qualifiers, UV/core/corner edge classification, two-sided Fresnel, stylized sky and sun reflection, part-specific premultiplied alpha, spectral edge tint, and long top-rim glint. Two otherwise-unused producer varyings are retained through impossible finite-geometry guards to match the GLSL interface contract without affecting valid fragments.

Fragment-only and complete-native pair screenshot RMSE were 0.003862 and 0.006825 versus 0.007654 same-GLSL repeat variation. Stripped fragment artifacts contain 49 instructions/6,492 bytes from shaderc and 62/6,884 from native Slang.

## Sprinkler rendering

`slang-sprinkler` replaces `shader/props/sprinkler.vert`. It preserves seven vertex attributes, eleven sparse set-0 descriptors, tick/phase-driven alternating X/Z arm extension, camera projection, shared stylized VSM/leaf/cloud lighting, sRGB conversion, terrain-edit preview tint, and the existing fragment-stage interface.

A temporarily placed center sprinkler rendered cleanly through both frontends. Fixed-preset screenshot RMSE was 0.006983 versus 0.006757 same-GLSL repeat variation, with visual inspection showing equivalent geometry, animation pose, lighting, and shadows. Stripped artifacts contain 278 instructions/13,240 bytes from shaderc and 268/13,820 from native Slang.

## Contree leaf construction

The third candidate replaces `shader/builder/contree/leaf_write.comp` under `slang-contree-leaf`; `slang-contree` is the aggregate contree feature. This pass exercises a shared workgroup prefix allocation, three synchronized barriers, a global atomic leaf allocator, a 64-element per-invocation temporary array, a structured `ContreeNode` runtime array, and ten descriptor bindings.

The port generates nearly identical optimized SPIR-V after debug stripping: 411 instructions from GLSL and 410 from Slang. Explicit storage qualifiers preserve `NonWritable` and `NonReadable` decorations.

Two order-reversed 50-sample hidden tree-benchmark pairs retained 208 heavy contree builds per frontend after filtering at 500,000 leaf bytes:

| Run order | Mean delta | Median delta | P95 delta |
| --- | ---: | ---: | ---: |
| GLSL then Slang | -9.45% | +2.17% | -10.37% |
| Slang then GLSL | +0.18% | -2.33% | +1.35% |

The pass takes only about 42-47 us on this machine, so its timestamp is quantized to one-microsecond steps. The combined median was exactly 44 us for both frontends; the large percentage changes in the first pair came from rare multi-millisecond OS/GPU stalls. There is no measured typical-time regression.

All 307 matched contree workloads had identical chunk, node-byte, and leaf-byte results. A 5120x2880 screenshot was identical outside 24 pixels of dynamic performance text in the bottom-right UI. Baseline pass breakdown also confirmed that `leaf_write` is the dominant contree shader: its stable median was approximately 43 us, compared with 20 us, 14 us, and 8 us for the three `tree_write` levels.

Reproduce the comparison with the same hidden tree-benchmark command used for the surface test, substituting `--features slang-contree-leaf`, then run:

```bash
scripts/compare_contree_pass.py <glsl-log> <slang-log> --pass leaf_write
```

## Contree buffer setup

Phase 3 begins with `shader/builder/contree/buffer_setup.comp` under the independent `slang-contree-buffer-setup` feature. The native entry preserves the seven descriptors and 1x1x1 workgroup while initializing build dimensions, level counters and offsets, indirect leaf dispatch, and result lengths. A shared `contree_build.slang` module now owns layouts used by setup and leaf construction.

The automated GLSL-reference ABI gate, Vulkan 1.3 SPIR-V validation, and release pipeline creation pass. A matched five-sample hidden tree-benchmark pair produced the same 28 contree workloads, including chunk identities and node/leaf byte counts. Both frontends had a 9 us median for this short initialization pass; timing is recorded only as a regression guard, not meaningful performance evidence.

`shader/builder/contree/buffer_update.comp` is independently selectable with `slang-contree-buffer-update`. It reuses the shared layouts and power-of-four dimension helpers, preserving its two descriptors and 1x1x1 workgroup while advancing build state and the next indirect 4x4x4 dispatch. Another matched 28-workload comparison had identical node/leaf results and a 2 us median for both frontends.

`slang-contree-last-buffer-update` ports the five-binding final state pass. It copies the root sparse node, accounts for that level, and creates the concat dispatch exactly as the GLSL source does. Its matched 28-workload comparison also preserved every node/leaf result and had a 2 us median for both frontends.

`slang-contree-tree-write` ports intermediate-level compaction: six descriptors, a 4x4x4 workgroup, a shared prefix allocation, two global atomics, and a 64-node per-invocation temporary array. Both GLSL and Slang now keep inactive edge invocations active until all workgroup barriers complete. The three measured levels matched all 28 workloads. GLSL/native medians were 20/20 us, 14/13 us, and 8/7 us; outlier-sensitive means and tails were not used as evidence for these sub-20-us passes.

`slang-contree-concat` completes the family by converting dense per-level data into final absolute child offsets. It preserves six descriptors, the 256x1x1 workgroup, and the fixed ten-level upper-bound array. Its isolated matched median was 3 us for both frontends. With all six entries selected by `slang-contree`, the same 28 workloads matched and the full contree pass median was 107.5 us versus 112.5 us for GLSL (`-4.4%`). `scripts/compare_contree_pass.py <glsl-log> <slang-log> --pass pass_total` reproduces the aggregate comparison.

## Native Slang traversal modules

The `slang-tracer-shadow` feature replaces production `shader/tracer/tracer_shadow.comp` with a native Slang entry point. Its reusable modules cover AABB intersection, camera-ray projection, contree traversal, DDA scene traversal, marching results, and voxel decoding. This exercises the branch-heavy core shared conceptually with the main tracer without relying on `-allow-glsl`.

The native path preserves all five descriptor bindings, the 400-byte camera uniform, the 64-invocation workgroup stack, std430 contree buffers, and read/write storage image formats. A 5120x2880 fixed-camera comparison showed equivalent terrain and object shadows; normal floating-point, moving-object, and UI differences prevented byte identity.

Two order-reversed local MoltenVK timing pairs measured `tracer_shadow.pass`:

| Run order | Mean delta | Median delta |
| --- | ---: | ---: |
| GLSL then native Slang | -1.94% | -1.37% |
| Native Slang then GLSL | +1.47% | -0.78% |

Typical GPU time is at parity on the tested Apple M4 Pro. The debug-stripped optimized artifact contains 736 instructions/12,796 bytes from shaderc and 815 instructions/14,168 bytes from Slang. Native Vulkan performance remains a TODO for when Windows/Linux hardware is available.

## Native Slang graphics-stage pair

The `slang-egui` feature replaces both `shader/egui/egui.vert` and `shader/egui/egui.frag`. It validates a native Slang vertex/fragment interface with three explicitly located vertex attributes, two interpolants, `SV_Position`, a 64-byte matrix push constant, a Vulkan combined `Sampler2D`, fragment output, and the existing alpha-blended graphics pipeline.

Both stages pass `spirv-val`, their reflected locations and resource contracts match GLSL, and the merged Vulkan graphics pipeline runs successfully through MoltenVK. A 2560x1440 screenshot rendered all UI geometry, text, textures, clipping, and blending correctly. Stable UI regions were visually equivalent, with only sparse 1-3-level 8-bit color differences. The optimized vertex modules contain 94 GLSL versus 103 Slang instructions; the fragment modules contain 35 GLSL versus 34 Slang instructions.

## Complex native flora graphics pair

The `slang-flora` feature replaces `shader/foliage/flora.vert` and `shader/foliage/flora.frag`. The native entry keeps the 18 descriptors across two sets together while focused type, motion, noise/color, and shadow modules cover the transitive GLSL behavior. This validates a 128-byte push constant with two 12-element color arrays, a five-element uniform array, a seven-element storage-buffer table, runtime storage arrays, an `r8ui` 3D storage image, 2D/3D combined samplers, camera matrices, wind, direct-sun/leaf/cloud shadows, depth offset, and smooth vertex-to-fragment color interpolation.

The vertex entry uses `SV_VulkanInstanceID`, not HLSL's relative `SV_InstanceID`: the renderer supplies a nonzero first instance to select each species' region of the shared instance buffer, so matching GLSL `gl_InstanceIndex` requires raw Vulkan `InstanceIndex`. The ABI check also exposed Slang's `_Array_*` wrapper structs. Build-time reflection now recovers fixed dimensions and strides from the wrapper's nested `data` member, while runtime layout reflection preserves the wrapper's outer offset and byte range. This keeps both the ABI gate and the existing Rust `U_FloraGrowthInfo` allocation authoritative without shader-specific aliases.

Matched 5120x2880 captures first painted 25 deterministic lavender brushes across the default world, then rendered the same `player-default` camera. Plant positions, geometry, height palettes, lighting, and shadows were visually equivalent. The native pipeline also completed normal hidden and aggregate release smoke runs. All 152 candidate artifacts pass `spirv-val --target-env vulkan1.3`.

Two order-reversed six-second MoltenVK pairs kept the authored flora resident and discarded samples before frame 120. MoltenVK's nested graphics sub-scope attribution moved most of `graphics.pass` between flora, leaves, and apples on different samples, so the enclosing matched graphics pass is the usable timing evidence:

| Frontend | Samples | Mean | Median | P95 | Range |
| --- | ---: | ---: | ---: | ---: | ---: |
| GLSL / shaderc | 19 | 2,702.68 us | 2,987 us | 3,627 us | 182-3,627 us |
| Native Slang | 18 | 2,562.44 us | 2,934.5 us | 3,416 us | 172-3,416 us |

The broad range and run-order variation preclude a speedup claim, but neither median nor tail indicates a regression. The optimized vertex module contains 2,357 instructions/54,220 bytes from shaderc and 2,450/61,648 from native Slang; the fragment modules contain 18/432 bytes and 14/420 bytes respectively. Isolated flora timing should be repeated on native Vulkan if this path becomes a performance focus.

## Native player-collider traversal

The `slang-player-collider` feature replaces `shader/tracer/player_collider.comp` with a native entry that reuses the accepted `contree_marching` and `scene_marching` modules. It preserves five bindings, a 64x1x1 workgroup, the per-invocation 64x11 contree stack, nine ground rays, nine ceiling rays, 32 horizontal ring rays, shared-memory reductions, and the fixed 32-float result array.

The port exposed undefined synchronization in the fallback source: its final 14 invocations returned before a workgroup barrier reached by the other 50. Both GLSL and native sources now leave those invocations idle but active, so all 64 execute one uniform barrier before invocation zero reduces and writes the results. Both reflection and optimized modules pass Vulkan 1.3 SPIR-V validation, and the automatic gate accepts the complete descriptor, workgroup, uniform, and std430 result-buffer ABI.

The production renderer still creates this pipeline and its reflected resources, but no longer dispatches it after player collision moved to CPU. A temporary validation-only dispatch/readback restored the old call sites and used the fixed in-world origin `(1.0, 1.5, 1.0)`. Shaderc and native Slang repeatedly produced the identical `0.7693079` weighted ground distance and `2.0` ceiling and sampled ring distances through MoltenVK. Since the pass is dormant rather than hot or frequently dispatched, a GPU timing comparison is not applicable. After stripping debug names, its optimized module contains 1,928 instructions/33,048 bytes from shaderc and 2,042/34,292 from native Slang.

## Main tracer through Slang's GLSL frontend

The fourth candidate replaces the backend for `shader/tracer/tracer.comp` under `slang-tracer-backend` without translating its approximately 2,700 transitive lines to native Slang. This validates the branch-heavy DDA/contree traversal, 28-entry include graph, four descriptor sets, camera matrices, structured SSBOs, storage images, sampler arrays, and lighting paths while keeping source logic shared.

Three frontend compatibility adjustments were required:

- Slang's GLSL parser requires explicit extents on the constant Poisson arrays.
- Implicit texture sampling in this compute shader caused Slang to request compute-derivative capabilities. The build defines `DIRECT_SUN_SHADOW_EXPLICIT_LOD` only for the Slang backend, selecting `textureLod(..., 0.0)` in the shadow includes without changing the default GLSL build.
- Slang reflects GLSL parameter groups as `SLANG_ParameterGroup_*` structs and matrices as nested `_MatrixStorage_*` wrappers. Reflection normalization maps these back to the existing resource and matrix layouts. Row-major frontend lowering is required; column-major lowering transposed the uploaded camera matrices and produced an empty traced scene.

A 2560x1440 fixed-camera capture was visually equivalent. The two images were not byte-identical: 649,906 pixels differed, primarily by one or two 8-bit levels from generated floating-point arithmetic and moving/UI content; 21,355 pixels had any channel differ by more than two. No missing terrain, geometry shift, or systematic rendering artifact remained after correcting matrix lowering.

Two order-reversed ten-second hidden timing pairs retained 70 post-startup `tracer.pass` samples per frontend:

| Frontend | Samples | Mean | Median | P95 | Range |
| --- | ---: | ---: | ---: | ---: | ---: |
| GLSL / shaderc | 70 | 10.73 us | 11.5 us | 19 us | 2-21 us |
| GLSL / Slang backend | 70 | 11.46 us | 11 us | 25 us | 2-40 us |

The pass is close to the one-microsecond timestamp resolution in this workload. Its combined median did not regress, while the mean increased by 0.73 us and the P95 was noisier. This establishes functional parity and no measurable typical-time regression on MoltenVK, but not tail-latency parity. The optimized, debug-stripped modules contained 3,979 instructions/71,748 bytes from shaderc and 4,261 instructions/75,712 bytes from Slang. Native Vulkan measurements are still required.

## Native main tracer

The `slang-tracer` feature replaces the same logical `shader/tracer/tracer.comp` path with `shader/slang/tracer.slang`. The retained `slang-tracer-backend` feature remains available as a code-generation baseline; if both are requested, the native override takes precedence.

The native entry keeps descriptor declarations and orchestration together while splitting reusable logic into focused modules for types, materials, direct-sun shadowing, terrain-edit preview, transforms, projection, packing, voxel data, and traversal. The contree and scene-marching modules now receive `StructuredBuffer` and scene-image resources from their caller, allowing the main and shadow tracer entries to share one traversal implementation despite their different bindings. The build-time GLSL-reference check accepts all four descriptor sets, 31 bindings, uniform/storage layouts, image formats, and the 8x8 workgroup.

A 2880x1620 `player-default` capture on an NVIDIA RTX 3060 Ti was visually equivalent to shaderc. Independent app runs differed at 963,421 pixels, but only 27,138 pixels exceeded two 8-bit levels and 5,444 exceeded five levels; the larger differences were concentrated in moving foliage, butterflies, and dynamic UI. No missing terrain, geometry shift, material mismatch, or systematic shadow artifact was visible.

Two order-reversed ten-second native-Vulkan pairs discarded the first ten logged samples from each run:

| Frontend | Samples | `tracer.pass` mean | Median | P95 | Range |
| --- | ---: | ---: | ---: | ---: | ---: |
| GLSL / shaderc | 151 | 467.38 us | 467 us | 477 us | 460-503 us |
| Native Slang | 152 | 473.68 us | 473 us | 482 us | 467-502 us |

The native median was 6 us (`+1.3%`) above shaderc in both run orders; the broader `tracer.render` median moved from 1,588 us to 1,596 us (`+0.5%`). This is small rather than material at the frame level, but repeatable enough to retain as a cross-driver follow-up. Generated-code inspection found 3,979 debug-stripped instructions/71,748 bytes from shaderc, 4,273/75,796 from the GLSL-through-Slang baseline, and 4,309/77,288 from native Slang. The native translation therefore adds only 36 instructions over the same Slang backend; much of the remaining frontend difference is aggregate decomposition and native column-major matrix lowering rather than an algorithm change.

## Largest shader through Slang's GLSL frontend

The `slang-composition-backend` feature compiles the 923-line `shader/tracer/composition.comp` and its approximately 1,279 lines of includes with Slang while preserving GLSL as the source language. This validates the largest production entry point independently from the main tracer. Its ABI includes 16 bindings, six uniform blocks, sampled textures, formatted storage images, camera matrices, and full-resolution branch-heavy sky, cloud, glass, SSR, and composition logic.

Slang initially inferred compute-derivative capability from the two implicit texture samples. The feature supplies `COMPOSITION_EXPLICIT_LOD`, selecting `textureLod(..., 0.0)` only for the Slang backend. The resulting module requires no compute-derivative capability and passes `spirv-val --target-env vulkan1.3`. The build-time GLSL-reference check accepts the complete descriptor and compute ABI.

A 5120x2880 fixed-camera screenshot was visually equivalent. Most changed channels differed by one or two 8-bit levels from generated floating-point arithmetic; larger differences were concentrated in moving butterflies and dynamic UI. Three order-varied hidden release pairs produced these combined `composition.pass` results after frame 120:

| Frontend | Samples | Mean | Median | P95 |
| --- | ---: | ---: | ---: | ---: |
| GLSL / shaderc | 93 | 290.75 us | 286 us | 348.8 us |
| GLSL / Slang backend | 99 | 287.74 us | 280 us | 336.6 us |

Run-level medians moved from Slang being 2.0% slower to 3.5% faster as run order and system state changed. The combined delta was `-1.04%` mean and `-2.10%` median, so there is no evidence of a regression. Debug-stripped optimized SPIR-V contained 1,105 instructions/20,452 bytes from shaderc and 1,145 instructions/20,860 bytes from Slang.

## Native composition modules

The `slang-composition` feature replaces the same logical path with a native entry and focused scene, sky, starlight, sunlight, cloud, hash, panel, glass, SSR, and type modules. It covers the active open-tank production path: 16 bindings, sky keyframes, sun-sprite projection, procedural starlight, precomputed screen clouds, raster/ray-traced composition, lens flare, and god rays. The retained `slang-composition-backend` feature remains available as a frontend baseline; native takes precedence if both are enabled. The panel, analytic glass, volumetric-cloud reflection, and SSR helpers remain disabled in production, but are fully translated and receive resources from the entry point rather than owning bindings.

Matched 2880x1620 day and night captures were visually equivalent. Independent daytime runs differed at 959,895 pixels before tolerance, 25,090 above two 8-bit levels, and 4,055 above five levels; visible differences were confined to moving leaves and dynamic UI. The final modular night capture exercises the expensive starlight path and differed at only 159 pixels, with no systematic sky difference.

Two order-reversed ten-second RTX 3060 Ti pairs discarded the first ten logged samples from each run:

| Frontend | Samples | Mean | Median | P95 | Range |
| --- | ---: | ---: | ---: | ---: | ---: |
| GLSL / shaderc | 146 | 62.59 us | 62 us | 63 us | 61-91 us |
| Native Slang | 145 | 62.72 us | 63 us | 65 us | 61-66 us |

One pair had equal 62 us medians; the reversed pair measured native at 63 us versus shaderc at 62 us. The 1 us combined delta is at timestamp and run-to-run resolution rather than evidence of a material regression. Debug-stripped optimized SPIR-V contained 1,105 instructions/20,452 bytes from shaderc, 1,145/20,860 from the GLSL-through-Slang baseline, and 1,182/21,684 from native Slang. Dead-code elimination keeps the disabled helper modules out of the production artifact.

For compile and behavior coverage of the disabled source, the old panel/glass calls were temporarily restored in both entry points. Native and shaderc 2880x1620 captures remained visually equivalent: only 31,222 pixels exceeded two 8-bit levels, concentrated in moving leaves, particles, and UI. A second temporary gate routed glass reflections through the otherwise-unused volumetric-cloud helper; the result remained visually equivalent and its 156,620-byte native SPIR-V passed Vulkan 1.3 validation. These paths remain disabled in committed production behavior.

## Compatibility findings

Slang names SPIR-V buffer-layout wrapper types with suffixes such as `_std140`, while existing GLSL reflection exposes source type names directly. Its GLSL frontend additionally adds the `SLANG_ParameterGroup_` prefix and materializes matrices as `_MatrixStorage_*` structs; native fixed arrays can appear as one-member `_Array_*` wrappers. The runtime normalizes these forms at the reflection boundary, allowing both frontends to resolve the same resource definitions, matrices, and fixed-array byte ranges without shader-specific aliases. The build-time ABI reflector independently recovers fixed-array dimensions and strides from the nested wrapper member.

## Limits of this result

The candidates establish Slang compatibility with the current shared-memory, uniform-barrier, atomic, storage-image, runtime/fixed-array, structured-SSBO, matrix, branch-heavy traversal, workgroup-reduction, and complex graphics-stage interface patterns. Native Slang modules cover all 76 production entries. Accepted sources live under `shader/slang/`, dependency-aware artifact reuse is active, and official Slang v2025.23 archives are checksum-pinned for the macOS, Windows, and Fedora validation matrix. The default should not switch until that hosted workflow succeeds and additional Windows/Linux native-Vulkan vendor coverage is recorded. The disabled composition helper source is translated and compile/visual-tested, but would need a fresh performance gate if product behavior re-enables it; the dormant player-collider pass similarly has matched execution/readback rather than production timing evidence. Flora timing is currently aggregate MoltenVK evidence because nested child-scope attribution is unstable; native Vulkan timing should be repeated across additional GPU vendors and drivers. The current source tree does not actively use buffer references or Vulkan sparse-residency intrinsics; those should be tested if introduced later.
