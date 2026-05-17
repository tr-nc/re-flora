# Terrain Visual Rebuild Pipeline and Atomic Publish Plan

## Goal

This document explains the current terrain rebuild implementation and the next target design.

The current gameplay requirement is intentionally narrow:

- **Must guarantee visible terrain does not crack across chunk boundaries.**
- Water terrain SDF, water cache, CPU/audio terrain cache, and flora can update later.
- It is acceptable for the visible terrain to stay old for a few extra frames.
- It is not acceptable to show one visible chunk with new geometry while an adjacent visible chunk still shows old geometry when they belong to the same edit batch.

In this project, the visible terrain is not a conventional triangle mesh. The tracer reads a per-chunk scene indirection texture:

```text
scene_tex[chunk] -> contree_node_data / contree_leaf_data offsets -> ray/voxel traversal
```

So the practical visual atomicity target is:

```text
For all chunks affected by one visible edit batch, scene_tex entries become visible together.
```

## Current implementation

### Source data

The terrain edit source of truth is the voxel atlas owned by `PlainBuilder`.

A terrain edit normally does this:

```text
voxel edit -> compute affected chunks -> enqueue terrain rebuild work
```

The affected chunk calculation is done through `world_ops::affected_chunk_indices_for_bound(...)` / `get_affected_chunk_indices(...)`.

### Rebuild stages

A visible terrain chunk rebuild currently has three GPU-facing stages:

```text
Surface build -> Contree build/alloc -> Scene texture update
```

#### 1. Surface build

Implemented by `SurfaceBuilder`.

It samples the voxel atlas for one chunk and writes intermediate per-chunk surface data into shared surface resources. It can also regenerate flora instance data when requested.

Important current detail:

- The surface builder uses shared scratch resources.
- Only one terrain surface rebuild should be in flight through this path at a time.

#### 2. Contree build and allocation

Implemented by `ContreeBuilder`.

It consumes the surface data and builds compact tree data into:

```text
contree_node_data
contree_leaf_data
```

The current allocator is a logical allocator over pre-created GPU buffers:

```rust
contree_node_data: 512 MiB
contree_leaf_data: 512 MiB
```

Each build currently reserves worst-case per-chunk space first:

```rust
MAX_NODE_BUFFER_SIZE_IN_BYTES = 10 MiB
MAX_LEAF_BUFFER_SIZE_IN_BYTES = 10 MiB
```

Then `finish_build_and_alloc()` reads the actual generated size and shrinks/confirms the allocation.

Current important behavior:

- The current finish path publishes allocator ownership immediately.
- It may deallocate the old chunk allocation before the new visual scene entry is logically treated as a batch commit.
- This is fine for single-chunk or synchronous rebuilds, but it is not enough for future multi-chunk atomic publish.

#### 3. Scene texture update

Implemented by `SceneAccelBuilder`.

The current shader `shader/builder/scene_accel/update_scene_tex.comp` writes one texel:

```glsl
scene_tex[chunk_idx] = uvec4(node_offset + 1, leaf_offset + 1, 0, 0)
```

A zero value means the chunk is empty/invalid.

The tracer reads `scene_tex` to decide which contree offset to use for a chunk. Therefore, changing `scene_tex` is the moment a chunk's new visible terrain becomes observable.

### Current execution paths

#### Single-chunk rebuild

Single-chunk rebuilds use the staged async path in `App::process_deferred_chunk_rebuild()`:

```text
frame A: submit surface
later: finish surface, submit contree
later: finish contree, submit scene_tex update
later: finish scene_tex update, schedule terrain SDF source refresh
```

The same chunk remains serial. Different subsystem work can continue on later frames.

This path reduces frame spikes and is still acceptable because a single chunk cannot create a cross-chunk half-new/half-old visual seam by itself.

#### Small multi-chunk visible rebuild

After the visible seam issue was found, small multi-chunk rebuilds were changed to a quality-first synchronous fallback:

```text
2..=8 chunks -> drain any active async rebuild -> rebuild all chunks synchronously -> publish in one call
```

This is controlled by:

```rust
SYNC_VISIBLE_REBUILD_CHUNK_LIMIT: usize = 8
```

This fallback intentionally trades a short frame spike for visual correctness.

Measured example from the hidden perf run:

```text
8-chunk sync visible rebuild: 35.32 ms
```

That cost is accepted for now because the alternative was visible cross-chunk cracking.

#### Larger multi-chunk rebuild

