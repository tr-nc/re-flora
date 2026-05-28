# Sampled Image Refactor Plan

## Goal

Fix macOS/MoltenVK descriptor-limit validation errors by converting read-only storage-image shader inputs to sampled images where appropriate.

Branch: `agent/sampled-image-refactor`

No separate worktree is used for this pass.

## Background

The latest macOS hidden run succeeds, but validation reports pipeline layout errors like:

```text
vkCreatePipelineLayout(): max per-stage storage image bindings count (22) exceeds device maxPerStageDescriptorStorageImages limit (8).
vkCreatePipelineLayout(): max per-stage storage image bindings count (14) exceeds device maxPerStageDescriptorStorageImages limit (8).
```

On the tested Apple M4 Pro / MoltenVK path, relevant limits are:

```text
maxPerStageDescriptorSamplers       = 16
maxPerStageDescriptorSampledImages  = 128
maxPerStageDescriptorStorageImages  = 8
```

The renderer currently declares many read-only textures as GLSL `image*` objects:

```glsl
layout(..., r32ui) readonly uniform uimage2D some_tex;
uint value = imageLoad(some_tex, uv).r;
```

Even with `readonly`, GLSL `image*` maps to `VK_DESCRIPTOR_TYPE_STORAGE_IMAGE`, so each binding counts against the small storage-image limit. Read-only texture inputs should generally use sampled-image descriptors instead:

```glsl
layout(set = ..., binding = ...) uniform usampler2D some_tex;
uint value = texelFetch(some_tex, uv, 0).r;
```

Use `texelFetch`, not `texture`, for exact integer-coordinate reads that replace `imageLoad`.

## Root Cause

The renderer treats many read-only texture inputs as storage images. This is portable enough on GPUs with generous storage-image limits, but not on Apple/MoltenVK where `maxPerStageDescriptorStorageImages` is 8.

The issue is descriptor classification, not simply texture count.

## Goals

- Bring every shader stage under the per-stage storage image limit on macOS/MoltenVK.
- Preserve existing rendering behavior.
- Keep exact texel addressing for denoiser and blue-noise reads.
- Keep true read-write or write-only outputs as storage images.
- Avoid large renderer architecture changes in this pass.
- Leave benchmarks and validation evidence in this document after implementation.

## Non-Goals

- Do not redesign denoising, tracing, or composition algorithms.
- Do not change texture formats unless validation requires it.
- Do not claim performance improvements without release-mode measurements.
- Do not split render passes unless sampled-image conversion is insufficient.

## Initial Suspects

Likely offending shaders:

- `shader/tracer/tracer.comp`
  - Declares scene texture, output images, blue-noise images, and full denoiser texture set as storage images.
  - Some denoiser declarations appear unused by this shader and can be removed before conversion.
- `shader/denoiser/temporal.comp`
  - Reads many denoiser history/current textures and writes a smaller subset.
- `shader/denoiser/spatial.comp`
  - Reads many denoiser textures and writes ping-pong / accumulated outputs.

Secondary candidates after the main errors are fixed:

- `shader/tracer/god_ray.comp` blue-noise inputs.
- `shader/tracer/composition.comp` read-only compute/depth/lens-flare inputs.
- VSM blur/creation read-only inputs.

## Implementation Plan

### Step 0: Descriptor Inventory

Produce a before/after count of storage-image and sampled-image descriptors per shader.

Record at least:

- shader path
- storage image count
- sampled image count
- write-capable storage bindings
- read-only storage bindings that can be converted

Expected current high counts:

```text
shader/tracer/tracer.comp             storage images: ~22
shader/denoiser/temporal.comp         storage images: ~14
shader/denoiser/spatial.comp          storage images: ~14
```

Inventory helper added:

```bash
python3 scripts/descriptor_inventory.py \
  shader/tracer/tracer.comp \
  shader/denoiser/temporal.comp \
  shader/denoiser/spatial.comp
```

Baseline source inventory:

| shader | storage | sampled | storage read-only/dead | storage write-capable |
| --- | ---: | ---: | ---: | ---: |
| `shader/tracer/tracer.comp` | 22 | 1 | 15 | 7 |
| `shader/denoiser/temporal.comp` | 14 | 0 | 12 | 2 |
| `shader/denoiser/spatial.comp` | 14 | 0 | 11 | 3 |

Step 0 status: done. This source-based inventory matches the validation-error scale and identifies the first cleanup targets.

### Step 1: Remove Unused Storage Image Declarations

Start with `shader/tracer/tracer.comp`.

Keep only denoiser textures that are actually written by the trace pass, such as:

- `compute_output_tex`
- `compute_depth_tex`
- `denoiser_normal_tex`
- `denoiser_position_tex`
- `denoiser_vox_id_tex`
- `denoiser_motion_tex`
- `denoiser_hit_tex`

Remove declarations from `tracer.comp` that are not referenced by the shader body.

