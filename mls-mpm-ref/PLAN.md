# Tiny Pond MLS-MPM Terrain Coupling Plan

## Summary

Couple the tiny MLS-MPM pond to the existing CPU terrain query by sampling a small
app-owned heightfield and passing that data into `crates/re-flora-water`. The water
crate remains independent from the app, renderer, terrain builder, and Vulkan
internals. The first collision response is deliberately vertical-only so we can
verify the data path and stability before introducing terrain normals.

## Current state

- Water simulation lives in `crates/re-flora-water`, allowing the main app to stay
  debuggable while the MLS-MPM hot loops compile optimized in dev builds.
- The pond is currently bounded by a fixed world-space box from `(1, 1, 1)` to
  `(2, 2, 2)`.
- The solver is single-CPU-core, explicit MLS-MPM/APIC, and currently collides
  only against the box walls.
- The main app updates the pond when particles are enabled and renders it as blue
  debug particles through the existing particle path.
- Runtime water perf logs are available through `--perf` as `[PERF][WATER]`.

## Goals and constraints

- Use the existing app-side CPU terrain query:

  ```rust
  App::query_terrain_height_cpu(Vec2)
  ```

- Keep `re-flora-water` app-agnostic. It should receive sampled height data, not
  call app APIs directly.
- Keep the fixed box collider active for all six faces. The terrain collider adds
  only a shaped bottom for now.
- Avoid simulation-time allocation. Height allocation happens only when the app
  refreshes the collider.
- Refresh the collider once at startup initially; terrain-edit invalidation comes
  later.
- Add tests at the water-crate level before integrating the app-side bridge.

## Why this shape

- **Sampled data boundary:** Passing a plain heightfield into the water crate keeps
  solver code reusable and avoids coupling physics to app, renderer, or terrain
  implementation details.
- **Vertical-only first response:** Clamping downward velocity is stable, easy to
  reason about, and enough to prove that terrain data is affecting the solver.
  Slope normals can be added after the heightfield path is verified.
- **One-time refresh:** The initial pond is static and tiny. Refreshing every frame
  would add noise to profiling and complexity without improving the first milestone.
- **Diagnostic logging:** The pond sits at small world coordinates (`x,z = 1..2`,
  `y = 1..2`). A sampled min/max/avg log is needed to confirm terrain heights are
  actually near the pond volume.

## Phase 1: water-crate heightfield collider

Add a heightfield collider type to `crates/re-flora-water`:

```rust
pub struct WaterTerrainCollider {
    pub xz_dim: UVec2,
    pub bounds_min_ws: Vec3,
    pub bounds_max_ws: Vec3,
    pub heights_ws: Vec<f32>,
    pub margin: f32,
}
```

Add optional terrain state to `PondWaterSim`:

```rust
terrain: Option<WaterTerrainCollider>
```

Expose a small API:

```rust
impl PondWaterSim {
    pub fn set_terrain_collider(&mut self, collider: WaterTerrainCollider);
    pub fn clear_terrain_collider(&mut self);
    pub fn terrain_collider(&self) -> Option<&WaterTerrainCollider>;
}
```

Implementation notes:

- Validate `heights_ws.len() == xz_dim.x * xz_dim.y` when setting the collider.
- Require practical dimensions (`xz_dim.x >= 2`, `xz_dim.y >= 2`) so bilinear
  sampling is well-defined.
- Sample heights bilinearly by world `x,z` inside the collider bounds.
- Clamp sample coordinates at heightfield edges.
- Keep the collider plain data; all allocation is owned by refresh/setup, not by
  the simulation loop.

## Phase 2: water-crate terrain collision

In `PondWaterSim::update_grid`, after mass normalization and gravity, apply the
terrain bottom response to active grid nodes.

For each active grid node:

1. Convert the grid node coordinate to world position.
2. Sample terrain height at the node's `x,z`.
3. If the node is below the terrain surface plus margin:

   ```rust
   node_world_y <= terrain_height + margin
   ```

   then apply the first-pass vertical response:

   ```rust
   if node.v.y < 0.0 {
       node.v.y = 0.0;
   }
   ```

