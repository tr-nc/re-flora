# Slang compatibility and performance validation plan

The authoritative migration status, per-entry-point checklist, phased roadmap, and next work queue live in [`slang-migration-roadmap.md`](slang-migration-roadmap.md). This document owns validation policy and evidence; do not duplicate the inventory here.

## Decision to make

Determine whether Slang can become re-flora's primary shader language without losing Vulkan functionality, correctness, or GPU performance. Migration effort is not part of the decision, but recurring build cost, toolchain reliability, runtime behavior, and maintainability are.

The experiment must preserve a mixed-language transition period. GLSL remains the default until the compatibility and performance gates below pass. Every Slang candidate must be independently selectable so its output and timing can be compared with the exact GLSL shader it replaces.

## Current baseline

The project currently has 76 GLSL entry points and approximately 13,500 shader lines. The build embeds reflection and optimized SPIR-V artifacts, so neither frontend is present at runtime.

The first proof of concept replaced `shader/tracer/post_processing.comp` behind a Cargo feature. On Apple M4 Pro through MoltenVK it produced a byte-identical 5120x2880 image and changed the pass mean from 713.67 us to 712.38 us (`-0.18%`). This proves basic build, reflection, dispatch, storage-image, and MoltenVK compatibility, but not compatibility with the difficult shaders.

A fresh source scan shows that the active shader tree currently relies on:

- compute, vertex, and fragment entry points;
- workgroup `shared` arrays and `barrier()`;
- `atomicAdd` and `atomicOr` on SSBO and shared state;
- structured and runtime-sized SSBO arrays;
- std140/std430-compatible uniform and storage layouts;
- formatted 2D and 3D storage images, including read-only and write-only access;
- many descriptor sets and sparse binding numbers;
- matrix-heavy camera data and explicit column-major compatibility;
- bit packing, integer shifts, image loads/stores, sampler arrays, and large include graphs.

There is no active `GL_EXT_buffer_reference` or Vulkan sparse-residency texture intrinsic in the current tree. The word `sparse` currently refers to sparse project data structures and work lists, not sparse image residency. Optional shader-clock code exists but is commented out in the tracer. These capabilities should not be claimed as current migration blockers, though they can be tested separately if they are planned for future use.

## Coexistence design

The build will retain one logical shader path and choose its frontend at build time:

- no Slang feature: all logical paths compile from the existing GLSL sources;
- one candidate feature: only that candidate is replaced by Slang;
- aggregate validation feature: all completed Slang candidates are enabled together;
- runtime Rust code, descriptor layouts, and pipeline selection remain unchanged.

Planned feature boundaries:

| Feature | Slang replacement scope |
| --- | --- |
| `slang-post-processing` | Existing simple proof of concept |
| `slang-composition` | Native composition entry plus sky, cloud, panel, glass, and SSR modules |
| `slang-composition-backend` | Retained composition GLSL-through-Slang baseline |
| `slang-surface` | Aggregate of completed surface-construction candidates |
| `slang-surface-clear-occupancy` | Flora occupancy-image clearing |
| `slang-surface-make` | Dense surface extraction through the shared normal-generation core |
| `slang-surface-make-sparse` | Sparse surface extraction and normal generation |
| `slang-surface-prepare-active-flora-dispatch` | Active-surface flora indirect-dispatch preparation |
| `slang-surface-prepare-sparse-dispatch` | Sparse workgroup filtering and indirect-dispatch preparation |
| `slang-contree` | Aggregate of completed contree construction candidates |
| `slang-contree-buffer-setup` | Contree level-state, indirect-dispatch, counter, and offset initialization |
| `slang-contree-buffer-update` | Contree level-state advancement and indirect tree dispatch |
| `slang-contree-concat` | Final level concatenation and absolute child-offset rewrite |
| `slang-contree-last-buffer-update` | Root-node seeding and final concat indirect dispatch |
| `slang-contree-tree-write` | Shared-prefix compaction of intermediate contree levels |
| `slang-contree-leaf` | Contree sparse-leaf construction and allocation |
| `slang-egui` | Native Slang vertex/fragment interface pair |
| `slang-flora` | Native complex flora vertex/fragment pair plus motion, color, and shadow modules |
| `slang-player-collider` | Native player-collision traversal, workgroup reduction, and fixed-array output |
| `slang-tracer` | Native main tracer resources, lighting, materials, and orchestration |
| `slang-tracer-backend` | Retained main-tracer GLSL-through-Slang baseline |
| `slang-tracer-shadow` | Native Slang contree/DDA shadow tracer modules |
| `slang-validation` | Aggregate of all completed candidates |