This step should reduce descriptor pressure with minimal behavior risk.

Step 1 status: done for `shader/tracer/tracer.comp`.

After removing unused denoiser declarations from the trace shader:

| shader | storage | sampled | storage read-only/dead | storage write-capable |
| --- | ---: | ---: | ---: | ---: |
| `shader/tracer/tracer.comp` | 14 | 1 | 7 | 7 |
| `shader/denoiser/temporal.comp` | 14 | 0 | 12 | 2 |
| `shader/denoiser/spatial.comp` | 14 | 0 | 11 | 3 |

Validation run:

```text
cargo check
```

### Step 2: Convert Read-Only Storage Images to Sampled Images

For exact read-only lookups, replace:

```glsl
layout(..., r32ui) readonly uniform uimage2D tex;
imageLoad(tex, uv).r;
```

with:

```glsl
layout(set = ..., binding = ...) uniform usampler2D tex;
texelFetch(tex, uv, 0).r;
```

Type mapping reminders:

- unsigned integer images: `usampler2D`, `usampler3D`, `usampler2DArray`
- signed integer images: `isampler*`
- float/normalized images: `sampler*`
- array coordinates for `sampler2DArray`: `ivec3(x, y, layer)` with `texelFetch`

Good first targets:

- blue-noise texture arrays in `shader/include/noise_tex.glsl`
- read-only denoiser current/history textures in temporal/spatial passes
- read-only depth/color inputs in post-denoiser passes if needed

### Step 3: Update Texture Usage Flags

Any texture sampled in any shader must include:

```rust
vk::ImageUsageFlags::SAMPLED
```

Textures that are both written by one pass and sampled by another should include both:

```rust
vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED
```

Likely files:

- `src/tracer/resources.rs`
- `src/tracer/denoiser_resources.rs`

### Step 4: Descriptor Layout / Auto Binding Compatibility

Reflection should map `sampler*` declarations to `VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER` or sampled-image descriptors depending on SPIR-V reflection.

Confirm the existing auto-binding path handles the reflected sampled descriptor type:

- `crates/re-flora-vkn/src/shader/shader_module.rs`
- `crates/re-flora-vkn/src/pipeline/descriptor_set_utils.rs`
- `crates/re-flora-vkn/src/descriptor/descriptor_set.rs`

Current `WriteDescriptorSet::new_texture_write` already receives `binding.descriptor_type`, so the main task should be shader/resource declaration alignment rather than new descriptor plumbing.

### Step 5: Layout Transition Follow-Up

First implementation may keep sampled reads in `VK_IMAGE_LAYOUT_GENERAL` if that is already how the texture is transitioned and validation accepts it.

After correctness is established, consider transitioning read-only sampled phases to:

```rust
vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
```

Only do this as a follow-up if it remains small and safe. It may improve performance, but it is not required for the first descriptor-limit fix.

## Validation Plan

Run after each meaningful step:

```bash
cargo fmt --check
cargo check
```

Before handoff:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Acceptance criteria:

- No `maxPerStageDescriptorStorageImages` validation errors.
- No other `[Validation]` errors introduced.
- App exits successfully in hidden release run.
- Denoiser/tracer output remains visually plausible.
- Storage image count per offending shader is at or below 8 on macOS/MoltenVK.

## Benchmark Plan

Use release-mode hidden runs. Debug builds and unit tests are not performance evidence.

Suggested commands:

```bash
cargo run --release -- --hidden --auto-exit 4 --perf
cargo run --release -- --tail-latest-log 200
```

Run enough repeats to separate real changes from noise, ideally 3 baseline runs and 3 post-change runs on the same machine/settings.

Track:

- total frame time
- `gpu_present` / GPU-visible frame time if available
- denoiser-related timing if available
- any shader/pass timestamp data if instrumentation exists or is added
- validation messages
- visual correctness notes

## Benchmark Results

Fill this after implementation.

### Baseline

- Date:
- Machine/GPU:
- Commit:
- Command(s):
- Log path(s):
- Storage image counts:
- Frame/perf summary:
- Validation summary:
- Visual notes:

### After Refactor

- Date:
- Machine/GPU:
- Commit:
- Command(s):
- Log path(s):
- Storage image counts:
- Frame/perf summary:
- Validation summary:
- Visual notes:

### Comparison

- Frame time delta:
- GPU/pass timing delta:
- Validation delta:
- Known caveats:

## Open Questions

- Does SPIR-V reflection report GLSL `sampler*` as `COMBINED_IMAGE_SAMPLER` in all affected shaders?
- Can all sampled replacements safely use `texelFetch`, or do any current reads rely on storage-image-specific format behavior?
- Should scene texture reads become sampled in this pass, or only if needed after denoiser/blue-noise cleanup?
- Should read-only sampled resources transition to `SHADER_READ_ONLY_OPTIMAL` immediately or in a separate follow-up?
