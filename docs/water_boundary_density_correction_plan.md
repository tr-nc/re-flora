# Water Boundary Density Correction Plan

## Goal

Reduce the persistent agitation of weakly-compressible water in the sloped startup pool without switching to a Poisson pressure solve and without hiding the issue behind terrain tangent damping.

The intended fix direction is an SDF/ghost-boundary density correction: near solid terrain, compensate for the missing kernel support in the density/pressure evaluation so the weakly-compressible EOS can produce a more correct hydrostatic pressure field.

Working branch/worktree for this effort:

- Branch: `agent/water-boundary-density`
- Worktree: `/home/terence/code/re-flora-agent-water-boundary-density`

## Current Problem

The startup water pool is now an inverted pyramid with 45-degree side slopes. In the current simulation, particles near those slopes continuously slide downward and stir the whole water volume. The visible symptom is a pond that never settles even when no external interaction is happening.

This is not primarily an EOS selection problem. The current EOS can generate pressure from density, but the boundary-side density estimate is deficient.

Current relevant behavior:

- Pressure is computed from a density-based weakly-compressible EOS in `crates/re-flora-water/src/mls_mpm.rs`:

  ```text
  rho0 = particle_mass / particle_volume
  r    = rho / rho0
  p    = stiffness * (r^gamma - 1)
  p    = max(p, pressure_floor)
  ```

- `rho` is estimated from real fluid mass on the MPM grid.
- Terrain collision projects normal velocity out of the SDF surface and can damp tangent velocity, but tangent damping is only a numerical dissipation control.
- On a free-slip sloped boundary, gravity has a real tangential component along the slope. In a correct static liquid, that component is not canceled by wall friction; it is canceled indirectly by the hydrostatic pressure field satisfying:

  ```text
  grad(p) = rho * g
  ```

The current boundary density estimate does not generate a well-balanced hydrostatic pressure field near the sloped terrain. As a result, slope-adjacent particles behave like beads sliding down a ramp, then pressure/collision responses recirculate the water.

## Root Cause

Near a solid boundary, the density kernel support of a particle is truncated: part of the kernel lies inside solid terrain. Our current density estimate only gathers mass from real water particles/grid mass. It does not include an equivalent ghost-fluid or boundary-density contribution from the solid side.

That causes:

1. Underestimated density near solid terrain.
2. Underestimated pressure near terrain after the EOS.
3. Weak or incorrect pressure gradient near sloped walls.
4. Failure to maintain hydrostatic balance in the pond.
5. Continuous flow along slopes even when the physically expected state is still water with a horizontal free surface.

This is exactly the class of issue addressed in SPH/WCSPH literature by ghost particles, boundary particles, density maps, volume maps, and modified dynamic boundary conditions.

## Constraints

- Keep the weakly-compressible water model.
- Do not add a Poisson pressure projection.
- Do not use terrain tangent damping as the primary fix.
- Preserve the existing terrain SDF/collider path.
- Keep changes staged and measurable.
- Do not let ghost/boundary density pollute real fluid mass or grid velocity normalization.

## Research Summary

### Ghost SPH for Animating Water

Reference: Schechter and Bridson, 2012, *Ghost SPH for Animating Water*.

Key ideas:

- Sample ghost particles in a narrow band inside solid geometry.
- Solid ghost particles contribute to density summation for nearby liquid particles.
- Solid ghost density is extrapolated from nearby liquid density so pressure remains continuous through the boundary.
- For inviscid/no-stick behavior, ghost velocity uses solid normal velocity plus liquid tangential velocity. This enforces normal boundary behavior without adding unphysical tangential drag.

Implementation lesson for us:

- The important part is not friction. The important part is filling missing density support at the wall so EOS pressure near the boundary is meaningful.
- Explicit ghost particles are possible but probably heavier than needed because we already have terrain SDF samples on a grid.

### Boundary Particles / Pseudo-Mass Density Correction

Common WCSPH boundary handling extends density as:

```text
rho_i = sum_j m_j W_ij + sum_k Psi_k W_ik
```

where boundary particles carry a pseudo-mass/volume term `Psi_k` chosen so that the boundary sampling contributes approximately rest density.

Implementation lesson for us:

- Boundary density should contribute to pressure/density evaluation.
- It should not be treated as real advected water mass.

### mDBC: Modified Dynamic Boundary Conditions

Reference: English et al., 2022, *Modified dynamic boundary conditions for general-purpose SPH* / DualSPHysics mDBC.

Key ideas:

- Original dynamic boundary particles often create an unphysical gap and noisy pressure near walls.
- mDBC places ghost nodes inside the fluid domain by mirroring across the boundary interface.
- Fluid density and density gradients are interpolated at the ghost node, then linearly extrapolated to boundary particles.
- Reported still-water tests converge to hydrostatic pressure, including beds with a wedge/sharp corner.
- Kinetic energy in still water is much lower than with the older DBC method.

Implementation lesson for us:

- Hydrostatic still-water over slopes and corners is a known validation case for boundary density correction.
- A more advanced second-stage version for us can mirror/sample into the fluid side using SDF normals and extrapolate boundary density, but the first experiment can be simpler.

### Density Maps / Volume Maps for Implicit Boundaries

References: Koschier/Bender density maps; Bender et al., *Volume Maps: An Implicit Boundary Representation for SPH*.

Key ideas:

- Represent solid boundaries with SDF/implicit geometry instead of explicit boundary particles.
- Precompute or evaluate the boundary volume that overlaps the kernel support.
- Add the boundary contribution to the fluid density calculation.
- This avoids bumpy particle-sampled boundaries and fits complex geometry.

Implementation lesson for us:

- This is the closest match for the existing `terrain_grid` SDF cache.
- We can approximate the missing boundary volume fraction directly from SDF samples and the same 3x3x3 kernel stencil used for density.

### Dynamic Pressure Boundary Correction

Recent WCSPH pressure-boundary work uses zeroth-order SPH consistency to account for missing support in pressure-gradient terms. It is mostly targeted at pressure inlet/outlet boundaries, but the useful idea is the same: missing support must appear in pressure terms, not in ad hoc damping.

Implementation lesson for us:

- If density-only correction is insufficient, a pressure-gradient/stress correction term could be a later experiment.
- It is not the first step because our MLS-MPM pressure path currently enters through density -> EOS -> stress.

### MLS-MPM Augmented Grid Points

Reference: Toyota and Umetani, 2024, *Accurate Boundary Condition for MLS-MPM using Augmented Grid Points*.

Key ideas:

- Add augmented grid points along boundaries to improve MLS interpolation near walls.
- Improves wall-normal velocity behavior and reduces particles passing through/accumulating at boundaries.

Implementation lesson for us:

- Relevant to MPM boundary interpolation quality, but it is primarily a velocity/boundary-condition method.
- It does not directly solve the weakly-compressible density deficiency that is driving our hydrostatic imbalance.

## Proposed Implementation Plan

### Step 0: Baseline Measurements

Before changing behavior, capture a baseline for the current inverted-pyramid pool:

- average/max particle speed over time
- near-terrain average/max speed
- particle kinetic energy
- raw density min/avg/max
- pressure min/avg/max
- terrain contact count
- penetration count / minimum SDF

Use release-mode app runs as the authoritative behavior check.

### Step 1: Local SDF Boundary Density Correction

Add a density correction only for particles whose density stencil overlaps solid terrain.

For the same 3x3x3 stencil used by `particle_density_from_grid`:

1. Compute raw fluid density from real grid mass as today:

   ```text
   rho_raw = gathered_fluid_mass * inv_cell_volume
   ```

2. Estimate how much of the kernel support lies in fluid versus solid terrain using terrain SDF samples:

   ```text
   fluid_fraction = weighted fraction of stencil support outside solid terrain
   solid_fraction = 1 - fluid_fraction
   ```

3. Correct density by filling the missing solid-side support with a pressure-continuous ghost-fluid assumption:

   ```text
   rho_corrected = rho_raw / max(fluid_fraction, min_fluid_fraction)
   ```

Equivalent interpretation:

```text
rho_boundary = rho_raw * (1 / fluid_fraction - 1)
rho_total    = rho_raw + rho_boundary
```

Important rules:

- Only apply this when the missing support is solid terrain, not air/free surface.
- Do not add ghost mass into `grid_node.mass`.
- Do not use corrected density for grid velocity normalization.
- Use corrected density only for EOS pressure/stress and density diagnostics.
- Clamp correction factor initially to avoid pressure spikes near corners or noisy SDF samples.

Expected first implementation location:

- `crates/re-flora-water/src/mls_mpm.rs`
  - density gather / pressure calculation path
  - helper for terrain SDF support fraction
  - diagnostics for correction factor and corrected density

### Step 2: Unit Tests for Fraction/Correction Math

Add pure tests for synthetic SDF cases:

- no terrain samples -> correction factor is 1
- particle far from terrain -> correction factor is 1
- planar half-space through stencil -> correction factor is greater than 1 and bounded
- fully valid fluid support -> no correction
- degenerate/invalid SDF data -> safe fallback to raw density