The existing `slang-poc` feature remains as a backward-compatible alias. A single declarative override mapping in `crates/re-flora-vkn/build.rs` owns logical path, replacement source, stage, frontend, include root, and defines. The frontend is explicitly either native Slang 2025 or GLSL through Slang; paths without an enabled override remain shaderc GLSL. This avoids scattered hard-coded substitutions and keeps the runtime independent of source language.

Every enabled override is checked against a freshly compiled GLSL reference during the build. The check rejects duplicate or missing logical paths, stage/extension mismatches, missing source/include paths, and differences in shader stage, compute workgroup size, descriptor set/binding/type/count/image contract, top-level buffer member offsets/sizes/array strides, push constant ranges and member layout, stage input/output locations and formats, arrays, or interpolation decorations. Slang sources use explicit `vk::binding` and `vk::image_format` annotations. Existing Rust resource definitions remain the detailed buffer-layout authority, while frontend-specific SPIR-V names are normalized only at the reflection boundary.

## Key shader candidates

### 1. Surface extraction and normal generation: first implementation

- GLSL: `shader/builder/surface/make_surface_sparse.comp`
- Timing label: `make_surface_sparse` in `[PERF][SURFACE_PASS_TIMING]`
- Why it matters:
  - 8x8x8 workgroup with 512 invocations;
  - 12x12x12 three-dimensional shared-memory tile;
  - workgroup barrier;
  - `atomicAdd` and `atomicOr` on storage buffers;
  - formatted read-only and write-only 3D storage images;
  - nested loops performing the 5x5x5 normal extraction kernel;
  - bit packing and runtime SSBO arrays.

This is the best first compatibility gate because one shader covers shared memory, barriers, atomics, storage images, normal extraction, and meaningful GPU work.

### 2. Contree construction: second implementation

Primary files:

- `shader/builder/contree/leaf_write.comp`
- `shader/builder/contree/tree_write.comp`

Timing labels:

- `leaf_write`
- `tree_write_0` through `tree_write_7`
- total `[PERF][CONTREE_PASS_TIMING] pass_total`

Why they matter:

- shared prefix-sum scratch storage and shared group base;
- multiple barriers;
- global atomics that allocate node and leaf ranges;
- structured node SSBOs and runtime arrays;
- per-invocation 64-element temporary arrays;
- dynamic dispatch state across multiple tree levels.

`leaf_write.comp` is the first compatibility target. Baseline pass timings will determine whether `tree_write.comp` or another contree pass dominates total construction time; the dominant pass is then the performance target. The complete contree pipeline does not need to be rewritten merely to measure an isolated replacement.

### 3. Main tracer: third implementation

- GLSL: `shader/tracer/tracer.comp`
- Timing label: `tracer.pass` inside the broader `tracer.render` scope

Why it matters:

- branch-heavy ray and DDA traversal;
- shared per-invocation contree marching stacks;
- a large transitive include graph;
- four descriptor sets and many texture/image formats;
- structured buffers, matrices, samplers, sampler arrays, and storage outputs;
- direct and indirect lighting paths;
- pressure on compiler inlining, register allocation, and control-flow optimization.

The tracer is the decisive backend optimization comparison. The first stage compiles the existing source and include graph through Slang's GLSL frontend, isolating code generation and runtime compatibility from translation differences. The accepted second stage is a native Slang entry point split into focused type, material, shadowing, preview, transform, packing, projection, and traversal modules.

### 4. Composition: largest-source backend and native gate

