# Slang compatibility and performance validation plan

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
| `slang-surface` | Surface extraction and normal generation candidate |
| `slang-contree` | Contree construction candidates |
| `slang-tracer-backend` | Main tracer compiled by Slang's GLSL frontend |
| `slang-validation` | Aggregate of all completed candidates |

The existing `slang-poc` feature will remain as a backward-compatible alias while the build logic is generalized. A single declarative mapping in `crates/re-flora-vkn/build.rs` will own logical path, Slang source, stage, and feature selection. This avoids scattered hard-coded substitutions.

Slang sources will use explicit `vk::binding` and `vk::image_format` annotations. Existing Rust resource definitions remain the ABI authority. Frontend-specific SPIR-V names are normalized only at the reflection boundary.

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

The tracer is the decisive backend optimization comparison. The first stage compiles the existing source and include graph through Slang's GLSL frontend, isolating code generation and runtime compatibility from translation differences. A native Slang/module rewrite remains a separate maintainability experiment if the backend result is acceptable.

### Secondary coverage

After the three primary gates:

- `shader/tracer/player_collider.comp` for additional shared-memory and synchronization coverage;
- one foliage vertex/fragment pair for graphics-stage interfaces and interpolation;
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

Repeat with `--features slang-tracer-backend`, discard startup frames, and compare `tracer.pass` plus `tracer.render`. Use enough samples to report count, mean, median, P95, range, and percentage delta. Investigate changes larger than normal run-to-run variance; do not accept a regression merely because the generated SPIR-V validates.

Also record optimized SPIR-V byte size and stripped instruction count as diagnostics. They do not override measured GPU time.

## Build-time requirement

The current `slangc` process startup is approximately 3.75 times slower than `glslc` for the proof-of-concept shader. Migration effort may be ignored, but this recurring iteration cost cannot be ignored.

Before making Slang the default for many entry points, replace one-process-per-artifact compilation with one of:

1. a persistent Slang compiler API session;
2. a combined module/entry-point invocation;
3. cached serialized modules with dependency-aware invalidation.

The mixed build may use individual `slangc` processes during compatibility experiments, but that is not the final production architecture.

## Execution order

1. Generalize the existing feature-gated build mapping while preserving the default GLSL build.
2. Port and validate `make_surface_sparse.comp`.
3. Capture matched hidden surface/normal extraction benchmarks.
4. Port `leaf_write.comp`, measure the contree pass breakdown, then port the dominant contree construction pass.
5. Capture matched hidden contree benchmarks and correctness evidence.
6. Compile `tracer.comp` and its existing includes through Slang's GLSL frontend, then evaluate whether a native Slang/module rewrite is warranted.
7. Capture matched hidden tracer benchmarks on MoltenVK and native Vulkan.
8. Validate one graphics-stage pair and cross-platform CI.
9. Decide among Slang default, mixed Slang/GLSL production use, or retaining GLSL as default.

## Current status

- Build selection is declarative and supports independent post-processing, surface, contree-leaf, and tracer-backend features plus the aggregate `slang-validation` feature.
- `make_surface_sparse.comp` has been ported and runs successfully through MoltenVK with shared memory, synchronization, atomics, formatted storage images, std140/std430 blocks, and runtime arrays.
- Matched hidden tree benchmarks produced identical surface workload counts and scene output. Typical GPU time is at parity; run-order-sensitive mean and P95 variation requires more native Vulkan evidence before a performance verdict.
- `leaf_write.comp`, the measured dominant contree construction shader, has been ported. Matched node/leaf sizes and scene output are identical, and both frontends have a combined 44 us median on MoltenVK.
- `tracer.comp` now compiles through Slang's GLSL frontend behind `slang-tracer-backend`. It runs through MoltenVK with the existing four-set descriptor ABI, structured SSBOs, storage images, camera matrices, and include graph. A fixed-camera image is visually equivalent; broad one- to two-level pixel differences and dynamic content prevent byte identity. Two order-reversed timing pairs showed no median regression in the approximately 11 us `tracer.pass`, while tail timing remains too quantized and noisy for a strong conclusion.
- Remaining primary gates are a graphics-stage pair, native Vulkan hardware, cross-platform CI, and compiler-session build-cost work.

## Decision criteria

Adopt Slang as the default when all current production capabilities are covered, difficult shader outputs are equivalent, no material GPU regression remains, cross-platform compilation is reproducible, and compiler-session integration makes normal iteration acceptable.

Keep a mixed production build if Slang is clearly beneficial for most modules but one or two Vulkan-specific shaders require fragile workarounds. Retain GLSL as default if the tracer or construction passes regress materially, correctness depends on compiler-version-specific behavior, or CI/toolchain reliability remains worse than the maintainability benefit.
