# Water MLS-MPM Performance Roadmap

## Current implementation status

We are **not finished with the full roadmap**, but the first optimization milestone is in good shape.

Current status by phase:

| phase | status | current result |
| --- | --- | --- |
| Phase 1: cheap terrain queries | Done | SDF-only checks, direct chunk lookup, normal only on collision |
| Phase 2: remove duplicate terrain repair | Done | steady-state pre-P2G terrain sweep removed |
| Phase 3A: water-grid terrain cache | Done | grid-node terrain collision moved out of the hot loop |
| Phase 3B: G2P terrain broadphase/cache | Partial | conservative broadphase exists, but exact terrain checks are still near particle count |
| Phase 4: collider scope/startup | Not started | lower priority after current hot-loop wins |
| Phase 5: solver scaling options | Not started | defer until algorithmic waste is exhausted |

Latest representative release result:

- Baseline from `REPORT.md`: ~4.9 ms/substep.
- Current Phase 3 release samples: ~1.95-1.99 ms/substep.
- Overall solver cost is down by roughly 59-60% from the original report.
- Functional release validation stayed clean: `penetrating 0`, `no_sdf 0`.

The remaining hot issue is G2P terrain collision. Phase 3 reduced its cost, but did **not** reduce exact collider checks enough:

- Phase 2: `terrain_checks/substep 4096`
- Phase 3: `terrain_checks/substep ~4031-4096`

That means most particles still run exact particle-vs-terrain collision every substep. The next meaningful optimization is not more grid-node caching; it is making the G2P common path use cached-grid terrain data safely, or adding a tighter verified broadphase.

## Current diagnosis

`REPORT.md` shows the visual particle path is cheap. The bottleneck was, and still mostly is, CPU water MLS-MPM terrain collision work.

Resolved findings:

- SDF-only terrain checks no longer compute normals by accident.
- Terrain collider set lookup now uses direct unit chunk lookup for ordinary samples, with scan fallback for boundaries/misses.
- Steady-state `repair_particles()` no longer repeats a full terrain collision pass before P2G.
- `update_grid()` uses cached water-grid terrain samples instead of live SDF/normal queries per active node.
- Non-perf simulation avoids per-particle `Instant::now()` timing overhead.

Remaining findings:

- G2P terrain remains one of the largest sub-costs.
- The current cached trilinear SDF broadphase is intentionally conservative, so in the current pond/terrain setup it skips only a small fraction of exact checks.
- Full terrain-grid cache rebuilds happen when chunks are inserted/set/cleared. Partial overlapping-region rebuilds are not implemented yet.

## Implemented phases

### Phase 1: make existing collision checks cheap

Goal: reduce terrain query cost without changing water behavior.

Implemented:

1. Terrain collision queries SDF first and computes normals only when `sdf <= collision_margin`.
2. `WaterTerrainColliderSet::sample_sdf_ws()` is SDF-only.
3. Collider set sampling uses direct chunk lookup for ordinary samples, with scan fallback for exact chunk boundaries or misses.
4. Fine-grained per-particle timing is only active in the perf-detail path.

Release benchmark command:

```bash
zsh -lc 'source ~/.zshrc && cargo run --release -- --hidden --auto-exit 4 --perf'
```

Baseline from `REPORT.md`:

| metric | before phase 1 |
| --- | ---: |
| particles | 4096 |
| grid | 32^3 |
| substeps/report | 192 |
| total water time | ~945 ms |
| avg/substep | ~4.9 ms |
| G2P total | ~492 ms |
| G2P terrain | ~319 ms |
| repair | ~225 ms |
| terrain checks/substep | 4096 in G2P plus full repair sweep |

After Phase 1:

Run log: `/tmp/re-flora-logs/re-flora-20260516-224936.851-112893.log`

