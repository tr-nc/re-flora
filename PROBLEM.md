# Water Terrain Fallthrough Problem

## Observation

After lowering the tiny pond simulation bounds to match the current terrain height range, all water debug particles appear to fall under the terrain.

Current pond bounds:

```text
x: 1.0..2.0
y: 0.0..1.0
z: 1.0..2.0
```

Current terrain assumption:

```text
terrain y: 0.0..1.0, guaranteed above 0.0
```

## Expected

Water particles should remain above the sampled terrain height inside the pond XZ footprint.

## Actual

Water particles fall below the visible terrain surface.

## Relevant recent changes

- Added `WaterTerrainCollider` heightfield data to `re-flora-water`.
- Added app-side sampling from `App::query_terrain_height_cpu(Vec2)` over `x,z = 1.0..2.0`.
- Added vertical-only terrain response in `PondWaterSim::update_grid`.
- Lowered the fixed pond box from `y = 1.0..2.0` to `y = 0.0..1.0`.

## Notes for later investigation

Do not solve this in this note; this is a marker for a follow-up fix.

Likely areas to inspect:

- Terrain collision is currently grid-node-only. Particles may still advect below the sampled terrain between grid updates.
- Particle-level collision still only clamps against the fixed box, not the terrain surface.
- The fixed box lower clamp is near `y = 0.0 + particle_padding`, which can be below the local terrain height.
- The sampled terrain diagnostic log should be checked to confirm sampled min/max/avg within the pond footprint.
- The vertical-only grid collision may be insufficient if active grid nodes do not overlap the terrain boundary at the right time or resolution.
