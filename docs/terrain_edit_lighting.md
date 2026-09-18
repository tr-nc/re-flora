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
