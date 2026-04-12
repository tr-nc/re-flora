# Terrain Edit Voxel Readback Plan

## Goal

For terrain voxel removal edits, capture updated voxel world positions on GPU, randomly choose up to 50 of them with equal chance, read those 50 positions back on CPU, and use them to guide voxel destruction particle spawn positions.

## Constraints

- Keep the CPU readback buffer fixed at `len = 50`.
- Only implement this for the voxel removal path for now.
- Use a 2-pass GPU approach.
- Reuse existing removal stats instead of adding a separate candidate counter.

## Existing Useful State

- Terrain removal currently runs through `chunk_modify.comp`.
- Successful removals already increment `edit_stats.added_counts[VOXEL_TYPE_EMPTY]`.
- For removal, that value is the total number of voxels affected by the dispatch.
- World-space voxel center for a removed voxel is:

```glsl
(vec3(world_voxel_pos) + vec3(0.5)) / 256.0
```

## Pass 0: Collect All Removal Candidate Positions

### Output

A large GPU-only candidate buffer that stores every removed voxel position for the dispatch.

### How it works

When a voxel removal succeeds in `chunk_modify.comp`:

1. Use the existing atomic add on `edit_stats.added_counts[VOXEL_TYPE_EMPTY]` as the append index.
2. Write the removed voxel world-space center to `candidate_positions[idx]`.
3. Keep the normal edit stats behavior unchanged.

### Notes

- No separate `candidate_count` buffer is needed.
- Pass 1 will use `edit_stats.added_counts[VOXEL_TYPE_EMPTY]` as the candidate length.
- The candidate buffer still needs a fixed capacity and a bounds check before writing.
- If capacity is exceeded, extra candidates can be dropped for now.

## Pass 1: Sample Up To 50 Positions In Parallel

### Input

- `candidate_positions[0..candidate_len)` from pass 0
- `candidate_len = edit_stats.added_counts[VOXEL_TYPE_EMPTY]`

### Output

A GPU-to-CPU readback buffer containing:

- `sample_count`
- `sample_positions[50]`

### Sampling rule

Use a simple parallel sampling pass with replacement:

1. Launch 50 shader invocations.
2. Each invocation independently generates a random index in `[0, candidate_len)`.
3. Each invocation reads that candidate position from the large candidate buffer.
4. Each invocation writes its result into the corresponding slot in the fixed sample buffer.

This keeps pass 1 fully parallel and keeps the CPU readback buffer fixed at 50 positions.

### Consequence

- Each draw is uniform over all valid candidate indices.
- Sampling is with replacement.
- Duplicate sampled positions are allowed.
- The output is intended to guide particles, so duplicate anchors are acceptable for this first implementation.

### Initial implementation choice

Prefer a simple pass-1 shader first:

- local size or total invocation count of 50
- one invocation per output slot
- each invocation hashes `(edit_seed, invocation_id)` to generate a random candidate index
- invocation 0 writes `sample_count = min(candidate_len, 50)`
- invocations beyond `sample_count` clear their output slot

## CPU Side Integration

### Builder layer

Extend the terrain removal result path so CPU can read back:

- edit stats
- sampled removal positions

The builder should:

1. Clear the candidate buffer before pass 0.
2. Run the existing modify pass.
3. Run the new sampling pass.
4. Read back the fixed sample buffer.

### App layer

Update terrain removal particle spawning so it uses sampled voxel positions instead of only the edit center.

Proposed behavior:

- if sampled positions exist, use them as particle spawn anchors
- if fewer than desired particles are available, reuse/jitter sampled anchors as needed
- if no sampled positions exist, fall back to the edit center

## Buffers To Add

### GPU-only candidate buffer

Stores all removed voxel positions for the dispatch.

Suggested shape:

```glsl
layout(set = 0, binding = X) buffer B_EditRemovalCandidates {
    vec4 positions[];
} edit_removal_candidates;
```

Use `vec4` for alignment safety. `xyz` stores world position.

### CPU-visible sample buffer

Stores the final up-to-50 sampled positions.

Suggested shape:

```glsl
layout(set = 0, binding = Y) buffer B_EditRemovalSample {
    uint sample_count;
    vec4 positions[50];
} edit_removal_sample;
```

## Seed Source

Pass 1 should use a per-edit random seed so repeated edits do not always choose the same sampled positions.

Possible sources:

- an incrementing terrain edit counter
- frame counter
- hashed edit center + counter

An incrementing terrain edit counter is likely the simplest option.

## First Implementation Scope

- removal only
- 2-pass GPU path
- fixed `50`-entry CPU readback buffer
- use sampled positions only for terrain destruction particles
- no placement/flora edit integration yet

## Follow-up Validation

After implementation, verify:

1. removed voxel candidates are written correctly in pass 0
2. pass 1 returns at most 50 positions
3. repeated edits produce varying sampled positions
4. particles spawn around actual removed voxels rather than only the edit center