| metric | sample A | sample B |
| --- | ---: | ---: |
| substeps/report | 240 | 243 |
| total water time | 798.44 ms | 856.86 ms |
| avg/substep | 3.327 ms | 3.526 ms |
| repair | 120.17 ms | 136.56 ms |
| G2P total | 478.87 ms | 518.23 ms |
| G2P terrain | 261.88 ms | 297.21 ms |
| terrain checks/substep | 4096 | 4096 |
| penetrating | 0 | 0 |
| no_sdf | 0 | 0 |

Interpretation:

- Overall water step cost improved by about 28-32%.
- Repair became much cheaper, but all particles were still checked against terrain in G2P.

### Phase 2: avoid duplicate collision passes

Goal: stop doing two full particle terrain sweeps per substep.

Implemented:

1. `repair_particles()` now separates steady-state state repair from terrain repair.
2. Normal substeps keep finite/box/J/speed cleanup.
3. Terrain correction remains in G2P after advection.
4. Terrain changes still call `stabilize_after_terrain_change()`, which pushes particles out of the new collider immediately.

Release benchmark:

Run log: `/tmp/re-flora-logs/re-flora-20260516-225544.645-114132.log`

| metric | after phase 1 A | after phase 1 B | after phase 2 |
| --- | ---: | ---: | ---: |
| substeps/report | 240 | 243 | 242 |
| total water time | 798.44 ms | 856.86 ms | 726.84 ms |
| avg/substep | 3.327 ms | 3.526 ms | 3.003 ms |
| repair | 120.17 ms | 136.56 ms | 12.92 ms |
| G2P total | 478.87 ms | 518.23 ms | 516.27 ms |
| G2P terrain | 261.88 ms | 297.21 ms | 294.94 ms |
| terrain checks/substep | 4096 | 4096 | 4096 |
| penetrating | 0 | 0 | 0 |
| no_sdf | 0 | 0 | 0 |

Interpretation:

- Pre-substep repair cost dropped to roughly 0.05 ms/substep.
- Overall water step cost improved to ~3.0 ms/substep.
- G2P terrain remained dominant and still checked every particle.

### Phase 3A: cache terrain on the water grid

Goal: avoid live terrain SDF/normal sampling for grid-node terrain collision.

Implemented code:

- `WaterTerrainGridSample` in `crates/re-flora-water/src/pond.rs`.
- `PondWaterSim::terrain_grid` cache in `crates/re-flora-water/src/pond.rs`.
- `PondWaterSim::rebuild_terrain_grid_cache()` in `crates/re-flora-water/src/mls_mpm.rs`.
- Cache rebuilds on terrain set, chunk insert, and terrain clear.
- `update_grid()` uses cached near-surface normals instead of live collider queries.

Implemented cache contents:

- `sdf`
- `normal`
- `near_surface`
- `has_sdf`

Release benchmark:

Run logs:

- `/tmp/re-flora-logs/re-flora-20260516-230119.097-116570.log`
- `/tmp/re-flora-logs/re-flora-20260516-230222.890-117415.log`

Representative samples:

| metric | after phase 2 | phase 3 sample A | phase 3 sample B | phase 3 sample C |
| --- | ---: | ---: | ---: | ---: |
| particles | 4096 | 4096 | 4096 | 4096 |
| grid | 32^3 | 32^3 | 32^3 | 32^3 |
| substeps/report | 242 | 240 | 240 | 241 |
| total water time | 726.84 ms | 467.51 ms | 468.63 ms | 479.30 ms |
| avg/substep | 3.003 ms | 1.948 ms | 1.953 ms | 1.989 ms |
| repair | 12.92 ms | 12.86 ms | 12.59 ms | 12.69 ms |
| grid | 121.72 ms | 16.80 ms | 18.13 ms | 16.69 ms |
| G2P total | 516.27 ms | 359.99 ms | 359.94 ms | 372.42 ms |
| G2P terrain | 294.94 ms | 127.32 ms | 112.77 ms | 127.47 ms |
| terrain checks/substep | 4096 | 4096 | 4031 | 4050 |
| active nodes/substep | 3066 | 2291 | 2488 | 2291 |
| penetrating | 0 | 0 | 0 | 0 |
| no_sdf | 0 | 0 | 0 | 0 |

