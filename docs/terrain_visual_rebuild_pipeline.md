# Terrain Visual Rebuild Pipeline

## Decision

Visible terrain chunk rebuilds should stay synchronous.

The design priority is:

```text
visual correctness and simple code > async visual terrain rebuild throughput
```

For visible terrain, the user must never see neighboring chunks in different visual revisions. It is acceptable to pay an edit-frame spike. It is not acceptable to show a chunk seam/crack caused by one chunk publishing before another.

The queued/async pipeline still applies to non-visual follow-up work:

```text
Terrain SDF Source Refresh -> Terrain SDF Collider Build -> Water Terrain Cache Rebuild
```

Those systems can lag behind the visible terrain because they do not directly determine the terrain image shown in the current frame.

## What counts as visible terrain

This project does not render terrain from a conventional triangle mesh. The tracer uses a per-chunk scene indirection texture:

```text
scene_tex[chunk] -> contree_node_data / contree_leaf_data offsets -> ray/voxel traversal
```

A chunk becomes visually updated when its `scene_tex` entry points at the newly built contree data.

Therefore the visual correctness rule is:

```text
Do not let rendering observe a partially updated group of affected chunks.
```

The simplest way to guarantee this is to rebuild and publish the affected visible chunks synchronously before the next rendered frame.

## Current synchronous visual rebuild path

A terrain edit normally follows this path:

```text
voxel edit
-> compute affected chunks
-> synchronously rebuild all affected visible chunks
-> update scene_tex entries during the same blocking rebuild call
-> schedule async/non-visual follow-up work
```

The visible rebuild itself has three stages per chunk:

```text
Surface build -> Contree build/alloc -> Scene texture update
```

### 1. Surface build

Implemented by `SurfaceBuilder`.

It samples the voxel atlas and writes per-chunk surface data into shared surface resources. It may also generate flora instance data depending on the edit path.

Important detail:

- The surface resources are shared scratch resources.
- Keeping the visible rebuild synchronous avoids multiple surface jobs contending for the same scratch state.

### 2. Contree build and allocation

Implemented by `ContreeBuilder`.

It builds compact tree data into pre-created GPU buffers:

```text
contree_node_data
contree_leaf_data
```

The buffers are created up front. Rebuild-time allocation is logical suballocation, not Vulkan buffer creation:

```rust
contree_node_data: 512 MiB
contree_leaf_data: 512 MiB
MAX_NODE_BUFFER_SIZE_IN_BYTES = 10 MiB
MAX_LEAF_BUFFER_SIZE_IN_BYTES = 10 MiB
```

The current build reserves worst-case space, then shrinks/confirms the allocation after the generated size is known.

### 3. Scene texture update

Implemented by `SceneAccelBuilder`.

The current shader writes one chunk entry:

```glsl
scene_tex[chunk_idx] = uvec4(node_offset + 1, leaf_offset + 1, 0, 0)
```

A zero entry means empty/invalid chunk.

Because the entire visible rebuild call blocks, no frame is rendered halfway through a multi-chunk rebuild. Even if chunks are updated one by one internally, the user only sees the state after the synchronous call completes.

That gives the visible behavior we want:

```text
before edit frame: all old chunks
next rendered frame: all affected chunks updated
```

Not:

```text
chunk A new, chunk B old
```

## Why we are not pursuing async visual atomic publish now

An async visual atomic publish design would require extra staging state, delayed frees, scene texture batch updates, and more complex revision handling.

That design can reduce edit-frame spikes, but it makes the terrain rebuild system significantly more complicated.

Current decision:

```text
Do not add visual async staging / atomic publish unless synchronous visible rebuilds become unacceptable in real gameplay.
```

For now, keep visible terrain rebuilds simple and synchronous.

## What remains queued/asynchronous

The following work remains queue-based and revision-guarded:

### Terrain SDF Source Refresh

This samples the visual/voxel terrain into CPU-side solid voxel data for later collider construction.

It can lag because visual terrain has already been published. Results are revision-guarded so stale work cannot overwrite newer state.

### Terrain SDF Collider Build

This builds reusable terrain SDF collider chunks for water and other consumers.

It runs after source refresh and can remain async/queued.

### Water Terrain Cache Rebuild

This builds water-grid cache patches from terrain collider chunks.

It runs on a worker and the main thread only applies revision-guarded results.

## Performance tradeoff

Synchronous visible rebuilds can create edit-frame spikes. Recent hidden perf runs:

```text
2026-05-17: 8-chunk synchronous visible rebuild: 35.32 ms
2026-05-18: sync-always run: 4 sync rebuilds, mean 12.39 ms, max 33.45 ms
2026-05-18: single-chunk PreserveFlora sync rebuilds: about 5.25-5.51 ms
```

This is an intentional tradeoff. The preferred behavior is:

```text
short visible edit hitch > visible cross-chunk crack
```

The remaining performance work should focus on systems that do not affect immediate visual terrain consistency:

- water simulation CPU cost,
- terrain SDF source/collider follow-up latency,
- water terrain cache worker latency/stale work,
- startup-only collider/cache spikes if they become important.

## Code simplification target

The desired long-term code shape is:

```text
visible terrain rebuild:
  synchronous and direct

terrain SDF / collider / water cache:
  queued, revision-guarded, budgeted
```

The staged async terrain visual rebuild code can remain temporarily as fallback/legacy code, but it is no longer the preferred path for visible terrain edits.

If we clean it up later, the simplification direction is:

1. Keep `world_ops::mesh_generate_chunks(...)` as the primary visible terrain rebuild path.
2. Keep preserve-flora visible rebuild synchronous unless it becomes visibly problematic.
3. Remove or isolate the old async visual terrain rebuild state machine.
4. Keep all water/collider/cache queues intact.

## Validation plan

Visible validation:

- Edit across chunk boundaries.
- Confirm no transient or persistent visible seam appears.
- Confirm the next rendered frame after the blocking edit shows all affected chunks updated together.

Performance validation:

- Continue release-mode measurements.
- Track sync visible rebuild spikes separately with `[PERF][SYNC_VISIBLE_REBUILD]`.
- Do not treat sync visible rebuild cost as a correctness bug unless gameplay hitches become unacceptable.

Follow-up queue validation:

- Confirm terrain SDF source refresh, collider build, and water cache rebuild still process after visible publish.
- Confirm stale queued results are discarded by revision checks.
- Confirm water collision catches up without overwriting newer terrain state.
