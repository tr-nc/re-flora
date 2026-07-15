# Slang shader proof of concept

This experiment replaces only `shader/tracer/post_processing.comp` with an equivalent Slang compute shader. The normal build remains GLSL-only; the replacement is enabled with the `slang-poc` Cargo feature.

The pass was selected because it runs every frame at the full output resolution, already has a Vulkan timestamp scope, and exercises a uniform buffer, formatted storage images, bounds checks, and a reusable dither module.

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

Enable the replacement shader with:

```bash
cargo check --features slang-poc
cargo run --release --features slang-poc -- --hidden --mute --auto-exit 8 --perf
```

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

## Compatibility finding

Slang names SPIR-V buffer-layout wrapper types with suffixes such as `_std140`, while existing GLSL reflection exposes source type names directly. The runtime now normalizes known Slang layout suffixes at the reflection boundary, allowing both frontends to resolve the same resource definitions without shader-specific aliases.

## Limits of this result

This validates one small, full-screen compute shader only. It does not establish suitability for the tracer, shared-memory builders, atomics, buffer references, sparse image operations, or graphics stages. The next useful experiment should port one branch-heavy shader and one shared-memory/atomic shader, then repeat correctness checks and GPU timestamp measurements on native Vulkan hardware as well as MoltenVK.