- GLSL: `shader/tracer/composition.comp`
- Timing label: `composition.pass`
- Why it matters:
  - largest production entry point at 923 lines plus approximately 1,279 lines of includes;
  - 16 descriptor bindings, six uniform blocks, sampled textures, and formatted storage images;
  - camera matrices, sky/starlight/cloud paths, analytic terrarium glass, SSR, and branch-heavy per-pixel composition;
  - full-resolution execution makes small backend regressions measurable.

As with the main tracer, the first stage keeps the GLSL source unchanged and switches only to Slang's GLSL frontend. The accepted second stage replaces the active production path with native entry, scene, sky, sunlight, starlight, cloud, hash, and type modules. The currently disabled open-tank panel, glass, volumetric-cloud reflection, and SSR helpers are also translated into resource-parameterized modules and were validated through temporary reactivation without changing production behavior.

### Secondary coverage

After the primary gates:

- the completed `shader/tracer/player_collider.comp` for additional shared-memory and synchronization coverage;
- the egui vertex/fragment pair for graphics-stage interfaces, push constants, vertex formats, interpolation, and combined image samplers;
- the completed flora vertex/fragment pair for complex graphics-stage resources, fixed arrays, raw Vulkan instance indexing, wind, shadows, and interpolation;
- optional shader clock support if it is re-enabled;
- any future buffer-reference or sparse-residency prototype before those features enter production.

## Validation gates

Each candidate must pass all applicable gates before the next candidate becomes the focus.

### Build and SPIR-V

- default `cargo check` succeeds without locating or invoking `slangc`;
- candidate and aggregate feature builds succeed on macOS, Windows, and Fedora CI;
- `spirv-val` accepts reflection and optimized artifacts;
- descriptor sets, bindings, descriptor types, image formats, workgroup size, and buffer member offsets match the GLSL contract;
- required SPIR-V capabilities and extensions are compared rather than assumed;
- compiler version and flags are logged and pinned for reproducibility.

### Correctness

- hidden release runs complete successfully on MoltenVK and native Vulkan;
- no Slang-specific validation-layer errors appear;
- deterministic screenshots are compared byte-for-byte when possible and with an explicit tolerance otherwise;
- surface active voxel/brick counts match;
- contree node/leaf counts and rendered traversal results match;
- tracer output attachments and final screenshots match under a fixed camera and fixed configuration;
- nondeterministic atomic allocation order is judged by semantic output, not raw buffer byte order alone.

### Performance

Performance evidence comes only from release-mode hidden runs. Debug builds and compile-time unit tests are not performance evidence.

Use matched runs from the same worktree and configuration. Alternate frontend order when collecting repeated trials to reduce thermal and temporal bias.

Surface and contree benchmark shape:

```bash
RUST_LOG=info,re_flora::builder::surface=debug,re_flora::builder::contree=debug \
  cargo run --release -- --hidden --mute --tree-bench --tree-bench-samples 20
```

Repeat with the candidate feature. Compare per-pass GPU timestamp labels, pass totals, active counts, and end-to-end tree benchmark time.

Tracer benchmark shape:

```bash
cargo run --release -- --hidden --mute --auto-exit 10 --perf
```

Repeat with `--features slang-tracer-backend` for the backend-only baseline and `--features slang-tracer` for the native candidate, discard startup frames, and compare `tracer.pass` plus `tracer.render`. Use enough samples to report count, mean, median, P95, range, and percentage delta. Investigate changes larger than normal run-to-run variance; do not accept a regression merely because the generated SPIR-V validates.

Also record optimized SPIR-V byte size and stripped instruction count as diagnostics. They do not override measured GPU time.

## Build-time requirement

The original `slangc` process startup was approximately 3.75 times slower than `glslc` for the proof-of-concept shader, and the first mixed build launched it once per artifact. That recurring startup cost is now removed: the build dynamically loads the Slang compiler library once and reuses one global session for all selected reflection and optimized compile requests.

On the local Linux Slang 2025.23.2 toolchain, three package-clean aggregate checks at the earlier 16-artifact snapshot improved from a 6.29 s median to 5.05 s, while three shader-touched incremental checks improved from 5.83 s to 4.66 s. All 16 API-generated artifacts were byte-identical to equivalent standalone `slangc` output. The current aggregate compiles 40 selected Slang artifacts in the shared session, and all 152 aggregate artifacts pass `spirv-val --target-env vulkan1.3`. The current 20-entry aggregate hidden release smoke run completes on native Vulkan; the preceding 16-entry aggregate also passed through MoltenVK.

