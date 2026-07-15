# Slang shader proof of concept

This experiment incrementally replaces selected GLSL compute shaders with equivalent Slang implementations. The normal build remains GLSL-only, and each replacement can be enabled independently for matched comparison.

The first pass, `shader/tracer/post_processing.comp`, was selected because it runs every frame at the full output resolution, already has a Vulkan timestamp scope, and exercises a uniform buffer, formatted storage images, bounds checks, and a reusable dither module. The surface and contree-leaf passes cover difficult shared-memory, synchronization, atomic, and structured-buffer paths. The main tracer is also compiled from its existing GLSL source through Slang's GLSL frontend to isolate backend compatibility and optimization.

## Requirements

Install `slangc` and make it available through one of:

1. the `SLANGC` environment variable,
2. `$VULKAN_SDK/bin/slangc`, or
3. `PATH`.

The initial validation used Vulkan SDK 1.4.321.0 and Slang `2025.11-12-gc5295eae2` on macOS. Native Slang sources pin the Slang 2025 language rules and column-major matrices. The GLSL frontend uses row-major lowering to preserve the existing GLSL std140 matrix bytes. Both paths emit SPIR-V 1.6 with Vulkan GL-compatible buffer layout.

## Build and validation

The default build does not invoke Slang:

```bash
cargo check
```

Enable an individual replacement or all completed validation candidates with:

```bash
cargo check --features slang-post-processing
cargo check --features slang-surface
cargo check --features slang-contree-leaf
cargo check --features slang-tracer-backend
cargo check --features slang-tracer-shadow
cargo check --features slang-validation
```

`slang-poc` remains a backward-compatible alias for `slang-post-processing`.

The build emits a summary such as:

```text
precompiled 75 GLSL shaders and 1 Slang shaders into SPIR-V artifacts
```

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

A 20-run warm compiler-process measurement produced median times of approximately 47.7 ms for `glslc -O` and 178.9 ms for `slangc -O3`. This is not the full Cargo build cost, but it confirms that launching one `slangc` process per artifact would scale poorly. Any broader migration should compile multiple entry points in one Slang session or use the compiler API and serialized modules.

Both matched run logs contained the same pre-existing validation warning that one pipeline layout exposes nine storage images on hardware reporting an eight-image per-stage limit. The Slang replacement did not introduce or change that warning.

## Surface extraction and normal generation

The second candidate replaces `shader/builder/surface/make_surface_sparse.comp` with `shader/experiments/slang/make_surface_sparse.slang` under `slang-surface`. It validates:

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
  cargo run --release --features slang-surface -- --hidden --mute \
  --tree-bench --tree-bench-samples 50 --no-tracer --no-shadows \
  --no-denoise --no-god-rays --no-lens-flare --no-clouds --no-flora \
  --no-particles

scripts/compare_surface_pass.py <glsl-log> <slang-log>
```

The final optimized surface artifact contained 498 SPIR-V instructions from GLSL and 508 from Slang after debug stripping. The remaining difference is small enough that generated-code inspection alone does not explain the run-order-sensitive tail.

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

## Native Slang traversal modules

The `slang-tracer-shadow` feature replaces production `shader/tracer/tracer_shadow.comp` with a native Slang entry point. Its reusable modules cover AABB intersection, camera-ray projection, contree traversal, DDA scene traversal, marching results, and voxel decoding. This exercises the branch-heavy core shared conceptually with the main tracer without relying on `-allow-glsl`.

The native path preserves all five descriptor bindings, the 400-byte camera uniform, the 64-invocation workgroup stack, std430 contree buffers, and read/write storage image formats. A 5120x2880 fixed-camera comparison showed equivalent terrain and object shadows; normal floating-point, moving-object, and UI differences prevented byte identity.

Two order-reversed local MoltenVK timing pairs measured `tracer_shadow.pass`:

| Run order | Mean delta | Median delta |
| --- | ---: | ---: |
| GLSL then native Slang | -1.94% | -1.37% |
| Native Slang then GLSL | +1.47% | -0.78% |

Typical GPU time is at parity on the tested Apple M4 Pro. The debug-stripped optimized artifact contains 736 instructions/12,796 bytes from shaderc and 815 instructions/14,168 bytes from Slang. Native Vulkan performance remains a TODO for when Windows/Linux hardware is available.

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

## Compatibility findings

Slang names SPIR-V buffer-layout wrapper types with suffixes such as `_std140`, while existing GLSL reflection exposes source type names directly. Its GLSL frontend additionally adds the `SLANG_ParameterGroup_` prefix and materializes matrices as `_MatrixStorage_*` structs. The runtime normalizes these forms at the reflection boundary, allowing both frontends to resolve the same resource definitions and matrix members without shader-specific aliases.

## Limits of this result

The compute candidates establish Slang compatibility with the current shared-memory, barrier, atomic, storage-image, runtime-array, structured-SSBO, matrix, and branch-heavy traversal patterns on MoltenVK. Native Slang modules now cover the core contree/DDA shadow traversal; the full main tracer result still evaluates Slang code generation through its GLSL frontend. Graphics stages and combined-session compiler performance remain unvalidated. Native Vulkan performance and cross-platform CI are deferred until suitable Windows/Linux hardware is available. The current source tree does not actively use buffer references or Vulkan sparse-residency intrinsics; those should be tested if introduced later.