If possible, test a 45-degree plane because it matches the inverted-pyramid pool wall.

### Step 3: Runtime Validation on the Pyramid Pool

Run the current startup scene with a fixed particle count, compare before/after logs:

```bash
cargo run --release -- --hidden --auto-exit 10 --water-particles 1000
cargo run --release -- --tail-latest-log 200
```

For visual confirmation, also use:

```bash
cargo run --release -- --windowed --water-particles 1000
```

Success indicators:

- lower long-term kinetic energy
- lower near-terrain average speed
- fewer repeated terrain corrections after settling
- plausible pressure increase with depth
- no increase in terrain penetration or non-finite particles
- no major performance regression

### Step 4: mDBC/Ghost Density Sampling

Upgrade the local support-fraction correction so solid-side stencil weight uses a virtual ghost density instead of only dividing by fluid fraction:

- Keep real fluid mass in `grid_node.mass` unchanged.
- For terrain-solid stencil samples, mirror/sample along the SDF normal into the fluid side.
- Convert the mirrored fluid density to pressure, add a hydrostatic pressure offset from mirror point to solid ghost point, and convert back to ghost density.
- Gather density as:

  ```text
  rho = rho_real + sum_solid W * occupancy * rho_ghost
  ```

This remains a pressure/EOS-only correction. The ghost density does not enter real grid mass or velocity normalization.

### Step 5: Separate Ghost Density Grid

Per-particle ghost density sampling was expensive because each corrected particle repeatedly mirrored/sample-tested the same solid stencil nodes. The current implementation caches mirrored ghost density in a separate `terrain_ghost_density` grid after real mass/momentum P2G and before pressure/stress P2G. The cache is populated only for currently touched terrain-overlap nodes and is cleared sparsely with the normal touched-grid clear path.

Measured on the 1000-particle hidden startup run, this reduced the mDBC P2G cost from about `0.960 ms/substep` to about `0.556 ms/substep` while preserving the same correction factors and contact behavior. Ghost density remains pressure-only and does not enter real grid mass or velocity normalization.

### Step 6: Optional First-Order mDBC-Style Extrapolation

If the ghost density grid helps but is noisy, add first-order density extrapolation:

- sample density at a fluid-side ghost point
- estimate local density gradient
- extrapolate back to the boundary/solid sample

This is closer to the DualSPHysics mDBC approach and may improve slopes/corners, but it should not be the first implementation.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Accidentally correcting free-surface density as if air were solid | Gate correction strictly on terrain SDF overlap, not empty fluid support alone |
| Pressure spikes near corners or noisy SDF gradients | Clamp correction factor; smooth SDF occupancy over about one grid cell |
| Ghost density changes momentum behavior | Keep ghost/boundary density out of real grid mass and velocity normalization |
| Hydrostatic pressure improves but waves persist | Add diagnostics first; then consider density diffusion or ghost density grid |
| Runtime cost increases | Start with the existing 3x3x3 stencil and cached terrain grid samples; avoid exact SDF calls in hot path |
| Tuning hides the physics | Compare pressure-vs-depth and kinetic energy, not just visual calmness |

## Acceptance Criteria

A first useful implementation should show:

- `cargo fmt --check`, `cargo check`, and relevant tests pass.
- The release hidden startup run exits cleanly.
- Logs show finite corrected density and pressure values.
- Long-term near-terrain speed/kinetic energy is lower than baseline.
- Terrain penetration and correction counts do not get worse.
- The fix does not require increasing terrain tangent damping.

## References

- Schechter, Bridson, *Ghost SPH for Animating Water*, 2012: https://www.cs.ubc.ca/~rbridson/docs/schechter-siggraph2012-ghostsph.pdf
- English et al., *Modified dynamic boundary conditions for general-purpose SPH*, 2022: https://doi.org/10.1007/s40571-021-00403-3
- Bender et al., *Volume Maps: An Implicit Boundary Representation for SPH*, 2019: https://dl.acm.org/doi/10.1145/3359566.3360077
- Interactive Computer Graphics WCSPH notes: https://interactivecomputergraphics.github.io/physics-simulation/examples/wcsph.html
- Fan et al., *Dynamical pressure boundary condition for WCSPH*, 2024: https://arxiv.org/html/2403.09485v1
- Toyota, Umetani, *Accurate Boundary Condition for MLS-MPM using Augmented Grid Points*, 2024: https://github.com/nobuyuki83/accurate_bc_for_mls_mpm