The Phase 2 reassessment also compared the current 11-entry aggregate against the default build on Apple M4 Pro using three order-interleaved samples per frontend. Package-clean medians were 3.72 s for default GLSL and 6.37 s for the aggregate; shader-touched medians were 3.13 s and 5.54 s. This exposed all-entry recompilation as the blocker before broad family migration.

The build now records shaderc's resolved includes and Slang's resolved module dependencies for each logical entry. A selected replacement tracks both its GLSL ABI reference and native graph. BLAKE3 manifests bind those files, target/frontend/compiler context, and both SPIR-V artifacts. Three-sample aggregate medians are now 2.29 s with all 76 entries reused, 2.42 s when one GLSL entry recompiles, and 4.03 s when a shared native module recompiles four entries. Package-clean checks remain comparable at 6.73 s. Dependency-edit, artifact-corruption, byte-equivalence, and SPIR-V validation checks pass. Exact compiler pinning and cross-platform reproduction remain required before the final default decision.

## Execution order

1. Generalize the existing feature-gated build mapping while preserving the default GLSL build.
2. Port and validate `make_surface_sparse.comp`.
3. Capture matched hidden surface/normal extraction benchmarks.
4. Port `leaf_write.comp`, measure the contree pass breakdown, then port the dominant contree construction pass.
5. Capture matched hidden contree benchmarks and correctness evidence.
6. Compile `tracer.comp` and its existing includes through Slang's GLSL frontend.
7. Rewrite the shared contree and DDA traversal as native Slang modules and validate them through `tracer_shadow.comp`.
8. Rewrite the full main tracer with reusable native modules and validate it against both frontend baselines.
9. Rewrite the active composition path with reusable native sky and starlight modules and validate it against both frontend baselines.
10. Validate complex graphics-stage pairs.
11. Port the player-collider traversal and synchronization consumer.
12. Reassess the decisive coverage before beginning family-wide migration.
13. Move accepted sources to the production tree and add compiler-resolved per-entry artifact caching.
14. Add broader native Vulkan performance coverage and cross-platform CI validation.
15. Decide among Slang default, mixed Slang/GLSL production use, or retaining GLSL as default.

## Current status

