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

The app builds water terrain collider chunks at `32^3` resolution. The stable
solid-mask-to-SDF hot path now lives in the `re-flora-terrain-collider` crate so
it can be compiled with release-style optimization during debug game builds.
Startup queues the initial pond chunk `(1, 0, 1)`. Terrain edit chunk rebuild completion
now marks that rebuilt chunk's CPU terrain source as dirty; the water collider
rebuild is enqueued only after the contree GPU->CPU query cache publishes or
clears that chunk's CPU source. Non-terrain-edit rebuilds, such as debug
tree/model mesh rebuilds, do not enqueue water collider work. The water queue
follows the terrain GPU->CPU cache pipeline: a queued collider chunk is submitted
once its required CPU contree cache work is complete, not after all editing
stops. For the current vertical
parity source, those dependencies are the chunk's `(x, z)` column from the
collider chunk upward, because downward raycasts can cross chunks above the
collider chunk. Collider construction runs on a background worker from a
CPU-query snapshot. Completed worker results are published even when a newer
revision is already queued, then the queued latest revision is processed next;
this gives progressive collider updates during continuous edits instead of
waiting for the edit stream to stop. The water sim still waits for the initial
pond chunk collider before it starts stepping or rendering water debug
particles.

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
  phases solid/count/sdf/stats ...

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
11. Added a dedicated delayed water terrain collider rebuild queue using
    `LatestChunkQueue`.
12. Startup now enqueues the initial pond chunk collider build, and terrain edit
    rebuild completion marks the rebuilt chunk's CPU terrain source dirty for
    later water collider enqueue after GPU->CPU cache publication.
13. Water collider construction moved off the main thread. The queue now waits
    for the collider chunk's vertical-column CPU contree cache dependencies to
    be ready, takes a CPU-query snapshot, and submits one background collider
    build at a time.
14. Worker completion now publishes intermediate collider revisions during
    continuous edits, then immediately requeues/processes the latest pending
    revision. This avoids the previous behavior where every in-flight build was
    discarded as stale until the user stopped editing. Collider build logs now
    include phase timings for solid classification, solid counting, SDF
    construction, and final stats hashing so the remaining cost can be optimized
    from measurements.
15. Publishing a collider chunk only stabilizes water particles on first insert
    for an overlapping pond chunk. Replacing an already-active terrain collider
    during edits no longer zeroes water velocities every build, so water can
    respond continuously to progressive collider updates. Unrelated cached chunks
    also do not reset/nudge the live particles.
16. The stable heap/Dijkstra solid-mask-to-SDF builder was extracted into the
    `re-flora-terrain-collider` crate and compiled with `opt-level = 3` in dev
    builds. This preserves the current collider field while reducing debug-build
    water collider rebuild latency.
17. Contree CPU query source updates now carry per-chunk source revisions when a
    chunk's GPU->CPU cache is published or cleared. Terrain edits mark the source
    chunk dirty, and the water collider queue is enqueued only after that CPU
    source update event arrives.
18. Water collider builds now record their dependency source revision vector and
    skip queued rebuilds when the CPU terrain source revisions have not changed.

## Known issues

1. Collider build is still the dominant source of edit-to-collider latency in
   debug builds. Before extracting the SDF builder crate, phase timing on the
   startup/edit chunk showed about `106.6ms` total in debug (`~22.2ms` solid
   classification, `~83.7ms` heap/Dijkstra SDF, negligible counting/stats). The
   same auto-exit run in release averaged about `6.0ms` total (`~1.2ms` solid,
   `~4.7ms` SDF), so the large delay was mostly a debug-build artifact. The SDF
   phase now comes from `re-flora-terrain-collider`, which is optimized in dev
   builds. A follow-up debug auto-exit run averaged about `28.6ms` total
   (`~22.7ms` solid classification, `~5.2ms` SDF), with `penetrating 0` in the
   water perf log. The remaining debug latency is now mostly the app-side solid
   classification/contree query phase. The work happens on a background worker
   instead of the main thread.
2. The dedicated queue coalesces repeated dirty requests and tracks latest
   revisions, but only one water collider worker job runs at a time. Rapid edits
   update progressively at roughly one collider-build cadence, so the collider
   can still lag behind the latest brush stroke by about a build duration.
3. The current vertical parity pass is a correctness bridge for the existing
   surface contree cache. A better long-term source is a chunk-local filled
   occupancy representation or direct SDF generation from voxel volume data.
4. The app still treats `{ (1, 0, 1) }` as the startup/first-water chunk even
   though the collider set can now receive additional rebuilt chunks.

## Next implementation steps

1. Make collider generation incremental or faster.
   - The build now runs off the main thread, but it still performs a full
     `32^3` parity classification and heap/Dijkstra SDF propagation per chunk.
   - Preserve the previous valid collider until the new
     `Arc<WaterTerrainColliderChunk>` is ready to publish.

2. Replace the single `water_terrain_initialized: bool` with per-chunk dirty
   state, for example:

   ```rust
   water_terrain_dirty_chunks: HashSet<glam::IVec3>
   water_terrain_needed_chunks: HashSet<glam::IVec3>
   water_terrain_built_revisions: HashMap<glam::IVec3, u64>
   ```

3. Add a stronger per-chunk publish policy:
   - preserve unrelated collider chunks;
   - avoid publishing all-empty chunks;
   - optionally prioritize startup/active-water chunks over unrelated chunks.

4. Replace the current heap/Dijkstra SDF propagation with a faster regular-grid
   distance transform. This should keep or improve quality while reducing build
   time.

5. Keep the startup/first-water chunk hardcoded to `{ (1, 0, 1) }` until the
   queued multi-chunk path is stable.

6. Expand source-update-to-collider invalidation from the current edited-chunk
   mapping to all active/needed collider chunks whose dependency set includes the
   updated CPU terrain source chunk.

7. Then expand active-water chunk selection beyond the startup pond chunk.

8. Validate against real terrain overhangs/caves: ceiling, wall, and floor
   surfaces should all collide through the same 3D SDF path.

Keep the fluid grid at `32^3` during this work. The collider representation can
be generalized without increasing MLS-MPM resolution.
