# Water Simulation Performance and Optimization Notes

Last updated: 2026-05-20.

This document is the running baseline for CPU water simulation performance. Update it whenever an optimization changes `crates/re-flora-water` or the water worker path.

## Current constraints

- Priority: CPU first, single-core first.
- Multi-core ideas are useful, but should not complicate the first optimization pass.
- Render thread must not block on simulation; water simulation currently runs on the dedicated `water-sim` worker thread.
- Fixed substep target: `substep_dt = 1 / 120s`, so the single-core realtime budget is `8.33 ms/substep`.
- Baseline grid: `160 x 64 x 160` = `1,638,400` nodes.
- Collider bounds: `0..5 x 0..2 x 0..5`, `dx = 1/32`.

## Baseline benchmark command

Use release-mode hidden app runs. Source the shell environment first if the agent settings are not already doing it.

```bash
cargo run --release -- --hidden --auto-exit 8 --perf --water-particles <count>
```

For longer steady-state sampling:

```bash
cargo run --release -- --hidden --auto-exit 35 --perf --water-particles 100000
```

The authoritative worker-side lines are:

```text
[PERF][WATER] ... avg <ms>/substep ...
```

Do not use main-thread frame metric `water_handoff` as simulation compute time; it only measures snapshot/command handoff.

## Baseline results

Measured after moving simulation to the worker thread, using release hidden runs with `--perf`.

| particles | windows | avg ms/substep | realtime at 120 Hz | core-equivalent at 120 Hz | status |
| ---: | ---: | ---: | ---: | ---: | --- |
| 0 | n/a | n/a | idle | ~0 | worker idles, no water perf line |
| 10,000 | 6 | 3.52 | 2.37x | 0.42 | realtime |
| 25,000 | 6 | 6.65 | 1.25x | 0.80 | realtime |
| 50,000 | 6 | 13.02 | 0.64x | 1.56 | behind |
| 100,000 | 31 | 26.09 | 0.32x | 3.13 | behind |

Long 100k run source: `target/re-flora-logs/re-flora-20260520-130805.933-28187.log`.

### 100k breakdown

Mean over 31 perf windows, normalized per substep:

| component | ms/substep | share of total |
| --- | ---: | ---: |
| repair | 0.44 | 1.7% |
| clear | 0.04 | 0.1% |
| P2G | 5.04 | 19.3% |
| grid update | 0.28 | 1.1% |
| G2P total | 20.29 | 77.8% |
| total | 26.09 | 100% |

G2P is the P0 bottleneck. At 100k, reaching single-core 120 Hz requires reducing total substep cost from about `26.1 ms` to `<= 8.33 ms`.

G2P sub-timers under `--perf` currently include noticeable instrumentation/loop overhead, so use `g2p total` as the reliable top-level number and sub-timers only for directional comparisons:

| G2P sub-timer | ms/substep |
| --- | ---: |
| gather | 4.44 |
| box collision | 1.54 |
| terrain projection | 2.27 |
| repair | 1.68 |

Validation status from the sweep: no `non_finite`, `out_of_bounds`, or `terrain_penetrating` particles were observed.

## Optimization backlog

### P0: G2P interior fast path

Current `particle_to_grid` already has an interior fast path that avoids per-node bounds checks and repeated 3D-to-linear index recomputation. `grid_to_particle` still does the slow path for every stencil node.

Implement the same split for G2P:

- If `particle_stencil_interior(base, grid_dim)`, compute `base_idx` once.
- Use linear offsets: `base_idx + ox + oy * y_stride + oz * z_stride`.
- Avoid `in_grid()` and `grid_index_dims()` inside the 27-node loop.
- Keep the current checked slow path for boundary particles.

Expected risk: low. This should preserve results except for tiny floating-point ordering differences.

### P0/P1: particle grouping by base cell for G2P locality

Research on PIC/MPM transfer optimization repeatedly points to particle sorting/binning by cell as the CPU locality win.

Candidate design:

- Compute a `base_cell` / Morton key for each particle.
- Group particles with the same G2P stencil base.
- For each group, load/cache the 27 grid velocities once, or at least reuse the computed base index and strides.
- Iterate particles in that group while grid data is hot in cache.

This could reduce random grid reads, repeated index arithmetic, and branch cost. It is a bigger data-structure change than the interior fast path, so benchmark before and after.

Questions to measure before implementing:

- Percentage of particles on the interior fast path.
- Distribution of particles per base cell.
- Sort/bin maintenance cost per substep.
- Whether grouping helps enough at 10k/25k, or only at 50k/100k.

