# Tiny Pond MLS-MPM Implementation Plan

Goal: add a tiny dynamically simulated pond to Re: Flora using a bounded, single-CPU-core MLS-MPM water solver inspired by Yuanming Hu et al.'s paper and the `taichi_mpm` reference implementation.

## Local references

- Primary implementation reference: `mls-mpm-ref/mls-mpm-final.md`
- Longer polished paper extraction: `mls-mpm-ref/mls-mpm-polished.md`
- Paper PDF authority: `mls-mpm-ref/mls-mpm.pdf`
- Reference codebase: `mls-mpm-ref/taichi_mpm/`
- Minimal MLS-MPM loop: `mls-mpm-ref/taichi_mpm/mls-mpm88.cpp`
- Explained MLS-MPM loop: `mls-mpm-ref/taichi_mpm/mls-mpm88-explained.cpp`
- Water material model: `mls-mpm-ref/taichi_mpm/src/particles.cpp`, `WaterParticle`
- 3D water scene example: `mls-mpm-ref/taichi_mpm/scripts/async/water.py`

## Constraints

- Simulate only a small local pond volume, not global water.
- Use one CPU core initially.
- Keep runtime allocation-free after initialization.
- Prefer a simple, visible implementation before adding better rendering.
- Keep the solver independent from the existing decorative `ParticleSystem`.
- Terrain coupling should be local and incremental.
- Start with plain explicit MLS-MPM/APIC; defer CPIC until we need thin-boundary compatibility or two-way rigid coupling.

## Solver shape

Add a new module:

```text
src/water/mod.rs
src/water/mls_mpm.rs
src/water/pond.rs
src/water/collider.rs
```

Core state:

```rust
struct WaterParticle {
    x: Vec3,      // world or pond-local position
    v: Vec3,      // velocity
    c: Mat3,      // APIC affine velocity matrix
    j: f32,       // volume ratio / compression
}

struct WaterGridNode {
    v: Vec3,
    mass: f32,
    solid: bool,
    normal: Vec3,
}

struct PondWaterSim {
    origin_ws: Vec3,
    extent_ws: Vec3,
    grid_dim: UVec3,
    dx: f32,
    inv_dx: f32,
    particles: Vec<WaterParticle>,
    grid: Vec<WaterGridNode>,
    accumulator: f32,
}
```

Initial target size:

```text
grid_dim: 32 x 16 x 32
particles: 4k-8k
substep dt: start around 1 / 240 s, clamp by CFL if needed
```

## Paper-derived implementation choices

Use the plain explicit APIC/MLS-MPM loop summarized in `mls-mpm-final.md`:

- Quadratic B-spline support over `3 x 3 x 3` grid nodes.
- Regular-grid moment constant `M_p = 1/4 * dx^2 * I`.
- Reuse APIC affine matrix `C_p` as the velocity-gradient estimate.
- Fuse affine momentum and stress/pressure in P2G where practical.
- Avoid explicit kernel-gradient evaluation.

For a general hyperelastic material, the fused P2G matrix is:

```text
Q_p = dt * V0_p * M_p^-1 * dPsi/dF(F_p) * F_p^T + m_p * C_p
mv_i += w_ip * (m_p * v_p + Q_p * (x_i - x_p))
```

For the tiny pond, we can use the water-specific scalar compression `j` from the reference `WaterParticle` instead of full solid plasticity.

## MLS-MPM substep

Each fixed substep follows the reference loop:

1. Clear grid mass and velocity.
2. Particle-to-grid scatter.
   - Compute base grid coordinate.
   - Compute quadratic B-spline weights over `3 x 3 x 3` nodes.
   - Scatter mass and momentum.
   - Add fused pressure/stress plus APIC affine contribution.
3. Grid update.
   - Normalize velocity by mass.
   - Apply gravity.
   - Apply pond box boundary collision.
   - Apply terrain/bottom solid collision.
4. Grid-to-particle gather.
   - Gather velocity.
   - Rebuild APIC affine matrix `c`.