Keep the fixed box collision active. The terrain collider only augments the lower
boundary; it does not replace the test box.

Add water-crate smoke tests with synthetic flat or sloped heightfields:

- create `PondWaterSim::fixed_test_box()`;
- set a collider with heights around `1.2`;
- run several substeps;
- assert all particles remain finite and inside the box.

## Phase 3: app-side terrain sampling bridge

Add `src/app/core/water.rs` for app-specific glue.

Add app state:

```rust
water_terrain_initialized: bool,
```

Add a refresh method:

```rust
impl App {
    pub(super) fn refresh_water_terrain_collider(&mut self);
}
```

Initial sampling target:

```text
bounds_min_ws = (1, 1, 1)
bounds_max_ws = (2, 2, 2)
xz_dim = 32 x 32
```

For each sample point:

```rust
let height = self.query_terrain_height_cpu(Vec2::new(world_x, world_z));
```

Then pass the result into the water crate:

```rust
self.water_sim.set_terrain_collider(WaterTerrainCollider {
    xz_dim,
    bounds_min_ws,
    bounds_max_ws,
    heights_ws,
    margin: self.water_sim.dx * 0.5,
});
```

Log one diagnostic line on refresh:

```text
[WATER][TERRAIN] sampled 32x32 heights min ... max ... avg ... pond_y 1.0..2.0
```

This is important: if sampled terrain is far below or above `y = 1..2`, the
terrain collider may appear to do nothing or may turn the whole pond box into a
solid bottom region.

## Phase 4: refresh timing

First implementation:

- Refresh lazily before the first water update after app loading.
- Guard with `water_terrain_initialized`.
- Do not refresh every frame.

Pseudo-flow in the redraw/update path:

```rust
if self.render_flags.enable_particles {
    if !self.water_terrain_initialized {
        self.refresh_water_terrain_collider();
        self.water_terrain_initialized = true;
    }
    self.water_sim.update(frame_delta_time, self.perf_logging);
    self.update_particle_simulation(frame_delta_time);
}
```

## Phase 5: terrain edit invalidation later

After static sampling works, invalidate the water collider when terrain edits
overlap the pond XZ footprint.

Pond XZ footprint:

```text
x: 1.0..2.0
z: 1.0..2.0
```

When an edit AABB overlaps that footprint:

```rust
self.water_terrain_initialized = false;
```

The next water update refreshes the heightfield. Keep this as a separate commit
from initial terrain coupling.

## Phase 6: sloped terrain normals later

After vertical-only collision is verified, estimate a terrain normal from nearby
height samples:

```rust
normal = normalize(Vec3::new(-dh_dx, 1.0, -dh_dz));
```

Then project inward velocity away from the terrain:

```rust
let vn = node.v.dot(normal);
if vn < 0.0 {
    node.v -= normal * vn;
}
```

This is deferred because noisy heightfields or scale mismatches can inject lateral
energy and destabilize the tiny solver.

## Commit plan

1. `add water terrain collider`
   - add `WaterTerrainCollider`;
   - add setter/clearer/accessor and bilinear sampler;
   - add water-crate tests.

2. `apply terrain collider to water grid`
   - sample terrain heights in `update_grid`;
   - add vertical-only collision response;
   - add synthetic heightfield smoke test.

3. `sample pond terrain collider`
   - add app-side `core/water.rs`;
   - call `query_terrain_height_cpu` over the pond footprint;
   - refresh once before water update;
   - log sampled min/max/avg.

## Validation

Run after each step:

```bash
cargo check
cargo test -p re-flora-water
```

After app integration:

```bash
cargo run -- --windowed --auto-exit 3 --perf --no-shadows --no-denoise --no-god-rays --no-lens-flare
```

Look for:

- `[WATER][TERRAIN] sampled ...`
- `[PERF][WATER] ...`
- water particles still visible and bounded;
- no large regression in water `avg ms/substep`.
