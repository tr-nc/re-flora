# Slang shader proof of concept

This experiment incrementally replaces selected GLSL compute shaders with equivalent Slang implementations. The normal build remains GLSL-only, and each replacement can be enabled independently for matched comparison.

The first pass, `shader/tracer/post_processing.comp`, was selected because it runs every frame at the full output resolution, already has a Vulkan timestamp scope, and exercises a uniform buffer, formatted storage images, bounds checks, and a reusable dither module. The second pass, `shader/builder/surface/make_surface_sparse.comp`, covers the difficult shared-memory normal extraction path.

## Requirements

Install `slangc` and make it available through one of:

1. the `SLANGC` environment variable,
2. `$VULKAN_SDK/bin/slangc`, or
3. `PATH`.

The initial validation used Vulkan SDK 1.4.321.0 and Slang `2025.11-12-gc5295eae2` on macOS. The build pins the Slang 2025 language rules and emits SPIR-V 1.6 with column-major matrices and Vulkan GL-compatible buffer layout.

## Build and validation

The default build does not invoke Slang:

```bash
cargo check
```

Enable an individual replacement or all completed validation candidates with:

```bash
cargo check --features slang-post-processing
cargo check --features slang-surface
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

## Compatibility finding

Slang names SPIR-V buffer-layout wrapper types with suffixes such as `_std140`, while existing GLSL reflection exposes source type names directly. The runtime now normalizes known Slang layout suffixes at the reflection boundary, allowing both frontends to resolve the same resource definitions without shader-specific aliases.

## Limits of this result

The surface candidate establishes Slang compatibility with the current shared-memory, barrier, atomic, storage-image, and runtime-array pattern. It does not yet establish suitability for contree's shared prefix allocation, the branch-heavy tracer and its include graph, or graphics stages. The current source tree does not actively use buffer references or Vulkan sparse-residency intrinsics; those should be tested if they are introduced later. The next experiment is `shader/builder/contree/leaf_write.comp`, followed by the dominant contree tree pass and `shader/tracer/tracer.comp`.
