# Terrain edit lighting

Rollback: pushed annotated `checkpoint/terrain-edit-lighting-start-ad7e4bd6`
(`ad7e4bd6d8e5aa20b112e23d177f7f8e9caf8869`).

## Reproduction and baseline

`python3 scripts/check_ddgi_sustained_edits.py target/edit-lighting/baseline-no-capture`
(builds release; GPU lock `/tmp/re-flora-summer-gpu.lock`; restores GUI/camera bytes).
The procedural skylight alternates 40 real terrain publications, at least 100ms apart,
independent of DDGI readiness. This is a scheduling repro, not a replay of the user's
saved cavity or a pixel-level blackness assertion. The runner fails unless at least two
complete fields promote **between** begin/end markers. No GPU test is added to cargo test.

Baseline: **RED**, 40 edits, zero promotions during the gesture, no ERROR/panic/VUID.
Last edit 04:09:15.529; first useful promotion 04:09:17.494: **1965ms after release**.
Several fully computed candidates were discarded. Matched active-gesture GPU render scope:
mean 3433.89us, maximum 4540us (sparse profiler samples, not total frame latency).
Artifacts: `target/edit-lighting/baseline-no-capture/{console.log,report.json,run-log-path.txt}`.
Initial capture-enabled attempt in `target/edit-lighting/baseline/` exposed an existing
`DDGI visibility sample evidence owner version is inconsistent` panic; production/no-capture
repro succeeds. Capture/readback acceptance is not silently claimed to pass.

Ranked hypotheses after establishing the red loop:
1. Latest-only rejection starves continuous edits: permitting bounded complete progressive
   publications should produce promotions during the gesture.
2. Private local-stability gating adds the post-release delay: publishing finite e0 rather than
   waiting for stable e5 should reduce first-publication latency.
3. Missing usable probe support is rendered as zero irradiance: an explicit support validity
   fallback should affect unavailable samples, but not valid black or occluded samples.

Stage-one validation: cargo fmt --check, cargo check, cargo test (1019 passed, 2 ignored),
hidden muted release smoke (0.5s), full smoke/repro logs checked. Smoke canonical log:
`target/re-flora-logs/re-flora-20260919-041148.119-170202.log`. No generated files changed.

## Missing-support fallback

Debug → Shadow → **Terrain Missing Lighting (albedo strength)** is a saved declarative
float (default 0.035, range 0–0.2; zero disables). Terrain final shading multiplies this
neutral display irradiance by the original material albedo only when the query has no
trustworthy geometric support. The query carries `lighting_available` explicitly. Global
sky is available; supported zero/near-zero irradiance and visibility-occluded lighting
remain unchanged. There is no luminance floor, no material replacement, and no feedback of
the fallback into DDGI transport. Diagnostic irradiance captures remain physical/raw;
only final terrain display receives the fallback. Raster/tree shading is unchanged.

Fallback validation: cargo check, cargo fmt --check, cargo test (1019 passed, 2 ignored),
all 16 Slang CPU tests (`python3 scripts/run_slang_tests.py`, including missing/valid-dark/
valid-colored truth table), hidden muted release smoke; no ERROR/panic/VUID.
Generated GPU structs and GUI adjustables changed from their shader/config sources.
This stage alone intentionally does not resolve publication starvation.

## Progressive publication

Root cause confirmed: both latest-only candidate rejection and private local-stability gating
starved the edit stream. Complete finite epoch zero now promotes; local recovery continues
without the former private e5 barrier. Newer requests and the conservative accumulated bounds
survive older publication. The next build coalesces to the latest revision, using the newly
resident complete field as history. Existing terrain/camera-priority batch ordering is retained.
Density arbitration remains latest-only. Descriptor/atlas publication is still atomic and
owner-validated; no partial batch, synchronous rebuild, sleep, or extra event-time work was added.

Important precision: progressive fields retain their original build revision; they do **not**
claim current-geometry readiness. Terrain changes can occur between asynchronous probe batches,
so these are complete temporal approximations, not immutable snapshots of the voxel scene.
Current exact occupancy remains authoritative for receiver visibility. Once input stops, the
latest geometry gets its own field and continued recovery/convergence. This explicitly trades
transient inaccuracies/flicker for responsiveness, as requested.

`cargo test sustained_edits_publish_progress` was run RED before the change (the actual
coordinator rejected its complete candidate). Full cargo tests now pass (1021, 2 ignored),
including complete-e0 readiness, retained pending bounds, token retirement, density arbitration,
and descriptor lineage guards. `cargo check`, `cargo fmt --check`, and hidden release smoke pass;
smoke log `target/re-flora-logs/re-flora-20260919-042722.662-183770.log`.

`python3 scripts/check_ddgi_sustained_edits.py target/edit-lighting/progressive`: **GREEN**,
40 edits, **21 complete promotions during editing**, no ERROR/panic/VUID. Active-gesture GPU
render mean 3523.33us, maximum 4371us vs baseline 3433.89us / 4540us: +2.6% mean in this
single sparse-sample run, not a statistically qualified performance improvement. Same windowed
scene, camera, spacing, flags and config; artifacts beside the baseline. No geometry/material,
editing, physics, tree, or GPU budget changes. No generated files changed in this stage.

Legacy DDGI acceptance scripts that require obsolete terrain rejection or private e4/e5 recovery
encode the superseded policy. Their historical capture assertions are **not** evidence for this
new publication contract; capture-enabled sustained testing is additionally blocked by the
baseline evidence-owner panic above. The new production-path sustained runner is the current
publication-liveness regression. Full legacy acceptance migration remains a limitation, not a
claimed pass.