5. Update water compression `j` from the gathered affine matrix trace.
6. Advect particles.
7. Clamp particles back into the pond volume if numerical drift escapes.

## Water material

Start with the water model from `WaterParticle` in the reference repo:

```text
p = k * (j^-gamma - 1)
sigma = -p * I
```

Initial parameters:

```text
k = 10000.0
gamma = 7.0, but test gamma = 1.0 too because the reference water script uses it
j_min = 0.1
gravity = Vec3::new(0.0, -9.8, 0.0) scaled for world units
```

Implementation note: if this is too stiff for our timestep, begin with the simpler Taichi `mpm88.py` fluid stress:

```text
stress = -dt * 4 * E * particle_volume * (j - 1) / dx^2
```

Then switch back to the Tait-style pressure once the pipeline is stable.

## Coordinate system

Use pond-local coordinates internally:

```text
local = world - pond_origin_ws
normalized/grid = local * inv_dx
```

Convert to world space only for rendering and for terrain height sampling.

## Terrain coupling

Phase 1 uses only a box collider.

Phase 2 samples a local heightfield under the pond:

```rust
height = App::query_terrain_height_cpu(Vec2::new(world_x, world_z));
```

Build a small collider grid matching the water grid XZ resolution. A water grid node is solid if its world `y` is below the terrain height plus a small margin.

When terrain edits affect the pond bounds, mark the pond collider dirty and resample it.

## Rendering plan

Phase 1 debug rendering:

- Convert water particles into `ParticleSnapshot`-like data.
- Render as small translucent blue particles using the existing particle upload path if possible.
- If existing texture kinds are too leaf/butterfly-specific, add a `ParticleRenderKind::WaterDebug` or a separate water debug upload path.

Later rendering options:

1. Blue point sprites with alpha/depth sorting ignored.
2. Screen-space metaballs from particle splats.
3. CPU-generated low-res surface mesh from the density grid.
4. Ray-traced implicit water surface in the tracer.

Do not start with surface extraction. First milestone is visibly flowing water.

## App integration

Add to `App`:

```rust
water_sim: PondWaterSim,
water_debug_snapshots: Vec<ParticleSnapshot>,
```

In the redraw loop, after time update and before GPU upload/recording:

```rust
self.update_water_simulation(frame_delta_time);
```

Expose minimal debug controls in the existing config panel later:

- enable water sim
- water particle count
- substeps per frame / timestep scale
- pressure stiffness `k`
- gravity scale
- reset pond

## Milestones

### Milestone 1: standalone CPU solver

- Add `src/water` module.
- Seed a rectangular water volume inside a box.
- Run fixed substeps.
- Add smoke test for particle bounds and finite values.
- No renderer integration yet.

### Milestone 2: debug render in world

- Instantiate one pond in `App::new`.
- Step it every frame.
- Upload water particles as debug blue sprites.
- Confirm stable flow at 4k particles.

### Milestone 3: terrain-shaped pond

- Place pond at a fixed world location.
- Sample terrain heightfield for bottom/banks.
- Collide grid velocities against the sampled solid region.
- Resample after local terrain edits.

### Milestone 4: interaction

- Add player disturbance impulses near feet or shovel hits.
- Add optional water source/sink for testing flow.
- Add splash/foam decorative particles for high velocity changes.

### Milestone 5: visual upgrade

- Evaluate metaballs or density surface extraction.
- Keep the debug particle mode as a fallback.

## Performance checklist

- Preallocate all `Vec`s.
- Flatten grid indexing: `idx = x + dim.x * (y + dim.y * z)`.
- Use struct-of-arrays if profiling shows particle loop bottlenecks.
- Avoid terrain ray queries per particle; sample a collider grid only when dirty.
- Measure solver milliseconds separately from renderer time.
- Keep grid size and particle count adjustable.

## Open questions

- Best world-unit scale for `dx`, gravity, and pressure stiffness.
- Whether existing particle renderer can represent water cleanly or needs a new render kind.
- How much pond deformation should terrain editing allow.
- Whether the pond should conserve fixed volume or allow gameplay sources/sinks.
