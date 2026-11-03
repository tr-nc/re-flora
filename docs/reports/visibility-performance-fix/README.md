# Grass visibility performance regression — measured follow-up

2026-11-03 · [Visual report](index.html) · implementation `25d91848`.
The [prior report](../source-space-visibility/index.html) remains historical
evidence of the rejected result, not current performance.

## Evidence and limits

- `measurements.json`: final Release binary/camera SHA-256, all six dense native
  runs, verdict and medians. Apple M4 Pro / Vulkan–MoltenVK, selected lights 8,
  seed 20260519, camera garden-arrival, time 0.95, probe spacing **32**,
  screen 2560×1440 / scene 512×288. Both modes use the same binary.
- Three 30-second pairs in A/B, B/A, A/B order, audit disabled. Last 8 seconds
  measure steady GPU time and actual frame-advance/wall-clock FPS.
  Whole-profiled FPS starts at the first PERF frame and includes construction
  and restarts, but excludes process launch/loading before that first frame.
- Grass median **28.472 → 22.812 ms**; steady FPS **17.2608 → 18.6998**;
  whole-profiled FPS **17.1465 → 18.5292**. All three individual pairs improve
  all metrics. 419/448 steady GPU samples, not confidence intervals.
- Shared runs each have 162 producer dispatches, about 0.553 seconds total GPU.
  Cold/invalidation cost remains. No every-frame or continuous-edit speedup
  guarantee; no broader-device acceptance or new RTX measurement.
- `receiver-audit.txt`: verbatim relevant lines, prefixed with local log paths,
  from default and seed-7 opaque-guardian runs. Separate observer dispatch reads
  the **normal fast program's output**, without replacing it. Same-frame
  reference transmission and RGB checks have zero sampled tolerance failures;
  maximum observed RGB difference 4.656613e-10. Last stable finite-source
  direct-luma sharing 74.78435% / 86.109006%, roughly 1/1024 receiver sampling,
  not all-pixel/all-light correctness or total direct+indirect light shares.
- No probe density, radiometry, exposure, indirect transport, geometry or
  quadrature change. Saved A/B remains off. Full post-readiness real-grass
  source/blocker lifecycle coverage remains incomplete.
- Existing seed-7 + Glass guardian DDGI non-finite-record panic and five
  historical standalone Slang failures are not fixed here. Baseline evidence
  remains in the prior report.

## Reproduction

Repository root, no other GPU application or browser running:

```sh
cargo build --release
node --test scripts/benchmark-local-visibility.test.mjs
node scripts/benchmark-local-visibility.mjs --repeats 3 \
  --output target/visibility-perf/a-new-directory
```

The [script](../../../scripts/benchmark-local-visibility.mjs) rejects any
median grass GPU, steady FPS or whole-profiled FPS regression; requires
publication, no steady rebuild and dense samples; restores GUI bytes and
checks binary/camera hashes. Thresholds were not relaxed. One pair is diagnostic.

Separate correctness runs: set capture GUI time 0.95/shared true; restore
0.47/false afterwards:

```sh
RE_FLORA_LOCAL_LIGHT_FIELD_AUDIT=1 target/release/re-flora \
  --hidden --windowed --mute --perf --environment-probe-spacing-voxels 32 \
  --screenshot garden-arrival target/visibility-perf/output-audit.png \
  --screenshot-delay 18 --auto-exit 30
# Repeat with --garden-seed 7, opaque guardian.
```

Ignored local raw logs:
`target/visibility-perf/final-acceptance/{0,1,2}-{off,on}.log`,
`target/visibility-perf/output-audit{,-seed7}.log`.
They are not promised to survive checkout; committed JSON/excerpts preserve
the reported evidence.

Validation: cargo fmt/check/test; 1834 app + 4 library tests, 15 app ignored;
76 re-flora-vkn tests including reflected and actual optimized SPIR-V for
all twelve original/shared/audit opaque/Glass/band variants; 21,805 certificate
and 441 endpoint checks; 6 benchmark tests; Release and hidden/muted native
smoke with log inspection. The historical native regression is still rejected
by the replay gate (exit 1). Three existing Rust warnings remain. The complete
standalone Slang suite was not rerun.

HTML was opened and visually inspected at 1440×1000 and 390×844: no page
horizontal overflow, six table rows, keyboard table scrolling, expandable
reproduction details and history/data navigation verified. Automated axe audit:
0 violations, 42 passes, 0 incomplete; not an exhaustive accessibility guarantee.

## Root cause and provenance

Regression introduced with `2e548ae4`: runtime-disabled audit remained in the
production compute program. Loop control did not help. Compiling auditing out
reversed the regression, but also changed original codegen, so that diagnostic
was not accepted as the fix. Distinct original/shared/observer programs exclude
unused resources, keep math single-sourced and opt-in verification intact.
Hardware register/spill counters were not measured.

Charts are HTML/CSS based on recorded data, no imported or generated image/audio
assets, image model or third-party artwork. No previous screenshot is presented
as current-build photographic evidence. Browser validation captures are under
ignored `target/visibility-perf/`.
