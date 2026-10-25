# Soil preserves unburied plants

## Impact and cause

A single Soil voxel could delete grass across its search halo. The opt-in native GPU replay started with 1,936 grass roots on a flat soil fixture: the old path removed 1,362 roots whose support and headroom had not changed. This is deletion, not delayed cache invalidation.

`apply_soil_deposition` passed the deposition **sampling halo** to `VisibleTerrainChange::preserving_flora`. Despite that name, its physical implementation removes all flora in a capsule, then runs ordinary grass placement/density eligibility over surviving occupancy. Neither operation is appropriate for exact soil additions. Already-established roots must not be reselected by procedural density rules.

The original Soil implementation `ac065c34` (2026-10-07) already passed an inflated radius to this removal path. Later code uses `sampling_halo`. The exact first player-visible occurrence has not been bisected.

## Fix and ownership

The deposition planner now returns the exact newly solid voxel coordinates alongside its inclusive terrain publication bounds. Speculative retries discard the whole plan; only uploaded writes reach application policy. The surface owner removes roots directly under those voxels, preserving all other packed instance state and authored identity. Removed authored plants update their competition fields. Terrain is published through the existing no-flora-rebuild path. Sampling bounds and publication bounds never serve as plant deletion masks.

No snapshot format or user settings change. This path uses synchronous instance readback only for chunks containing potentially buried roots; no performance improvement is claimed.

## Regression

```sh
cargo test terrain_deposition
cargo test flora_removal
RUST_LOG=info RE_FLORA_SOIL_BAG_REVIEW=flora cargo run --release -- \
  --rooftop-poc --hidden --mute --auto-exit 0.5
```

The isolated, unsaved rooftop review exercises production `apply_soil_deposition`, actual GPU terrain writes/publication and instance readback. Cases: one precise voxel and 64 spread voxels; reference and fast deposition; roots crossing horizontal chunk boundaries and at y=255 under writes in the next vertical chunk. Both grass species are present. It asserts unchanged roots survive, buried roots disappear, and no new roots appear.

After fix: all four cases pass, zero untouched roots removed. One-voxel cases retain 1,935/1,936 roots; spread cases retain 1,922/1,936. `cargo check`, Release build and hidden muted run pass; run log has no ERROR/panic/VUID. Authored-plant removal uses the same exact roots but this fixture does not independently cover authored identity restoration. Visual appearance and performance have not been approved.
