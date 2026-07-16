# Slang shader proof of concept

For migration status, completion criteria, the 76-entry-point checklist, and next tasks, see [`slang-migration-roadmap.md`](slang-migration-roadmap.md). This document is the operator guide and technical evidence record.

This experiment incrementally replaces selected GLSL compute shaders with equivalent Slang implementations. The normal build remains GLSL-only, and each replacement can be enabled independently for matched comparison.

The first pass, `shader/tracer/post_processing.comp`, was selected because it runs every frame at the full output resolution, already has a Vulkan timestamp scope, and exercises a uniform buffer, formatted storage images, bounds checks, and a reusable dither module. The surface and contree-leaf passes cover difficult shared-memory, synchronization, atomic, and structured-buffer paths. The main tracer first established a GLSL-through-Slang backend baseline and now also has a complete native Slang implementation.

## Requirements

Install a Slang distribution that includes both `slangc` and the compiler shared library. The build locates the library through:

1. the `SLANG_LIB` environment variable,
2. a library beside or under the installation containing `SLANGC`,
3. `$VULKAN_SDK`, or
4. the installation containing `slangc` on `PATH`.

The build dynamically loads the compiler API only when a Slang feature is enabled. A default build does not locate or load Slang. The initial validation used Vulkan SDK 1.4.321.0 and Slang `2025.11-12-gc5295eae2` on macOS; the shared-session build-cost validation used Vulkan SDK Slang `2025.23.2` on Linux. Native Slang sources pin the Slang 2025 language rules and column-major matrices. The GLSL frontend uses row-major lowering to preserve the existing GLSL std140 matrix bytes. Both paths emit SPIR-V 1.6 with Vulkan GL-compatible buffer layout.

## Build and validation

The default build does not invoke Slang:

```bash
cargo check
```

Enable an individual replacement or all completed validation candidates with:

```bash
cargo check --features slang-post-processing
cargo check --features slang-composition
cargo check --features slang-composition-backend
cargo check --features slang-surface-make
cargo check --features slang-surface-make-sparse
cargo check --features slang-surface-prepare-sparse-dispatch
cargo check --features slang-surface
cargo check --features slang-contree-leaf
cargo check --features slang-egui
cargo check --features slang-flora
cargo check --features slang-player-collider
cargo check --features slang-tracer
cargo check --features slang-tracer-backend
cargo check --features slang-tracer-shadow
cargo check --features slang-validation
```

`slang-poc` remains a backward-compatible alias for `slang-post-processing`.

The build reports each frontend separately, for example:

```text
precompiled 58 shaderc GLSL, 0 Slang GLSL, and 18 native Slang shaders into SPIR-V artifacts
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

The package-clean ranges were 6.24-6.38 s before and 5.05-5.10 s after. The shader-touched ranges were 5.82-5.83 s before and 4.60-4.71 s after. All 16 reflection and optimized artifacts from that API-path snapshot were byte-identical to artifacts generated by equivalent standalone `slangc` commands. The current aggregate submits 36 selected artifacts through the shared session, and all 152 aggregate artifacts pass `spirv-val --target-env vulkan1.3`. The current 18-entry aggregate hidden release smoke run completes on an NVIDIA RTX 3060 Ti through native Vulkan; the preceding 16-entry aggregate also passed through MoltenVK. The API reports its actual build tag in Cargo output; exact version pinning and Windows/macOS reproduction remain open release tasks.

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

The independent feature, three-entry `slang-surface` aggregate, and full `slang-validation` aggregate pass the automatic GLSL-reference ABI gate. All 152 aggregate reflection and optimized artifacts pass `spirv-val --target-env vulkan1.3`; an aggregate 2400x1350 hidden release smoke run completed on an NVIDIA RTX 3060 Ti without validation errors or panics.

Four five-sample tree benchmarks used order `GLSL, Slang, Slang, GLSL`. Every run produced the same 44 chunk, active-voxel, active-brick, and solid-workgroup workloads. After filtering to the 28 workloads with at least 10,000 solid workgroups per run, the combined results were:

| Frontend | Samples | Mean | Median | P95 range across runs | Overall range |
| --- | ---: | ---: | ---: | ---: | ---: |
| GLSL / shaderc | 56 | 7.04 us | 7 us | 7.65-8 us | 6-8 us |
| Native Slang | 56 | 6.77 us | 7 us | 7-8 us | 6-8 us |

The one-microsecond timestamp resolution dominates this pass, so the result is a correctness and regression guard rather than evidence of a speedup. After stripping debug data, both optimized modules contain 167 SPIR-V instructions; shaderc is 2,788 bytes and native Slang is 2,824 bytes.

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

The candidates establish Slang compatibility with the current shared-memory, uniform-barrier, atomic, storage-image, runtime/fixed-array, structured-SSBO, matrix, branch-heavy traversal, workgroup-reduction, and complex graphics-stage interface patterns. Native Slang modules now cover the main, shadow, and player-collider tracer entries, the full composition source, the complex flora pair, and the egui pair. This is sufficient to continue staged migration, not to switch the default: accepted sources now live under `shader/slang/` and dependency-aware artifact reuse is active, while the dynamically loaded API path still needs exact version pinning and Windows/macOS CI coverage. The disabled composition helper source is translated and compile/visual-tested, but would need a fresh performance gate if product behavior re-enables it; the dormant player-collider pass similarly has matched execution/readback rather than production timing evidence. Flora timing is currently aggregate MoltenVK evidence because nested child-scope attribution is unstable; native Vulkan timing should be repeated across additional GPU vendors and drivers. The current source tree does not actively use buffer references or Vulkan sparse-residency intrinsics; those should be tested if introduced later.
