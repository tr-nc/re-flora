# Water MLS-MPM Performance Roadmap

## Current implementation status

We are **not finished with the full roadmap**, but the main hot-loop terrain-collision milestone is now in good shape.

Current status by phase:

| phase | status | current result |
| --- | --- | --- |
| Phase 1: cheap terrain queries | Done | SDF-only checks, direct chunk lookup, normal only on collision |
| Phase 2: remove duplicate terrain repair | Done | steady-state pre-P2G terrain sweep removed |
| Phase 3A: water-grid terrain cache | Done | grid-node terrain collision moved out of the hot loop |
| Phase 3B: G2P terrain broadphase/cache | Implemented, needs soak/verification | cached skip/projection path is active; exact checks are reduced but still significant |
| Phase 4: collider scope/startup | Not started | lower priority after current hot-loop wins |
| Phase 5: solver scaling options | Not started | defer until algorithmic waste is exhausted |

Latest representative release result:

- Baseline from `REPORT.md`: ~4.9 ms/substep.
- Current Phase 3A release samples: ~1.95-1.99 ms/substep.
- Current Phase 3B release samples: ~1.80-1.88 ms/substep.
- Overall solver cost is down by roughly 62-63% from the original report.
- Functional release validation stayed clean: `penetrating 0`, `no_sdf 0`.

The remaining hot issue is still G2P terrain collision, but Phase 3B moved part of the common path onto cached-grid SDF data:

- Phase 2 exact terrain checks: `4096/substep`
- Phase 3A exact terrain checks: `~4031-4096/substep`
- Phase 3B exact terrain checks: `~2526-2892/substep`

The next meaningful optimization is verification and tuning of the cached G2P path: reduce exact fallbacks further without allowing missed terrain penetration.

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
- The cached trilinear SDF path now skips far particles and directly projects clearly colliding particles, but ambiguous near-surface particles still use exact collider fallback.
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

### Phase 3B: cached G2P terrain collision

Status: implemented, needs more soak/verification before calling the whole phase complete.

Implemented:

- Added G2P terrain counters:
  - `terrain_cache_skips/substep`
  - `terrain_cache_projections/substep`
  - `terrain_exact_fallbacks/substep`
  - `terrain_exact_checks/substep`
  - `terrain_exact_corrections/substep`
- Replaced the boolean broadphase with `terrain_grid_particle_query()`.
- The query interpolates cached SDF from the 8 surrounding water-grid nodes.
- The query derives a normal from the trilinear cached-SDF gradient.
- G2P now has three paths:
  - skip exact terrain work when cached SDF is safely outside the collision margin plus slack;
  - project directly with cached SDF/normal when cached SDF is clearly colliding;
  - fall back to exact collider sampling for invalid cache data, out-of-grid particles, ambiguous near-surface particles, or invalid gradients.

Release benchmark:

Run log: `/tmp/re-flora-logs/re-flora-20260516-231428.005-119895.log`

| metric | phase 3A sample B | phase 3B sample A | phase 3B sample B | phase 3B sample C |
| --- | ---: | ---: | ---: | ---: |
| substeps/report | 240 | 217 | 241 | 240 |
| total water time | 468.63 ms | 407.57 ms | 437.87 ms | 432.12 ms |
| avg/substep | 1.953 ms | 1.878 ms | 1.817 ms | 1.800 ms |
| repair | 12.59 ms | 11.43 ms | 12.66 ms | 12.64 ms |
| grid | 18.13 ms | 16.49 ms | 16.63 ms | 16.01 ms |
| G2P total | 359.94 ms | 310.49 ms | 332.24 ms | 326.03 ms |
| G2P terrain | 112.77 ms | 109.95 ms | 109.66 ms | 104.68 ms |
| cache skips/substep | n/a | 337 | 794 | 847 |
| cache projections/substep | n/a | 1233 | 410 | 572 |
| exact fallbacks/substep | n/a | 2526 | 2892 | 2677 |
| exact checks/substep | 4031 | 2526 | 2892 | 2677 |
| exact corrections/substep | n/a | 224 | 116 | 88 |
| penetrating | 0 | 0 | 0 | 0 |
| no_sdf | 0 | 0 | 0 | 0 |

Interpretation:

- Overall water step cost improved again, from ~1.95-1.99 ms/substep after Phase 3A to ~1.80-1.88 ms/substep after Phase 3B.
- Exact G2P terrain checks dropped from almost all particles to roughly 62-71% of particles per substep.
- Cached projection is active and release validation stayed clean in this run.
- `g2p_terrain` improved only modestly because many particles still take exact fallback and the cached query/projection path has its own cost.

Recommended next steps:

1. Add verification for cached G2P decisions:
   - in perf/debug mode, shadow-sample a small deterministic subset of skipped/projected particles with the exact collider;
   - log any false skips or large cached-vs-exact SDF disagreement.
2. Tune fallback slack only after verification data is available.
3. Consider storing a cached gradient/normal for all terrain-grid nodes, not just near-surface nodes, if it reduces query cost or improves cached projection quality.
4. If exact fallbacks remain high after tuning, consider a coarser per-cell classification cache: empty / cached-projectable / exact-required.

Phase 3B completion criteria:

- `penetrating 0` and `no_sdf 0` remain true across several release hidden runs.
- Shadow verification reports no missed terrain contacts for skipped/projected particles.
- Exact G2P terrain checks are substantially below particle count in representative scenes.
- `g2p_terrain` is no longer a dominant sub-cost relative to G2P gather, or further reductions require solver-level changes.

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
- `terrain_cache_skips/substep`
- `terrain_cache_projections/substep`
- `terrain_exact_fallbacks/substep`
- `terrain_exact_checks/substep`
- `terrain_exact_corrections/substep`
- `penetrating`
- `no_sdf`

Functional acceptance:

- `cargo check` passes.
- `cargo test` passes.
- Hidden release run exits successfully.
- Latest log has no water, Vulkan, or shader errors.
- `penetrating 0` and `no_sdf 0` stay stable in representative release perf logs.
