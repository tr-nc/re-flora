# Water MLS-MPM Performance Roadmap

## Current implementation status

We are **not finished with the full roadmap**, but the main hot-loop terrain-collision milestone is now in good shape.

Current status by phase:

| phase | status | current result |
| --- | --- | --- |
| Phase 1: cheap terrain queries | Done | SDF-only checks, direct chunk lookup, normal only on collision |
| Phase 2: remove duplicate terrain repair | Done | steady-state pre-P2G terrain sweep removed |
| Phase 3A: water-grid terrain cache | Done | grid-node terrain collision moved out of the hot loop |
| Phase 3B: G2P terrain broadphase/cache | Implemented, needs more soak/tuning | cached skip/projection path is active and shadow-verified in perf runs |
| Phase 4: collider scope/startup | Implemented and benchmarked | startup/edit collider refreshes are limited to water-grid-domain chunks |
| Phase 5: solver scaling options | Implemented initial knobs/profile, needs visual soak | CLI sweep knobs plus `--water-profile performance`; release sweeps identify a safe low-CPU candidate |

Latest representative release result:

- Baseline from `REPORT.md`: ~4.9 ms/substep.
- Current Phase 3A release samples: ~1.95-1.99 ms/substep.
- Current Phase 3B/Phase 4 default release samples: ~1.75-1.85 ms/substep.
- Current Phase 5 performance profile samples: ~0.96-0.99 ms/substep at 120 Hz with 2048 particles.
- Default solver cost is down by roughly 62-64% from the original report; the initial performance profile reduces per-second water CPU budget further by halving the fixed substep rate and particle count.
- Functional release validation stayed clean: `penetrating 0`, `no_sdf 0`.
- Phase 3B shadow verification has reported `terrain_shadow_false_skips 0` in the latest release run.

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

Status: implemented, needs more soak/tuning before calling the whole phase complete.

Implemented:

- Added G2P terrain counters:
  - `terrain_cache_skips/substep`
  - `terrain_cache_projections/substep`
  - `terrain_exact_fallbacks/substep`
  - `terrain_exact_checks/substep`
  - `terrain_exact_corrections/substep`
  - `terrain_shadow_samples/substep`
  - `terrain_shadow_false_skips`
  - `terrain_shadow_sdf_err_avg`
  - `terrain_shadow_sdf_err_max`
- Replaced the boolean broadphase with `terrain_grid_particle_query()`.
- The query interpolates cached SDF from the 8 surrounding water-grid nodes.
- The query derives a normal from the trilinear cached-SDF gradient.
- G2P now has three paths:
  - skip exact terrain work when cached SDF is safely outside the collision margin plus slack;
  - project directly with cached SDF/normal when cached SDF is clearly colliding;
  - fall back to exact collider sampling for invalid cache data, out-of-grid particles, ambiguous near-surface particles, or invalid gradients.
- Perf logging shadow-samples a deterministic subset of cached skip/projection decisions with the exact collider and reports false skips plus cached-vs-exact SDF error.

Release benchmark:

Run logs:

- `/tmp/re-flora-logs/re-flora-20260516-231428.005-119895.log`
- `/tmp/re-flora-logs/re-flora-20260516-231849.292-121006.log`
- `/tmp/re-flora-logs/re-flora-20260516-232008.680-121537.log`

| metric | phase 3A sample B | phase 3B initial | phase 3B tuned A | phase 3B tuned B | phase 3B tuned C |
| --- | ---: | ---: | ---: | ---: | ---: |
| substeps/report | 240 | 240 | 217 | 240 | 241 |
| total water time | 468.63 ms | 432.12 ms | 401.04 ms | 428.25 ms | 421.42 ms |
| avg/substep | 1.953 ms | 1.800 ms | 1.848 ms | 1.784 ms | 1.749 ms |
| repair | 12.59 ms | 12.64 ms | 11.20 ms | 12.41 ms | 12.46 ms |
| grid | 18.13 ms | 16.01 ms | 16.97 ms | 17.31 ms | 16.32 ms |
| G2P total | 359.94 ms | 326.03 ms | 302.16 ms | 318.84 ms | 313.19 ms |
| G2P terrain | 112.77 ms | 104.68 ms | 105.17 ms | 98.11 ms | 93.75 ms |
| cache skips/substep | n/a | 847 | 741 | 1635 | 1602 |
| cache projections/substep | n/a | 572 | 1233 | 409 | 572 |
| exact fallbacks/substep | n/a | 2677 | 2122 | 2051 | 1922 |
| exact checks/substep | 4031 | 2677 | 2122 | 2051 | 1922 |
| exact corrections/substep | n/a | 88 | 224 | 117 | 88 |
| shadow samples/substep | n/a | 25.2 | 13.0 | 23.1 | 26.9 |
| shadow false skips | n/a | 0 | 0 | 0 | 0 |
| shadow SDF avg abs error | n/a | 0.00183 | 0.00143 | 0.00157 | 0.00185 |
| shadow SDF max abs error | n/a | 0.00367 | 0.00624 | 0.00471 | 0.00441 |
| penetrating | 0 | 0 | 0 | 0 | 0 |
| no_sdf | 0 | 0 | 0 | 0 | 0 |

