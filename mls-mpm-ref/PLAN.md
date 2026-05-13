# Tiny Pond MLS-MPM Plan

## Current state

- Water solver lives in `crates/re-flora-water` so the main game can stay debuggable while the MLS-MPM hot loops compile optimized in dev builds.
- The initial pond is bounded by the fixed world-space box from `(1, 1, 1)` to `(2, 2, 2)`.
- The solver is single-CPU-core, explicit MLS-MPM/APIC, and currently uses only box-wall collision.
- The main app updates the pond when particles are enabled and renders water as blue debug particles through the existing particle path.
- Runtime perf logs are available with `--perf` as `[PERF][WATER]`.

## Terrain query coupling goal

Use the app's existing CPU terrain height query to give the tiny pond a terrain-shaped bottom while keeping `re-flora-water` independent from app, renderer, terrain-builder, and Vulkan internals.

App-side terrain API:

```rust
App::query_terrain_height_cpu(Vec2)
```

The water crate should receive already-sampled heightfield data and should not call app APIs directly.

## Phase 1: water crate terrain collider data

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

Add optional terrain collider state to `PondWaterSim`:

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

- Validate `heights_ws.len() == xz_dim.x * xz_dim.y`.
- Use bilinear height sampling by world `x,z` inside the collider bounds.
- Clamp sampling coordinates at the heightfield edges.
- Keep the type plain and allocation-free during simulation; all height allocation happens when the app refreshes the collider.

## Phase 2: water crate terrain collision

During `PondWaterSim::update_grid`, after mass normalization and gravity, apply terrain collision to active grid nodes.

For each active grid node:

1. Convert grid node coordinate to world position.
2. Sample terrain height at node `x,z`.
3. If the node is below the terrain surface plus margin:

```rust
node_world_y <= terrain_height + margin
```

then apply a first-pass vertical bottom response:

```rust
if node.v.y < 0.0 {
    node.v.y = 0.0;
}
```

Keep fixed box collision active on all six faces for now. The terrain collider only adds a shaped bottom; it does not replace the test box.

Why vertical-only first:

- It is stable and simple.
- It avoids injecting lateral energy from noisy heightfield normals.
- It is enough to confirm that app terrain data is correctly coupled into the solver.

Add a water crate smoke test with a synthetic flat or sloped heightfield:

- create `PondWaterSim::fixed_test_box()`
- set a terrain collider with height around `1.2`
- run substeps
- assert particles remain finite and inside the box

## Phase 3: app-side terrain sampling bridge

Add `src/app/core/water.rs` for app-specific glue.

Add state to `App`:

```rust
water_terrain_initialized: bool,
```

Add method:

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

This diagnostic is important because the current fixed box is at small world coordinates. We need to confirm that terrain height near `x,z = 1..2` is actually near the pond volume. If terrain is far below or above `y = 1..2`, the collider may appear to do nothing or may fill the whole box as solid.

## Phase 4: refresh timing

First implementation:

- Refresh once, lazily, before the first water update after the app finishes loading.
- Guard with `water_terrain_initialized`.

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

Do not refresh every frame.

## Phase 5: terrain edit invalidation later

After static sampling works, invalidate the water collider when terrain edits overlap the pond XZ footprint.

Pond XZ footprint:

```text
x: 1.0..2.0
z: 1.0..2.0
```

When an edit AABB overlaps that XZ box:

```rust
self.water_terrain_initialized = false;
```

The next update refreshes the heightfield.

This should be a separate commit from initial terrain coupling.

## Phase 6: sloped terrain normals later

After vertical-only collision is verified, estimate a normal from neighboring sampled heights:

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

This is deferred because terrain normals can introduce instability if the heightfield is noisy or mismatched to water scale.

## Commit plan

1. `add water terrain collider`
   - add `WaterTerrainCollider`
   - add setter/clearer/sampler
   - add water crate tests

2. `apply terrain collider to water grid`
   - use sampled heights in `update_grid`
   - vertical-only collision response
   - synthetic heightfield smoke test

3. `sample pond terrain collider`
   - add app-side `core/water.rs`
   - call `query_terrain_height_cpu`
   - refresh once before water update
   - log sampled min/max/avg

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
- water particles still visible and bounded
- no large regression in water `avg ms/substep`
