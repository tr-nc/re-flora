# Water Terrain Generalization Plan

The tiny pond heightfield milestone is complete. This document tracks the next
architecture step: replacing the pond-only heightfield with chunked 3D terrain
collider data that can scale toward dynamic water flowing through the world.

## Goal

Generalize terrain collision for water from:

```text
one fixed pond -> one 2D heightfield bottom collider
```

to:

```text
active water regions -> chunked 3D terrain collider cache
```

The terrain world may be huge, so water must never require a whole-world SDF or
whole-world rebuild. Collider data should be built per terrain chunk, cached,
and updated only for chunks affected by terrain edits or currently needed by
active water.

`re-flora-water` should remain app-agnostic: the app owns terrain sampling and
passes plain collider chunk data into the water crate.

## Core invariant

A water terrain collider chunk is a collider for one terrain chunk.

Do not give collider chunks arbitrary world bounds. Bounds are derived from the
chunk id and the terrain chunk coordinate system. In the current normalized
terrain coordinates:

```text
chunk_id (x, y, z) -> world bounds [chunk_id, chunk_id + 1]
```

The initial pond is deliberately inside chunk `(1, 0, 1)`, so the one-chunk
phase builds exactly that terrain chunk's collider. It is not a pond AABB and it
has no pond padding.

## Current water-side data shape

Implemented shape:

```rust
pub struct WaterTerrainColliderChunk {
    pub chunk_id: glam::IVec3,
    pub dim: glam::UVec3,
    pub sdf_ws: Vec<f32>, // negative = terrain solid, positive = empty/water
    pub revision: u64,
}

pub struct WaterTerrainColliderSet {
    pub chunks: HashMap<glam::IVec3, Arc<WaterTerrainColliderChunk>>,
}
```

`WaterTerrainColliderChunk` derives its chunk bounds from `chunk_id`.
`WaterTerrainColliderSet` is already multi-chunk-ready.

## Current app-side collider source

The current one-chunk app path builds chunk `(1, 0, 1)` at `32^3` resolution.
It no longer uses a heightfield fallback.

Important detail: the CPU contree cache currently represents terrain surface
voxels, not a filled solid volume. Direct point occupancy against that surface
cache produced a thin-shell SDF and allowed water to fall through gaps. The app
therefore classifies collider grid samples with a pure-3D vertical
surface-crossing parity pass:

```text
for each collider (x, z) column:
    raycast downward through terrain surfaces
    collect crossing y values
    classify sample y by odd/even crossing parity
```

Direct surface occupancy is still ORed in so exact surface samples remain solid.
This keeps the collider 3D and supports floors, ceilings, walls, caves, and
overhangs better than the old pond heightfield path.

Runtime diagnostics now log:

```text
[WATER][TERRAIN] built collider ...
  solid_samples ...
  source surface-contree-vertical-parity
  columns_with_hits ...
  crossings total/max ...
  direct_surface_samples ...
  center_sdf ...
  build_ms ...

[PERF][WATER] ...
  particle_y min/max/avg ...
  terrain_sdf_min ...
  penetrating ...
  no_sdf ...
```

A local smoke run showed no particle penetration after this change:

```text
terrain_sdf_min 0.0002 penetrating 0 no_sdf 0
```

## Completed in the first per-chunk pass

1. Added `WaterTerrainColliderChunk` and `WaterTerrainColliderSet` to
   `crates/re-flora-water`.
2. Added trilinear SDF sampling and gradient-normal sampling by world position.
3. Updated MLS-MPM terrain collision to query the collider set instead of the
   pond heightfield.
4. Added app-side construction of one collider chunk for terrain chunk
   `(1, 0, 1)`.
5. Store and pass that one chunk through `WaterTerrainColliderSet`.
6. Preserve the previous valid collider while refresh is deferred behind terrain
   rebuild and CPU cache work.
7. Added tests for SDF floor, ceiling, and wall collision paths.
8. Prevent water simulation/rendering before the first terrain collider exists.
9. Added 3D surface-crossing parity classification to avoid thin-shell
   falling-through from surface-only CPU terrain caches.
10. Added runtime logs for collider quality and particle terrain penetration.

## Known issues

1. Collider build is still synchronous and expensive. Local debug run showed
   roughly `~106ms` per one-chunk collider build.
2. Collider rebuilds can repeat more often than expected after terrain/flora
   rebuild work. This causes visible hitches.
3. The current vertical parity pass is a correctness bridge for the existing
   surface contree cache. A better long-term source is a chunk-local filled
   occupancy representation or direct SDF generation from voxel volume data.
4. The app still has a single hardcoded needed chunk: `{ (1, 0, 1) }`.

## Next implementation steps

1. Stop unnecessary repeated collider rebuilds.
   - Track the last built terrain/CPU-cache revision for each collider chunk.
   - Only rebuild a collider when the underlying terrain chunk revision changed.
   - Keep the previous valid collider active while a dirty chunk waits.

2. Extract app-side chunk building into a reusable function:

   ```rust
   fn build_water_terrain_collider_chunk(
       &self,
       chunk_id: glam::IVec3,
   ) -> Option<WaterTerrainColliderChunk>
   ```

   It should sample exactly that terrain chunk's bounds and return `None` rather
   than publishing an all-empty/missing-source collider.

3. Replace the single `water_terrain_initialized: bool` with per-chunk dirty
   state, for example:

   ```rust
   water_terrain_dirty_chunks: HashSet<glam::IVec3>
   water_terrain_needed_chunks: HashSet<glam::IVec3>
   water_terrain_built_revisions: HashMap<glam::IVec3, u64>
   ```

4. On terrain edit/rebuild completion, mark only overlapping terrain chunk ids
   dirty. Do not clear unrelated collider chunks.

5. After deferred terrain rebuilds and CPU cache jobs are idle, refresh dirty
   collider chunks one at a time and replace only that chunk in
   `WaterTerrainColliderSet`.

6. Move collider building off the main thread or into an incremental job.
   Preserve the previous valid collider until the new `Arc<WaterTerrainColliderChunk>`
   is ready to publish.

7. Replace the current heap/Dijkstra SDF propagation with a faster regular-grid
   distance transform. This should keep or improve quality while reducing build
   time.

8. Keep the initial needed chunk set hardcoded to `{ (1, 0, 1) }` until the
   one-chunk path is stable.

9. Then expand chunk selection from the hardcoded pond chunk to all terrain
   chunks overlapping active water regions.

10. Validate against real terrain overhangs/caves: ceiling, wall, and floor
    surfaces should all collide through the same 3D SDF path.

Keep the fluid grid at `32^3` during this work. The collider representation can
be generalized without increasing MLS-MPM resolution.
