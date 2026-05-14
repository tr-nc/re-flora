# Water Terrain Generalization Plan

The tiny pond heightfield milestone is complete. This document now tracks the next
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
whole-world rebuild. Collider data should be built per chunk, cached, and updated
only for chunks affected by terrain edits or currently needed by active water.

`re-flora-water` should remain app-agnostic: the app owns terrain sampling and
passes plain collider chunk data into the water crate.

## Next step: per-chunk collider building

Build a first per-chunk 3D terrain collider path. The initial phases should
still build only one collider chunk for the current pond area, but the data model,
refresh path, and MLS-MPM sampling API should already be shaped to support
multiple collider chunks without another rewrite.

Proposed water-side data shape:

```rust
pub struct WaterTerrainColliderChunk {
    pub chunk_id: glam::IVec3,
    pub bounds_min_ws: glam::Vec3,
    pub bounds_max_ws: glam::Vec3,
    pub dim: glam::UVec3,
    pub sdf_ws: Vec<f32>, // negative = terrain solid, positive = empty/water
    pub revision: u64,
}

pub struct WaterTerrainColliderSet {
    pub chunks: HashMap<glam::IVec3, Arc<WaterTerrainColliderChunk>>,
}
```

Initial implementation path:

1. Add `WaterTerrainColliderChunk` and `WaterTerrainColliderSet` to
   `crates/re-flora-water`.
2. Add trilinear SDF sampling and gradient-normal sampling by world position.
3. Update MLS-MPM terrain collision to query the collider set instead of the
   pond heightfield.
4. In the app, build exactly one collider chunk covering the current pond box
   plus padding.
5. Store and pass that single chunk through `WaterTerrainColliderSet` so the code
   path is already multi-chunk-ready.
6. On terrain edits/rebuild completion, invalidate the collider chunk only if the
   edit overlaps its bounds.
7. Refresh the dirty collider chunk after terrain rebuilds and CPU cache jobs are
   idle, preserving the previous valid chunk while waiting.
8. Validate overhangs/caves: ceiling, wall, and floor surfaces should all collide
   through the same 3D SDF path.

After the one-chunk path is stable, expand chunk selection from "the current pond
chunk" to "all collider chunks overlapping active water regions."

Keep the fluid grid at `32^3` during this work. The collider representation can
be generalized without increasing MLS-MPM resolution.
