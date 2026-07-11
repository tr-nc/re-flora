# Water boundary density correction

This document records the current SDF/ghost-boundary density correction used by the weakly-compressible water solver.

## Problem

The startup pond has sloped terrain walls. Without boundary density support, particles near those walls underestimate density because part of their pressure kernel lies inside solid terrain. The EOS then produces too little pressure near the wall, so gravity's tangential component along the slope keeps driving persistent circulation.

The desired fix is not terrain tangent damping. A still liquid on a slope should be supported by a hydrostatic pressure field:

```text
grad(p) = rho * g
```

The boundary correction fills the missing solid-side kernel support for density/pressure evaluation while keeping real fluid mass and grid velocity normalization unchanged.

## Current implementation

The water solver keeps a separate pressure-only `terrain_ghost_density` grid:

1. Real particle mass and momentum are scattered to the MPM grid.
2. Terrain-overlap grid nodes touched by the current stencil populate `terrain_ghost_density`.
3. The pressure/stress P2G pass samples real density plus ghost-boundary density.
4. Ghost density affects EOS pressure/stress only; it is not added to real grid mass.
5. The ghost-density grid is cleared sparsely with the normal touched-grid clear path.

The cached grid replaced a more expensive per-particle ghost sample. On the 1000-particle startup hidden run, the mDBC-style P2G cost dropped from about `0.960 ms/substep` to about `0.556 ms/substep` while preserving the same correction factors and contact behavior.

Key files:

- `crates/re-flora-water/src/mls_mpm.rs`
- `crates/re-flora-water/src/pond.rs`
- `src/app/core/water/*`
- `config/gui.toml`

## Runtime tuning parameters

These are exposed through GUI/config and forwarded to the water worker:

- `water_boundary_density_min_fluid_fraction`
- `water_boundary_density_max_correction_factor`
- `water_boundary_density_occupancy_transition_cells`

Related fluid parameters:

- `water_stiffness`
- `water_gamma`
- `water_dynamic_viscosity`
- `water_pressure_floor`
- `water_particle_edge_len`

## Rules to preserve

- Apply boundary correction only for terrain/SDF overlap, not for free-surface air.
- Do not add ghost density to `grid_node.mass`.
- Do not use ghost density for grid velocity normalization.
- Use corrected density for EOS pressure/stress and diagnostics only.
- Clamp correction factors to avoid pressure spikes near corners or noisy SDF samples.
- Release hidden runs, not debug tests, are the behavior/performance evidence.

## Validation signals

Useful release-run checks:

```bash
cargo run --release -- --hidden --auto-exit 10 --water-particles 1000 --perf
cargo run --release -- --tail-latest-log 200
```

Look for:

- finite corrected density and pressure values;
- lower long-term near-terrain speed and kinetic energy;
- no increase in terrain penetration or non-finite particles;
- correction-factor averages/maxima staying within the intended clamp;
- no reliance on increased terrain tangent damping to calm the pond.

## References

- Schechter and Bridson, *Ghost SPH for Animating Water*, 2012: https://www.cs.ubc.ca/~rbridson/docs/schechter-siggraph2012-ghostsph.pdf
- English et al., *Modified dynamic boundary conditions for general-purpose SPH*, 2022: https://doi.org/10.1007/s40571-021-00403-3
- Bender et al., *Volume Maps: An Implicit Boundary Representation for SPH*, 2019: https://dl.acm.org/doi/10.1145/3359566.3360077
- Interactive Computer Graphics WCSPH notes: https://interactivecomputergraphics.github.io/physics-simulation/examples/wcsph.html
- Fan et al., *Dynamical pressure boundary condition for WCSPH*, 2024: https://arxiv.org/html/2403.09485v1
- Toyota and Umetani, *Accurate Boundary Condition for MLS-MPM using Augmented Grid Points*, 2024: https://github.com/nobuyuki83/accurate_bc_for_mls_mpm