Interpretation:

- Overall water step cost improved again, from ~1.95-1.99 ms/substep after Phase 3A to ~1.75-1.85 ms/substep after Phase 3B tuning.
- Exact G2P terrain checks dropped from almost all particles to roughly 47-52% of particles per substep in the tuned samples.
- Cached projection is active and release validation stayed clean in these runs.
- Shadow validation sampled cached decisions and found no false skips in the latest release runs.
- Reducing interpolation slack from `dx * 0.5` to `dx * 0.25` improved exact fallback counts without causing observed penetration or false skips.
- `g2p_terrain` improved only modestly because many particles still take exact fallback and the cached query/projection path has its own cost.

Recommended next steps:

1. Soak the cached path across several release hidden runs and terrain-edit scenarios.
2. Tune fallback slack only while `terrain_shadow_false_skips` remains zero and `penetrating 0` stays stable.
3. Consider storing a cached gradient/normal for all terrain-grid nodes, not just near-surface nodes, if it reduces query cost or improves cached projection quality.
4. If exact fallbacks remain high after tuning, consider a coarser per-cell classification cache: empty / cached-projectable / exact-required.

Phase 3B completion criteria:

- `penetrating 0` and `no_sdf 0` remain true across several release hidden runs.
- Shadow verification reports no missed terrain contacts for skipped particles.
- Exact G2P terrain checks are substantially below particle count in representative scenes.
- `g2p_terrain` is no longer a dominant sub-cost relative to G2P gather, or further reductions require solver-level changes.

## Later phases

### Phase 4: reduce terrain collider scope and startup work

Status: implemented and release-benchmarked; still useful to soak with terrain-edit scenarios.

Goal: avoid building and publishing colliders that cannot affect the pond.

Implemented work:

1. Startup collider refresh now only enqueues chunks in the water grid domain: `floor(water_min)..=floor(water_max)`.
2. Terrain edit invalidation also skips source refresh for chunks outside the water grid domain.
3. Startup logging reports enqueued water-domain chunks and skipped out-of-domain chunks.
4. Direct chunk lookup and scan fallback behavior remain intact.

Expected effect: lower startup GPU solid readback, fewer worker jobs, fewer collider chunks published, and smaller terrain cache rebuild input. For the current fixed water box, the startup candidate set drops from the full `5 x 2 x 5 = 50` terrain chunks to at most `2 x 2 x 2 = 8` water-domain chunks before empty-terrain filtering.

Release validation:

- Run log: `/tmp/re-flora-logs/re-flora-20260516-233023.731-125389.log`
- Startup log: `enqueued startup collider rebuilds for 8 water-domain chunks, skipped 42 out-of-domain chunks`.
- Water perf stayed in the Phase 3B range:
  - `1.868 ms/substep`, then `1.764 ms/substep`, then `1.740 ms/substep`.
  - `terrain_shadow_false_skips 0`, `penetrating 0`, `no_sdf 0`.
- This phase mainly reduces startup/edit collider scope; it is not expected to materially change steady-state solver time.

### Phase 5: solver-level scaling options

Status: initial implementation complete; needs visual/gameplay soak before becoming the default.

Goal: preserve appearance while lowering total CPU budget after hot-loop waste is removed.

Implemented work:

1. Added release-benchmark sweep knobs:
   - `--water-particles <N>`
   - `--water-grid <N>` for cubic grid dimension
   - `--water-substep-hz <Hz>` for fixed substep rate
2. Added named water profile selection:
   - `--water-profile default`: current quality/default config, 4096 particles, 32^3 grid, 240 Hz substeps.
   - `--water-profile performance`: lower-CPU candidate, 2048 particles, 32^3 grid, 120 Hz substeps.