Larger batches still use the deferred path. This is less ideal visually, but the common visible edit case is expected to be within the small-batch limit. The long-term solution below should remove the need for this split.

### Terrain SDF / water follow-up

After visible terrain publish, the app schedules:

```text
Terrain SDF Source Refresh -> Terrain SDF Collider Build -> Water Terrain Cache Rebuild
```

Those stages are already decoupled and revision-guarded. For the visual requirement in this document, they are allowed to lag behind the visible terrain publish.

## Current problem

The P1 async staged rebuild improved frame smoothness, but per-chunk scene publishing made cross-chunk edits visually inconsistent:

```text
chunk A scene_tex entry points to new contree
chunk B scene_tex entry still points to old contree
=> visible crack/seam at boundary
```

The temporary synchronous fallback fixes the visual issue for small batches, but it can reintroduce edit-time spikes.

The next target is to keep the visual correctness guarantee without blocking the main frame for the whole batch rebuild.

## Target design: visual atomic batch publish

### Expected behavior

For one visible edit batch affecting multiple chunks:

```text
frame N: all affected chunks still render old terrain
frame N+k: all affected chunks render new terrain together
```

There should never be a rendered frame where only part of the affected visible batch is published.

### Scope of atomicity

Atomicity is only required for visible terrain data:

```text
scene_tex + the contree data referenced by scene_tex
```

The following can update after the visual commit:

- Terrain SDF source refresh.
- Terrain SDF collider build.
- Water terrain cache rebuild.
- CPU/audio contree cache.
- Flora-specific follow-up, unless it becomes visibly problematic later.

### No special slow-chunk partial publish

If one chunk in the batch is slow, the batch remains visually old until all chunks are ready.

No timeout partial publish should be used if it can create a visible seam.

## Recommended implementation plan

### 1. Introduce a visual rebuild batch state

Add an app-level batch object, conceptually:

```rust
struct VisualTerrainRebuildBatch {
    batch_id: u64,
    chunk_ids: Vec<UVec3>,
    per_chunk_revision: HashMap<UVec3, u64>,
    chunks: Vec<VisualTerrainChunkBuildState>,
    created_at: Instant,
}
```

The batch is created from one edit's affected chunks.

Rules:

- Single-chunk work can continue using the existing async staged path.
- Multi-chunk visible work should use this batch path.
- If a newer edit touches a chunk already in a batch, the old batch result must not publish stale data for that chunk.
- A simple first version can cancel/stale the older batch and enqueue a new batch.

### 2. Build chunks asynchronously, but do not publish `scene_tex`

For each chunk in the batch:

```text
submit surface
finish surface
submit contree
finish contree into pending/staging data
```

Important difference from current `finish_build_and_alloc()`:

```text
Do not update scene_tex per chunk.
Do not make the new chunk visually observable yet.
Do not release old visible contree allocation while scene_tex still points at it.
```

This likely requires splitting contree finish into two concepts:

```text
finish build as pending allocation
commit pending allocation as visible allocation
```

### 3. Batch update `scene_tex` in one render command buffer

Current one-chunk scene update uses one uniform and one dispatch. For atomic batch publish, prefer a storage-buffer based update:

```rust
struct SceneTexBatchUpdateEntry {
    chunk_idx: [u32; 3],
    node_offset: u32,
    leaf_offset: u32,
    is_valid: u32,
}
```

New shader shape:

```glsl
layout(local_size_x = 64) in;

layout(set = 0, binding = 0) readonly buffer B_SceneTexBatchUpdates {
    SceneTexBatchUpdateEntry entries[];
};

layout(set = 0, binding = 1) uniform U_SceneTexBatchUpdateInfo {
    uint update_count;
};

layout(set = 0, binding = 2, rg32ui) writeonly uniform uimage3D scene_tex;

void main() {
    uint i = gl_GlobalInvocationID.x;
    if (i >= update_count) {
        return;
    }

    SceneTexBatchUpdateEntry e = entries[i];
    if (e.is_valid == 0u) {
        imageStore(scene_tex, ivec3(e.chunk_idx), uvec4(0u));
    } else {
        imageStore(scene_tex, ivec3(e.chunk_idx),
            uvec4(e.node_offset + 1u, e.leaf_offset + 1u, 0u, 0u));
    }
}
```

The render frame command order should be:

```text
record scene_tex batch update
insert compute shader write -> compute shader read barrier
record tracer pass
```

This makes the publish atomic from the renderer's point of view: no tracer pass can observe the batch halfway through the scene texture update.

