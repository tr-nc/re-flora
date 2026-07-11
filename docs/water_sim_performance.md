# Water Simulation Performance and Optimization Notes

Last updated: 2026-05-20.

This document is the running baseline for CPU water simulation performance. Update it whenever an optimization changes `crates/re-flora-water` or the water worker path.

For the current terrain-boundary density correction design, see `docs/water_boundary_density.md`.

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

## Optimization history

### 2026-05-20: G2P interior gather fast path

Change summary:

- Added an interior G2P stencil path using linear grid offsets.
- Precomputed scalar weight lanes (`wx`, `wy`, `wz`) and stencil `dpos` offsets.
- Reused `weighted_v` for both velocity and affine accumulation.
- Specialized timed vs untimed G2P with a const generic, so normal non-`--perf` simulation can compile out breakdown timer branches.

Release hidden sweep command:

```bash
cargo run --release -- --hidden --auto-exit 8 --perf --water-particles <count>
```

| particles | windows | before ms/substep | after ms/substep | delta | status |
| ---: | ---: | ---: | ---: | ---: | --- |
| 10,000 | 7 | 3.52 | 3.55 | +0.9% | noise/slightly worse |
| 25,000 | 6 | 6.65 | 6.04 | -9.1% | faster |
| 50,000 | 6 | 13.02 | 11.93 | -8.4% | faster |
| 100,000 | 6 | 26.09 | 23.85 | -8.6% | faster |

Sweep sources:

- `target/re-flora-logs/re-flora-20260520-133342.981-69858.log` (`10k`)
- `target/re-flora-logs/re-flora-20260520-133355.263-70302.log` (`25k`)
- `target/re-flora-logs/re-flora-20260520-133407.420-70506.log` (`50k`)
- `target/re-flora-logs/re-flora-20260520-133639.546-74696.log` (`100k`, rerun after moving affine scale inside the gather timer)

100k after-change breakdown, normalized per substep:

| component | before ms/substep | after ms/substep |
| --- | ---: | ---: |
| repair | 0.44 | 0.45 |
| clear | 0.04 | 0.04 |
| P2G | 5.04 | 4.92 |
| grid update | 0.28 | 0.29 |
| G2P total | 20.29 | 18.14 |
| total | 26.09 | 23.85 |

100k G2P gather sub-timer improved from about `4.44 ms/substep` to about `2.18 ms/substep`. Validation status from the sweep: no `non_finite`, `out_of_bounds`, or `terrain_penetrating` particles were observed.

### 2026-05-20: P2G arithmetic and steady-state cleanup

Change summary:

- Reused the G2P scalar/linear-offset stencil pattern in the P2G interior path.
- Accumulated G2P affine columns directly instead of constructing a `Mat3` outer product per stencil node.
- Added the default `gamma == 7` EOS pressure fast path and early no-tension return for `j >= 1`.
- Used unchecked interior grid indexing with debug assertions for the proven-in-bounds stencil paths.
- Removed the redundant pre-P2G repair pass; particles are repaired at the end of G2P, and external spawn/stabilization paths already produce bounded finite particles.

Long 100k confirmation command:

```bash
cargo run --release -- --hidden --auto-exit 35 --perf --water-particles 100000
```

Source: `target/re-flora-logs/re-flora-20260520-140236.405-30357.log`.

| reference | windows | avg ms/substep | delta vs original baseline |
| --- | ---: | ---: | ---: |
| original baseline | 31 | 26.09 | -- |
| after G2P fast path short run | 6 | 23.85 | -8.6% |
| after P2G/cleanup long run | 32 | 23.10 | -11.5% |

100k long-run breakdown, normalized per substep:

| component | original baseline | after P2G/cleanup |
| --- | ---: | ---: |
| repair | 0.44 | 0.00 |
| clear | 0.04 | 0.04 |
| P2G | 5.04 | 4.47 |
| grid update | 0.28 | 0.28 |
| G2P total | 20.29 | 18.31 |
| total | 26.09 | 23.10 |

Validation status from the 35s run: no `terrain_shadow_false_skips`, `terrain_penetrating`, or `no_sdf` particles were observed.

## Optimization backlog

### Done: G2P/P2G scalar interior fast paths and steady-state cleanup

Implemented. Kept checked slow paths for boundary particles. Results are listed in the optimization history above.

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