3. Water startup logs the selected profile, effective particle count, grid dimension, and substep dt.
4. Changing particle count preserves the intended total fill volume by rescaling per-particle volume.
5. Explicit particle/grid/substep CLI overrides are applied after the named profile, so profiles remain easy to tweak during sweeps.

Initial release validation:

- Default config log: `/tmp/re-flora-logs/re-flora-20260516-233315.367-126594.log`
  - effective config: `particles=4096 grid=UVec3(32, 32, 32) substep_dt=0.004167s`
  - representative samples: `1.854`, `1.790`, `1.781 ms/substep`
  - `terrain_shadow_false_skips 0`, `penetrating 0`, `no_sdf 0`
- Half-particle sweep log: `/tmp/re-flora-logs/re-flora-20260516-233332.832-126982.log`
  - command: `--water-particles 2048`
  - effective config: `particles=2048 grid=UVec3(32, 32, 32) substep_dt=0.004167s`
  - representative samples: `0.984`, `0.941`, `0.927 ms/substep`
  - `terrain_shadow_false_skips 0`, `penetrating 0`, `no_sdf 0`
  - expected linear-ish particle scaling is visible in P2G/G2P and particle upload.

Additional Phase 5 sweeps:

| config | log | representative avg/substep | notes |
| --- | --- | ---: | --- |
| default: 4096 particles, 32^3, 240 Hz | `/tmp/re-flora-logs/re-flora-20260516-233315.367-126594.log` | 1.78-1.85 ms | current quality/default baseline |
| 2048 particles, 32^3, 240 Hz | `/tmp/re-flora-logs/re-flora-20260516-233332.832-126982.log` | 0.93-0.98 ms | best pure particle-count win; keeps 240 Hz stability |
| 4096 particles, 24^3, 240 Hz | `/tmp/re-flora-logs/re-flora-20260516-234751.534-128074.log` | 1.86-1.91 ms | grid work drops, but coarser cached SDF raises exact fallbacks/G2P terrain; not a win |
| 4096 particles, 16^3, 240 Hz | `/tmp/re-flora-logs/re-flora-20260516-234804.067-128384.log` | 2.05-2.08 ms | too coarse; many exact fallbacks/corrections; not a win |
| 4096 particles, 32^3, 120 Hz | `/tmp/re-flora-logs/re-flora-20260516-234816.156-128686.log` | 1.77-1.82 ms | per-substep similar to default, but roughly half the substeps per second |
| 2048 particles, 32^3, 120 Hz | `/tmp/re-flora-logs/re-flora-20260516-234832.306-128986.log` | 0.95-0.99 ms | best measured low-CPU candidate |
| `--water-profile performance` | `/tmp/re-flora-logs/re-flora-20260516-235018.242-129739.log` | 0.96-0.99 ms | profile maps to 2048 particles, 32^3 grid, 120 Hz |

Interpretation:

- Reducing particle count scales the dominant P2G/G2P work nearly linearly and also halves visual water-debug upload cost.
- Reducing grid resolution alone is counterproductive in this scene because the cached terrain broadphase gets less precise; grid-node work drops but G2P terrain exact fallbacks/corrections rise.
- Reducing substep rate to 120 Hz does not change per-substep cost much, but roughly halves the number of water substeps per second. It needs visual/gameplay soak for stability and appearance.
- The initial performance profile is the best measured low-CPU candidate: 2048 particles, 32^3 grid, 120 Hz.
- All sweep samples above kept `terrain_shadow_false_skips 0`, `penetrating 0`, and `no_sdf 0` in release hidden perf runs.

Remaining work:

1. Visual/gameplay soak the performance profile in visible app runs and during terrain edits.
2. Decide whether `--water-profile performance` is opt-in only or should become an exposed settings preset.
3. Try adaptive CFL limits only if the fixed 120 Hz profile shows instability or visual artifacts.
4. Consider CPU parallelism or GPU/storage-buffer paths only after profile soak and cached G2P collision tuning.

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
- `terrain_shadow_samples/substep`
- `terrain_shadow_false_skips`
- `terrain_shadow_sdf_err_avg`
- `terrain_shadow_sdf_err_max`
- `penetrating`
- `no_sdf`

Functional acceptance:

- `cargo check` passes.
- `cargo test` passes.
- Hidden release run exits successfully.
- Latest log has no water, Vulkan, or shader errors.
- `penetrating 0` and `no_sdf 0` stay stable in representative release perf logs.