### 4. Resource lifetime rules

The atomic publish path must maintain two generations for affected chunks while a batch is preparing:

```text
old visible generation: currently referenced by scene_tex
new pending generation: built but not yet visible
```

Only after the batch scene texture update has been submitted/ordered safely can the CPU-side visible mapping switch to the new generation and the old generation become releasable.

A conservative rule:

```text
old contree allocations are released only after the GPU frame containing the scene_tex batch update has completed
```

This avoids reusing memory while an earlier frame might still be tracing against the old `scene_tex` entries.

Because `MAX_FRAMES_IN_FLIGHT` is currently 1, this is simpler than it would be with multiple frames in flight, but the design should still keep explicit delayed-free ownership.

## Fixed staging slots option

Fixed staging slots can reduce memory compared to full double buffering.

### Full double-buffer comparison

The current world has:

```text
CHUNK_DIM = 5 * 2 * 5 = 50 chunks
```

Worst-case full double buffering would require one extra maximum allocation for every chunk:

```text
50 * (10 MiB node + 10 MiB leaf) = 1000 MiB extra
```

### Fixed staging slot upper bound

If the atomic batch limit is 8 chunks, fixed staging slots need only:

```text
8 * 10 MiB node = 80 MiB node staging
8 * 10 MiB leaf = 80 MiB leaf staging
160 MiB total staging
```

That is much smaller than full double buffering.

### Two ways to use staging slots

#### Option A: staging slot becomes the visible allocation

Flow:

```text
build into slot
atomic scene_tex publish points at slot offset
old visible allocation is delayed-freed
slot is no longer free because it became visible data
```

This is simple and avoids a copy, but effectively requires enough fixed slots for both visible chunks and pending batch chunks if every visible chunk is also slot-backed.

Worst-case pool sizing becomes approximately:

```text
(CHUNK_COUNT + MAX_BATCH_SIZE) * max_chunk_size
```

For the current constants:

```text
node: (50 + 8) * 10 MiB = 580 MiB
leaf: (50 + 8) * 10 MiB = 580 MiB
```

Current pools are 512 MiB each, so strict worst-case support would require increasing them.

#### Option B: fixed staging slots plus compact final copy

Flow:

```text
build into fixed staging slot
read actual node/leaf sizes
allocate compact final region using actual size
GPU copy staging -> final region
when all batch chunks are ready, atomic scene_tex publish points at final regions
old visible allocations are delayed-freed
staging slots are reusable
```

This can use less memory when actual chunk contrees are much smaller than the 10 MiB worst-case limit.

Tradeoff:

- Lower memory pressure.
- More implementation complexity.
- Requires an extra GPU buffer copy and copy/compute barriers.

### Recommendation

For a first robust implementation, prefer the simpler route unless memory pressure becomes a real problem:

```text
fixed-size pending capacity + delayed free + atomic scene_tex batch publish
```

If memory use becomes too high, evolve to:

```text
fixed staging slots + compact final copy
```

The visual atomicity mechanism is the same in both cases: the batch becomes visible only when `scene_tex` is updated for all affected chunks together.

## Validation plan

Functional validation:

- Reproduce an edit crossing a chunk boundary.
- Confirm frames show either all-old or all-new affected chunks.
- Confirm no persistent or transient crack appears between affected chunks.

Logging validation:

- Log batch creation with chunk IDs and revisions.
- Log per-chunk surface/contree readiness.
- Log batch commit with all scene texture entries.
- Log delayed free of old contree allocations.
- Log stale batch discard when a newer edit supersedes an older batch.

Performance validation:

Use release hidden runs and visible manual runs:

```bash
cargo run --release -- --hidden --auto-exit 6 --perf --water-profile performance --water-edit-soak --water-particles 128
```

Track:

- Frame `deferred_rebuild` cost.
- Batch wait time from edit to visual commit.
- Batch scene publish cost.
- Old allocation delayed-free count/bytes.
- Whether sync visible fallback is still needed.

## End state

The desired end state is:

```text
single chunk edits:
  async staged rebuild, low frame cost

multi-chunk visible edits:
  async staged build into pending data
  no partial visual publish
  one atomic scene_tex batch publish before tracing
  old contree data released only after safe GPU completion

water / collider / CPU cache:
  revision-guarded follow-up after visual commit
```

This should preserve the main performance win from async rebuilds while guaranteeing the visual terrain never cracks across chunk boundaries during ordinary edits.
