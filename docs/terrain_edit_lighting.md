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

## Matched image evidence

The runner now captures `editing.png` at edit 20 (reopened skylight), not after release.
It installs an isolated fixed camera under the GPU lock and restores camera/GUI bytes;
the screenshot checkpoint does not make the lifecycle or raw capture report readiness.

- `target/edit-lighting/matched-baseline32/`: baseline rendering/coordinator sources from
  `2c8656d1`, with the screenshot harness retained. RED, 0 promotions, mean/max GPU render
  3673.56/5293us. Sources were restored immediately and cargo check regenerated bindings.
- `target/edit-lighting/final32/`: GREEN, 22 promotions, mean/max 3510.33/5047us.
- `target/edit-lighting/final16/`: denser stress GREEN, 3 promotions, mean/max 3752/4540us.

All three captured 2560×1440 at the identical camera and GUI SHA-256
`775bd2a7995629e0afea87188bbea34d25a7bfd4f4a33bb3c7a62cd59e3f7008`.
Their reports, command argv, complete console/canonical logs, and screenshots are retained
in those directories. No ERROR/panic/VUID. These images show provisional lighting differences
(including darker areas), not a blanket brightening. They do not establish pixel-level accuracy
or reproduce the user's unavailable saved cavity. Run-to-run timing variability means these
single runs are responsiveness evidence, not a performance acceptance claim.

## Surface validity refinement (current fallback semantics)

An old field can have valid probe support yet describe the solid voxel just removed. Support
alone cannot classify the newly exposed surface. Terrain now also carries an explicit pending
surface bound: outstanding edited voxels plus **one voxel**, not the broad probe-recovery domain.
Within that bound, display uses the authored fallback until a covering complete field publishes.
Outside it, supported physical darkness remains unchanged. This is conservative AABB validity,
not per-face validity: unchanged surfaces within a multi-edit bounding box can temporarily use
the fallback too. It never applies globally or to raster/tree materials. Zero strength deliberately
renders unavailable lighting black; it does not disable tracking of surface validity.

Progressive publication retires only edits covered by its root; edits arriving afterward retain
their own bound. This prevents a long held gesture from marking its entire historical trail
unavailable forever. `cargo test progressive_publication_retires_only` was RED before the change,
GREEN afterward. New GPU-derived uniform fields carry that explicit domain.

The runner now also asserts the newly revealed left skylight receiver is nonblack (ImageMagick,
fixed 16:9 camera). This is independent of liveness and can fail while promotions succeed:

```
python3 scripts/check_ddgi_sustained_edits.py target/edit-lighting/confidence-final32
python3 scripts/check_ddgi_sustained_edits.py target/edit-lighting/negative-control --fallback-strength 0
python3 scripts/check_ddgi_sustained_edits.py target/edit-lighting/confidence-final16 --spacing 16
```

Results: default32 GREEN (22 during-edit promotions, receiver RGB mean 31/255); strength-zero
negative control **RED** (receiver 0/255 despite ongoing promotions); default16 GREEN (3
promotions, receiver 31/255). GUI/camera bytes restored including after the negative run.
Raw images and full logs reside at those paths. `confidence-final32` render mean/max 3472.22/
4289us versus matched baseline 3673.56/5293us, with the same caveat about sparse single runs.
Latest terrain-42 publication after release: baseline **2001ms**, default32 **192ms**, dense16
**1611ms**. During-edit root-to-publication latency at default32 was approximately 194–300ms.

Final refinement validation: fmt/check, 1023 Rust tests passed (2 ignored), all 16 Slang CPU
tests, release hidden smoke (`target/re-flora-logs/re-flora-20260919-044905.373-195379.log`),
and positive/negative real rendering loops. No ERROR/panic/VUID in production logs. GPU structs
were regenerated; unrelated Cargo.lock metadata excluded.