- Build selection is declarative and supports independent per-shader-family features plus the aggregate `slang-validation` feature. The source-language-neutral logical path is preserved, and every active override now receives an automatic build-time pipeline ABI comparison against its GLSL reference.
- `make_surface_sparse.comp` has been ported and runs successfully through MoltenVK with shared memory, synchronization, atomics, formatted storage images, std140/std430 blocks, and runtime arrays.
- Matched hidden tree benchmarks produced identical surface workload counts and scene output. Typical GPU time is at parity; run-order-sensitive mean and P95 variation requires more native Vulkan evidence before a performance verdict.
- `prepare_sparse_surface_dispatch.comp` is native under `slang-surface-prepare-sparse-dispatch`, while `slang-surface` aggregates it with both extraction entries. The entry preserves six descriptors, the 128x1x1 workgroup, atlas-dimension lookup, solid-workgroup bit filtering, compacted index allocation, and atomic indirect-dispatch maximum. Four order-reversed five-sample native-Vulkan tree benchmarks matched all 44 surface workloads per run. Filtering to 28 heavy workloads per run gave a 7 us combined median for both frontends and 6-8 us ranges; the pass is too short for stronger performance conclusions.
- `make_surface.comp` is native under `slang-surface-make`. It and the accepted sparse entry share halo preload, occlusion, 5x5x5 normal extraction, packing, and brick-index logic while retaining their distinct result layouts and dispatch mapping. Its six descriptors, 8x8x8 workgroup, storage-image formats, atomics, capabilities, and top-level layouts pass the automatic gate and Vulkan 1.3 validation. The current runtime selects only the sparse entry, so direct dense dispatch is not an applicable production runtime gate; a matched sparse tree benchmark exercises the shared core and preserves all 44 workloads.
- `clear_occupancy.comp` is native under `slang-surface-clear-occupancy`. Its uniform, read/write `r32ui` image, 8x8x8 workgroup, bounds check, and zero store pass the ABI and SPIR-V gates. A temporary Grass Mix mode for the existing authored-flora benchmark exercised the production flora-edit dispatch. Twenty-five matched native-Vulkan edits produced identical before/after/appended instance counters through both frontends; clear-pass medians were 169 us for GLSL and 161 us for native Slang, with outliers moving in both directions across shorter order-reversed runs.
- `prepare_active_surface_flora_dispatch.comp` is native under `slang-surface-prepare-active-flora-dispatch`. It preserves two storage buffers, a 1x1x1 workgroup, 64 invocations per active brick, 128 invocations per flora group, and the minimum-one-group empty-chunk contract. Temporarily enabling `place_flora` during initial loading exercised all eight chunks in four order-reversed runs. The four non-empty chunks produced identical five-species instance counts through every frontend; both frontends had a 3 us combined median for the preparation pass.
- `leaf_write.comp`, the measured dominant contree construction shader, has been ported. Matched node/leaf sizes and scene output are identical, and both frontends have a combined 44 us median on MoltenVK.
- `buffer_setup.comp`, `buffer_update.comp`, and `last_buffer_update.comp` are native under independent contree features. They share contree-build layouts and helpers with leaf writing, preserve their seven-, two-, and five-descriptor ABIs and 1x1x1 workgroups, and produce matching node/leaf workloads in hidden tree benchmarks. The 28-sample local medians were 9 us for setup and 2 us for both update passes with both frontends; these state passes are correctness-sensitive but too short for meaningful isolated performance conclusions.
- `tree_write.comp` is native under `slang-contree-tree-write`. Its six descriptors, 4x4x4 workgroup, shared prefix allocation, atomics, and 64-node per-invocation temporary array pass the ABI and runtime gates. All three measured tree levels matched 28 node/leaf workloads; GLSL/native medians were 20/20, 14/13, and 8/7 us. Both sources now route inactive edge invocations through every barrier before returning.
- `concat.comp` completes the six-entry native contree family. Its six descriptors, 256x1x1 workgroup, fixed ten-level upper-bound array, and relative-to-absolute child offset rewrite pass the gates. The isolated concat median was 3 us for both frontends. With all six entries enabled, all 28 workloads matched and the full contree pipeline median was 107.5 us versus 112.5 us for GLSL.
- `tracer.comp` retains its GLSL-through-Slang baseline under `slang-tracer-backend` and now has a full native implementation under `slang-tracer`. The native entry reuses resource-independent contree/DDA traversal and focused material, shadowing, preview, transform, packing, projection, and type modules. Its four-set ABI matches automatically, all SPIR-V validates, and a 2880x1620 fixed-camera image is visually equivalent. Two order-reversed RTX 3060 Ti pairs retained 151 shaderc and 152 native post-startup samples: `tracer.pass` medians were 467 us and 473 us (`+1.3%`), while `tracer.render` medians differed by `+0.5%`. The small repeatable pass delta is documented for cross-driver follow-up rather than hidden by aggregate frame noise.
- The production `tracer_shadow.comp` path has been rewritten in native Slang modules under `slang-tracer-shadow`. The modules cover AABB intersection, camera-ray projection, contree traversal, DDA scene traversal, voxel decoding, workgroup stack storage, structured SSBOs, storage images, and matrix uniforms. Fixed-camera shadow output is visually equivalent. Two order-reversed local MoltenVK pairs had native medians 0.8-1.4% lower, within run noise.
- The egui vertex/fragment pair has been rewritten in native Slang under `slang-egui`. Runtime pipeline-layout merging, three vertex attributes, two interpolants, a matrix push constant, combined image sampler, alpha blending, and UI rendering all pass on MoltenVK. Static UI regions are visually equivalent, with only sparse 1-3-level color differences.
- The complex `flora.vert`/`flora.frag` pair has been rewritten under `slang-flora`. It preserves 18 bindings across two sets, fixed-array uniforms and push constants, the raw Vulkan instance index required by nonzero first-instance draws, storage buffers/images, wind sampling, direct-sun shadows, depth offsets, and smooth color interpolation. The automatic ABI gate required fixed-array wrapper normalization at both build-time and runtime reflection boundaries. Matched 25-brush authored-lavender captures are visually equivalent. Two order-reversed six-second MoltenVK pairs retained 19 shaderc and 18 native post-startup `graphics.pass` samples; medians were 2,987 us and 2,934.5 us, while sub-scope attribution was too unstable for a meaningful isolated flora timing. No aggregate graphics regression was measured.
- `player_collider.comp` has been rewritten under `slang-player-collider` by reusing the native contree/DDA modules. Its five descriptors, 64x1x1 workgroup, fixed-array result layout, per-invocation traversal stacks, and workgroup reduction pass the ABI gate and SPIR-V validation. The GLSL fallback and native source both keep all 64 invocations active through the barrier instead of allowing the 14 non-ray invocations to return early. The production pass is dormant after collision moved to CPU, so no hot-pass benchmark applies; temporary matched dispatch/readback at a fixed in-world origin produced identical `0.7693079` ground distance and `2.0` ceiling/ring distances through both frontends.
- `composition.comp`, the largest production entry point, retains its GLSL-through-Slang baseline under `slang-composition-backend` and now has a native implementation under `slang-composition`. Its 16-binding ABI passes the automatic GLSL-reference check, and explicit LOD on sampled textures avoids requesting compute-derivative capabilities. At 2880x1620, day and night captures are visually equivalent: the final modular night/starlight pair differed at only 159 pixels across independent runs, while larger daytime differences were confined to moving leaves and UI. Two order-reversed RTX 3060 Ti pairs retained 146 shaderc and 145 native post-startup samples; both had 62-63 us medians, with the native combined median 1 us higher and within timestamp/run variance. The disabled panel, glass, volumetric-cloud reflection, and SSR paths are fully translated; temporarily re-enabling both GLSL and native implementations produced visually equivalent glass/cloud-reflection captures and valid Vulkan 1.3 SPIR-V.
- The aggregate build now uses one dynamically loaded Slang compiler API global session for all 40 selected compile requests instead of one `slangc` process per artifact. The earlier 16-artifact benchmark improved package-clean and shader-touched checks by about 20%, all measured API artifacts matched standalone compiler output byte-for-byte, and Cargo logs the loaded compiler build tag.
- Compiler-reported transitive dependency graphs now drive per-entry reflection/optimized SPIR-V caches. Cache hits verify dependency, context, and artifact digests; a native override includes its GLSL ABI-reference dependencies. Targeted GLSL, transitive GLSL, shared native module, and corrupted-artifact invalidation tests recompiled only affected entries, while all 152 clean outputs matched pre-cache bytes.
- The Phase 2 reassessment supports continued staged migration: difficult capabilities and semantic gates pass, and no material frame-level GPU regression is present. GLSL remains the default because compiler pinning, broader native-Vulkan performance coverage, and cross-platform CI remain open. All 60 accepted sources live under `shader/slang/`, dependency-aware artifact reuse is active, and explicit declarations plus automatic ABI checking remain the binding source of truth.

## Decision criteria

The Phase 2 decision is to continue isolated and family-by-family native migration while retaining GLSL as the default. This is approval to continue migration, not approval for a default switch. The source layout and build invalidation are now productionized, so Phase 3 family migration can begin. Compiler pinning, cross-platform CI, and broader native-Vulkan timing remain Phase 5/default-switch gates.

Adopt Slang as the default when all current production capabilities are covered, difficult shader outputs are equivalent, no material GPU regression remains, cross-platform compilation is reproducible, and compiler-session integration makes normal iteration acceptable.

Keep a mixed production build if Slang is clearly beneficial for most modules but one or two Vulkan-specific shaders require fragile workarounds. Retain GLSL as default if the tracer or construction passes regress materially, correctness depends on compiler-version-specific behavior, or CI/toolchain reliability remains worse than the maintainability benefit.
