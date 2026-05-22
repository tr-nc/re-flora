# Fluid MPM Replacement Plan

## Goal

Replace the current pond water MPM behavior with a 3D single-threaded fluid solver based on the water example from `nialltl/incremental_mpm`, while preserving the game's existing world-space box/SDF collider integration path.

Reference implementation cloned locally at:

- `/home/terence/code/incremental_mpm`
- Fluid reference: `Assets/3. MLS_MPM_Fluid_Multithreaded/MLS_MPM_Fluid_Multithreaded.cs`
- Simple transfer reference: `Assets/1. MLS_MPM_Intro_SingleThreaded/MLS_MPM_Intro_SingleThreaded.cs`

Working branch/worktree for this effort:

- Branch: `agent/fluid-mpm`
- Worktree: `/home/terence/code/re-flora-agent-fluid-mpm`

## Constraints

- First implementation should stay single-threaded.
- The design should keep phase boundaries clear so future parallelization is straightforward.
- The game simulation is 3D, not 2D.
- The game already has SDF terrain collider support; do not discard that path.
- Keep changes small and staged. Validate each step before integrating more complexity.

## High-Level Approach

The reference fluid solver uses two P2G passes:

1. Scatter particle mass and momentum to the grid.
2. Re-read grid mass to estimate density, compute fluid pressure/viscosity stress, and scatter stress momentum to the grid.

Then it updates grid velocities and gathers them back to particles with APIC/MLS-MPM transfer.

For our 3D game version:

- 2D 3x3 stencils become 3D 3x3x3 stencils.
- `float2` / `float2x2` become `Vec3` / `Mat3`.
- Density should be world-space density, so grid mass density needs the cell-volume scale, e.g. `density = weighted_grid_mass * inv_dx^3`.
- APIC affine reconstruction should remain world-space, e.g. `C = B * 4 * inv_dx^2` for quadratic kernels.
- The current terrain and box collision stages should remain available after the new fluid constitutive model is working.

## Step 1: Standalone 3D Single-Core Fluid Box

Implement the reference fluid algorithm in the existing water crate, but initially run it in a simple cubic world-space box:

- Bounds min: `(1, 1, 1)`
- Bounds max: `(2, 2, 2)`
- No terrain SDF collision in this first step.
- Box walls only.

### Tasks

1. Add or adapt configuration for the new solver parameters:
   - rest density
   - EOS stiffness
   - EOS power/gamma
   - dynamic viscosity
   - pressure floor / negative pressure clamp
   - substep dt
   - particle spacing/volume/mass

2. Refactor the simulation substep into explicit phases:
   - `clear_grid`
   - `particle_to_grid_mass_momentum`
   - `particle_to_grid_fluid_stress`
   - `update_grid`
   - `grid_to_particle`
   - `box_collision_and_repair`

3. Port the reference fluid model to 3D:
   - Estimate density from neighboring grid mass.
   - Compute particle volume as `mass / density`.
   - Compute pressure with Tait-style EOS.
   - Clamp negative pressure to avoid tensile clumping.
   - Add Newtonian viscosity from the velocity gradient / APIC affine matrix.
   - Scatter the stress contribution back to grid momentum.

4. Keep data structures compatible with future parallelization:
   - Avoid hidden global state in phase functions.
   - Keep read/write ownership of particles/grid clear per phase.
   - Do not introduce iterator patterns that make later chunking difficult.

5. Add focused tests or debug checks for:
   - particles remain finite
   - particles remain inside `(1,1,1)..(2,2,2)` with padding
   - grid mass is non-negative
   - no NaN/Inf velocities
   - a short fixed substep run does not explode

### Expected Result

A simple 3D fluid marker block behaves like weakly-compressible water inside a cube, without depending on terrain SDF. This step proves the 3D port and parameter scale before collider integration.

## Step 2: Reconnect Game SDF Collider Support

After the cubic box solver is stable, reconnect the existing terrain collider path.

Current relevant systems:

- `WaterTerrainColliderSet`
- `WaterTerrainColliderChunk`
- `terrain_grid` cache
- grid-node velocity projection against cached SDF normals
- particle-level SDF projection after G2P
- terrain change stabilization

### Tasks

1. Preserve the existing terrain cache rebuild and chunk update APIs.
2. Keep grid collision projection in `update_grid`:
   - after `node.v /= node.mass`
   - after gravity/damping
   - before G2P
3. Keep particle-level terrain correction after advection:
   - cached terrain-grid projection when available
   - exact SDF fallback when needed
   - bounded iterative correction for deep penetration
4. Consider a pre-P2G repair pass only if particles can remain inside terrain after terrain edits.
5. Validate terrain interactions with existing SDF tests and a hidden app run.

### Expected Result

The new fluid model works with the game's SDF terrain collider without depositing persistent mass from inside solids and without losing the existing collider update workflow.

## Step 3: Integration and Tuning

Once the new solver and SDF collision path both work, tune for gameplay and performance.

### Tasks

1. Tune default parameters in release app runs, not debug tests.
2. Compare visible behavior against the current solver:
   - settling behavior
   - compression/bounciness
   - splashing
   - terrain contact stability
   - boundary sticking
3. Inspect logs for:
   - max velocity saturation
   - terrain penetration
   - non-finite particles
   - out-of-bounds particles
   - active node counts
4. Keep future parallelization options open:
   - split P2G into independent particle batches later
   - use per-thread grid buffers or tiled accumulation later
   - parallelize grid update directly
   - parallelize G2P directly

## Suggested Validation Ladder

For Rust/shader-safe changes:

```bash
cargo fmt --check
cargo check
cargo test -p re-flora-water
cargo test
cargo run --release -- --hidden --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

For early algorithm-only work, `cargo test -p re-flora-water` plus targeted unit tests is enough before app validation.

## Open Questions

- Should the first cubic prototype replace the default pond bounds temporarily, or live behind a debug/test constructor?
- Should `WaterParticle::j` be removed, repurposed as a density ratio, or kept only for diagnostics during transition?
- What particle count and grid resolution should the first cubic run use for stable release-mode testing?
- How much viscosity should be exposed as gameplay tuning versus hardcoded solver stabilization?