Interpretation:

- Overall water step cost improved from ~3.0 ms/substep to ~1.95-1.99 ms/substep.
- The biggest confirmed win is the grid-node terrain cache: grid time dropped from ~122 ms/report to ~17-18 ms/report.
- G2P terrain time also dropped, but exact terrain checks are still almost one per particle per substep.

### Phase 3B: finish G2P terrain optimization

Status: partial / next active work.

Already implemented:

- `terrain_grid_particle_may_hit()` uses trilinear cached water-grid SDF as a conservative broadphase.
- Exact particle collision is skipped only when cached SDF is safely outside the conservative band.
- Invalid, missing, out-of-grid, or boundary-risk cache cases fall back to exact collision.

Current limitation:

- The pond particles are close enough to terrain that the conservative band still marks almost all particles as exact-collision candidates.
- Therefore `terrain_checks/substep` remains near 4096.

Recommended next steps:

1. Add better G2P terrain counters:
   - cache skip count
   - cache candidate count
   - cached projection count
   - exact fallback count
   - exact collision correction count
2. Add a cached G2P collision path for the common case:
   - interpolate cached SDF from the 8 surrounding water-grid nodes;
   - interpolate or derive a cached normal;
   - if cached SDF is clearly outside the margin, skip exact terrain collision;
   - if cached SDF is inside the collision margin and cached normal is valid, project using cached data;
   - use exact collider sampling only for invalid cache data, ambiguous normals, out-of-grid particles, or debug verification fallback.
3. In perf mode, optionally shadow-sample a small subset of skipped/projected particles with the exact collider to verify no missed penetration.
4. Only tighten the conservative band after the counters show where exact checks are coming from.

Phase 3B completion criteria:

- `penetrating 0` and `no_sdf 0` remain true in release hidden runs.
- Exact G2P terrain checks are no longer near particle count, or exact collider sampling is removed from the common G2P path.
- `g2p_terrain` is no longer a dominant sub-cost relative to G2P gather.

## Later phases

### Phase 4: reduce terrain collider scope and startup work

Status: not started / lower priority.

Goal: avoid building and publishing colliders that cannot affect the pond.

Planned work:

1. Log collider chunks published vs. actually sampled by water.
2. Prioritize startup collider generation for chunks overlapping or near the pond.
3. Optionally skip non-overlapping chunks until water can move into them.
4. Keep direct chunk lookup and scan fallback behavior intact.

Expected effect: lower startup work and smaller terrain cache rebuild input.

### Phase 5: solver-level scaling options

Status: not started.

Goal: preserve appearance while lowering total CPU budget after hot-loop waste is removed.

Planned work:

1. Measure release-mode sweeps for particle count, grid resolution, and substep count.
2. Try fewer fixed substeps with adaptive CFL limits if stability allows.
3. Consider CPU parallelism or GPU/storage-buffer paths only after cached G2P collision is measured.

## Validation policy

Benchmark in release mode is king. Debug builds and unit tests are not performance evidence.

For each code phase:

```bash
cargo fmt --check
cargo check
cargo test
zsh -lc 'source ~/.zshrc && cargo run --release -- --hidden --auto-exit 4 --perf'
cargo run --release -- --tail-latest-log 200
```

When validating water performance:

- Use `--hidden --perf` so the real app/window/Vulkan path still runs.
- Do not use `--no-particles` for water benchmarks; current app update gating also disables water updates when particles are disabled.
- Compare `[PERF][WATER]` lines before and after the change.

Primary metrics:

- `total` and `avg/substep`
- `repair`
- `grid`
- `g2p`
- `g2p_gather`
- `g2p_terrain`
- `terrain_checks/substep`
- `penetrating`
- `no_sdf`

Functional acceptance:

- `cargo check` passes.
- `cargo test` passes.
- Hidden release run exits successfully.
- Latest log has no water, Vulkan, or shader errors.
- `penetrating 0` and `no_sdf 0` stay stable in representative release perf logs.
