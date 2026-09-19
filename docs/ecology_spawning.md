# Shared vegetation ecology

Grass (both registered kinds), authored plants and committed tree foliage can supply butterflies
and cicada calls. Static vegetation continues supplying new opportunities and newly sampled
positions; planting and world loading do not trigger births.

## Ownership and data flow

- `src/ecology.rs` owns a small weighted region index, independent random opportunity clocks and
  bounded per-region rests. No GPU resources, audio handles or particle handles live there.
- `SurfaceBuilder` publishes chunk/species counts and samples one actual instance on demand.
  Zero-growth roots and invalid coordinates are rejected. A root's identity excludes changing
  growth bits but includes its spawn time. Existing load/edit/growth jobs already wait for their
  own completion before the app observation hook; this observer introduces no GPU wait or dispatch.
- The tree owner exposes counts and bounds, and samples its existing committed leaf-position
  array. Canopy generations invalidate obsolete hosts. The old expanded butterfly position copy
  and dependence on the acoustic sample layout have been removed.
- `src/app/core/ambient_ecology.rs` publishes metadata every 0.5 seconds, checks active sound hosts,
  resolves candidates, and passes accepted sites to the two consumers.
- `ButterflyEmitter` owns appearance, lifetime, movement and capacity. It no longer maintains a
  source list or a second spawning clock. A butterfly may outlive the plant where it emerged.
- `SummerCicadas` owns finite spatial audio calls, including end-of-call and lost-host retirement.
  World replacement clears the region index, clocks, rests, active calls and old-world butterflies.

The index refresh is O(chunks + trees), not O(blades + leaf voxels). It is currently a low-frequency
metadata rebuild, **not a fully incremental spatial index**. Candidate selection uses cumulative
weights and binary search. A very large number of chunks/trees needs separate profiling; current
acceptance does not establish unbounded-world scalability.

## Initial tuning

Counts are normalized by kind: 1,000 grass instances, 8 authored plants, or 200 leaf positions.
Each region contributes `preference * normalized / (1 + normalized)`. Butterflies prefer authored
plants by 1.5x; cicadas prefer canopy by 1.5x. All kinds retain a positive weight.

Total opportunity intensity is `maximum * total_weight / (1 + total_weight)`, with maximum 0.20/s
for butterflies and 0.25/s for cicadas. The clock adds an exponential random delay plus a minimum
1.5s / 2.7s gap respectively. Actual births are lower when candidates are invalid, areas are resting
or capacity is full. These are initial gameplay settings, not a biological population model.

The existing butterfly config key `butterfly_spawn_rate_per_source` is retained for compatibility;
0.00002 now means normal habitat rate, 0 disables it, and the multiplier is capped at 10. Its GUI
label describes the new meaning. No per-blade linear rate remains.

- Nearby candidate radius: 1.4 world units; actual sampled positions are checked again.
- Maximum one opportunity per animal per frame, two candidate attempts per opportunity.
- Maximum four candidate samples per frame; active cicada validation adds at most three samples
  on the 0.5-second observation cadence. A grass/plant sample copies one 8-byte instance.
- Maximum three active cicada calls; calls cannot start less than 2.7 seconds apart.
- A cicada region rests 24–40 seconds after an accepted start. Clips are at most 15 seconds.
- Butterfly world capacity retains the existing two-per-world-chunk rule; a radius of 0.22 may
  contain at most two butterflies at birth. Cicada starts require 0.22 separation from active calls.
- Empty, disabled and full populations disarm their clocks. Re-entry draws a new delay; no backlog
  is replayed. Source metadata changes do not reset a running clock.

The radius check can reject a sampled point from a partially nearby region. There are no unbounded
retries. This intentionally trades lower accepted rates near boundaries for a hard work budget.

## Repeatable real-app evidence

All commands run from this worktree, using `CARGO_BUILD_JOBS=2`, the default local target, and
`flock /tmp/re-flora-summer-gpu.lock` for app/GPU runs. Preserve `config/gui.toml` across runs.

```sh
CARGO_BUILD_JOBS=2 cargo fmt --check
CARGO_BUILD_JOBS=2 cargo check
flock /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo test
flock /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run --release -- --hidden --mute --auto-exit 0.5
```

Opt-in `RE_FLORA_ECOLOGY_SCENE=grass|plants|canopy|dense|empty` replaces vegetation through normal
editing paths, creates the chosen fixture, and saves it under
`target/summer-evidence/ecology-<scene>.rflterrain`. It does not alter the user's original input save.
No further planting occurs during observation. Use `--camera-snapshot bright-pix` for nearby review.

`RE_FLORA_ECOLOGY_SMOKE=1` saves the current live garden, waits for sounding hosts, checks two world
replacements, removes a sounding host through the ordinary tool path, then clears all vegetation
and verifies twenty seconds without new births. A run is accepted only if its log contains
`[ECOLOGY][SMOKE] passed`; merely exiting successfully before this marker is insufficient.
The old `RE_FLORA_CICADA_SMOKE=1` diagnostic name remains an alias.

`[ECOLOGY][SUPPLY]` logs real instance counts. `[ECOLOGY][BIRTH]` logs animal, kind, region, instance
slot and world position. `[ECOLOGY][PERF]` reports bounded timing samples including index updates,
sampling, host validation and consumer calls. It excludes existing butterfly movement/rendering
and the asynchronous audio engine. `--perf` additionally records whole-frame and GPU scopes.
Use `--latest-log` and `--tail-latest-log 200` from this worktree.

A muted run proves behavior and lifecycle, not listening quality. CPU unit tests are deterministic
logic guardrails, not performance measurements. Release app measurements and manual listening
must be reported separately.