### P1: split and reduce G2P terrain/collision work

The current G2P pass also does:

- box collision,
- terrain cache query / projection,
- exact fallback if needed,
- final repair.

Ideas:

- Keep terrain exact fallback out of steady-state hot paths.
- Check if terrain collision can be skipped for particles whose cached SDF is comfortably positive.
- Preserve existing diagnostics for cache false skips.
- Be careful: terrain repair prevents the next P2G from depositing mass inside terrain.

### P2: AoSoA / SIMD-friendly particle layout

Current `WaterParticle` is AoS:

```rust
struct WaterParticle { x, v, c, j }
```

AoS is convenient but not ideal for SIMD. Research often uses SoA or AoSoA:

- SoA: best contiguous field access, worse whole-particle locality.
- AoSoA: groups 4/8/16 nearby particles; each group stores fields in SoA form.

For CPU single-core, AoSoA becomes most attractive after particles are grouped by cell, because a small group of nearby particles can share a grid tile and be vectorized together.

Expected risk: high. This touches particle storage, snapshots, spawning, diagnostics, and rendering handoff.

### P2: exact G2P2G fusion

Wang et al. 2020 propose a Grid-to-Particles-to-Grid (`G2P2G`) fused pipeline. The exact CPU analogue would be:

```text
bootstrap P2G(particles_n -> grid_current)
for each substep:
  update grid_current
  G2P(grid_current -> particles_{n+1})
  in the same particle pass, P2G(particles_{n+1} -> grid_next)
  clear/swap grids
```

Requirements:

- double-buffer grid state,
- strict invalidation when config/terrain/spawn/stabilization changes particles or transfer parameters,
- CFL/substep guarantees,
- careful perf counters for prepared-grid rebuilds and wasted prepared grids.

Expected CPU single-core benefit is likely smaller than on GPU because the next P2G scatter still has to happen. Treat this as an experiment after G2P locality work.

Avoid approximate fusion that reuses the old G2P stencil for next P2G; next P2G must use `x_{n+1}` to remain equivalent.

### Later: multi-core CPU options

These are not first priority, but useful for future scaling:

- G2P is naturally particle-parallel if particles are disjoint.
- P2G scatter needs conflict handling:
  - per-thread grid buffers plus reduction,
  - grid/block coloring,
  - cell-binned particle bags,
  - domain decomposition with halo regions.
- PIC chunk-bag approaches avoid atomics and preserve locality, but are larger architecture changes.

## Research references and applicable lessons

- Hu et al. 2018, *A Moving Least Squares Material Point Method with Displacement Discontinuity and Two-Way Rigid Body Coupling*  
  https://yuanming.taichi.graphics/publication/2018-mlsmpm/  
  MLS-MPM reduces transfer/stress-divergence work and is designed to be optimization-friendly.

- Gao et al. 2018, *GPU Optimization of Material Point Methods*  
  https://cemyuksel.com/research/papers/gpu_mpm.pdf  
  Even though GPU-focused, it identifies particle-grid transfer operators as the key bottleneck and discusses sparse grids, cell-based sorting, and transfer design choices.

- Wang et al. 2020, *A Massively Parallel and Scalable Multi-GPU Material Point Method*  
  https://yuxingqiu.github.io/publication/mpmgpu2020siggraph/paper.pdf  
  Introduces G2P2G fusion and AoSoA particle bins. Directly relevant conceptually, but GPU benefits do not automatically transfer to single-core CPU.

- Barsamian, Chargueraud, Ketterlin 2017, *A Space and Bandwidth Efficient Multicore Algorithm for the Particle-in-Cell Method*  
  https://chargueraud.org/research/2017/pic_chunk/PIC-chunks.pdf  
  Strong CPU relevance: keep particles sorted by cell, improve locality, read/write each particle once per step, and use chunk bags for multicore without atomics. Their chunk-bag method was slightly slower on one core than a carefully sorted SoA baseline but scaled better on many cores.

- Taichi sparse/performance docs  
  https://docs.taichi-lang.org/docs/sparse  
  https://docs.taichi-lang.org/docs/performance  
  Useful ideas: sparse grid blocks, data layout experimentation, and local storage for stencil-like access patterns.

## Update protocol

When optimizing water performance, update this document with:

1. commit/branch or short description,
2. benchmark command,
3. particle counts tested,
4. `avg ms/substep` and key component timings,
5. anomaly status (`non_finite`, `out_of_bounds`, `terrain_penetrating`),
6. conclusion: keep, revert, or continue investigating.
