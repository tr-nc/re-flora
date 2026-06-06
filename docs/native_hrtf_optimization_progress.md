# Native HRTF Optimization Progress

## Goal

Make PetalSonic's native HRTF path fast enough for re-flora while preserving the clearer direct-source localization heard in the native path.

Done means:

- Native HRTF has detailed per-stage profiling for direction lookup, direct processing, HRIR/FIR work, Ambisonics encode/decode, and total per-block cost.
- Native HRTF performance is acceptable under re-flora-like stress loads, or a hybrid fallback policy is explicit.
- Listening comparisons can separate three variables: direct backend, HRTF dataset, and rendering method.
- The chosen default remains realtime-safe: no audio-callback locks, allocations, file I/O, GPU waits, or unpredictable work.

## Current State

Known facts:

- re-flora defaults to `DirectPathBackend::Native` and `HrtfBackend::Native` in `src/audio/spatial_sound_manager.rs`.
- re-flora's GUI can switch Direct Path and HRTF backends at runtime between `Native` and `Steam Audio`.
- Direct occlusion and reflections are intentionally disabled in re-flora now:
  - `direct_occlusion_enabled=false`
  - `reflections_enabled=false`
  - `native_early_reflections_enabled=false`
- Native HRTF uses `assets/hrtf/hrtf_b_nh172.petalhrtf`, converted from `assets/hrtf/hrtf_b_nh172.sofa` by `../petalsonic/tools/sofa_to_petalhrtf.py`.
- Steam HRTF currently uses Steam Audio's default HRTF when selected from the GUI, so native-vs-Steam listening comparisons also include an HRTF dataset difference.
- Native HRTF currently renders every source independently with nearest-direction lookup plus 256-tap time-domain FIR convolution.
- Steam HRTF path encodes each source into a low-order Ambisonics field, sums all sources, then performs one binaural decode. This is much cheaper but can blur localization.

Relevant files:

- PetalSonic native HRTF: `../petalsonic/petalsonic/src/spatial/native_hrtf.rs`
- PetalSonic spatial processor: `../petalsonic/petalsonic/src/spatial/processor.rs`
- PetalSonic Steam effects: `../petalsonic/petalsonic/src/spatial/effects.rs`
- PetalSonic runtime backend switch: `../petalsonic/petalsonic/src/engine.rs`
- re-flora audio integration: `src/audio/spatial_sound_manager.rs`
- re-flora GUI config: `config/gui.toml`

Recent ad hoc release stress result, 1024-frame block, 48 kHz, no occlusion/reflections:

| sources | Native direct + Native HRTF median/p95 | Native direct + Steam HRTF median/p95 |
|---:|---:|---:|
| 36 | 12.47 / 13.19 ms | 0.32 / 0.38 ms |
| 64 | 22.70 / 23.74 ms | 0.47 / 0.52 ms |
| 128 | 44.97 / 46.78 ms | 0.90 / 1.00 ms |
| 256 | 88.86 / 91.66 ms | 1.69 / 1.86 ms |

Assumptions to confirm:

- The native path sounds more correct mainly because it renders per-source HRIRs and/or uses the NH172 dataset; not necessarily because Steam Audio is wrong.
- Steam's blur is likely from order-2 Ambisonics plus default HRTF dataset, but this needs controlled listening tests.
- Steam Audio core source may not be available; if only SDK/docs/bindings are available, research must rely on docs, AudioNimbus bindings, and black-box benchmarks.

## Plan / Phases

### Phase 0 - Profiling harness and metrics

- Objective: Make current costs reproducible and visible.
- Expected output: A checked-in release benchmark or hidden-app stress mode that reports source count, backend combination, HRTF taps, direction lookup time, FIR/render time, Ambisonics encode/decode time, simulation time, and total time.
- Dependencies/blockers: Need decide whether the harness lives in PetalSonic, re-flora CLI, or `tools/`.
- Status: not started. An ad hoc `/tmp` benchmark exists only as temporary evidence.

### Phase 1 - Controlled audio-quality comparison

- Objective: Separate HRTF dataset differences from rendering-method differences.
- Expected output: Listening matrix for native table vs Steam default/SOFA where possible, per-source HRIR vs Ambisonics bus, and order 2/3/4 if available.
- Dependencies/blockers: Need determine whether Steam Audio can load the same SOFA in the current runtime switch path, or add a controlled startup mode.
- Status: not started.

### Phase 2 - Low-risk native FIR optimizations

- Objective: Improve current per-source native HRTF without changing the algorithmic model.
- Expected output: Faster renderer with identical or near-identical output for static directions.
- Candidate work:
  - cache nearest direction index per source and update only when direction changes enough;
  - remove `% taps` from the inner FIR loop by splitting the delay line into contiguous ranges or using a duplicated/ring buffer layout;
  - store HRIR data in cache-friendly left/right contiguous arrays;
  - test tap-count variants such as 64/128/256 taps;
  - consider SIMD after scalar layout is clean.
- Dependencies/blockers: Needs Phase 0 profiling to prove which optimization matters.
- Status: not started.

### Phase 3 - Native Ambisonics bus prototype

- Objective: Recreate the cheap Steam-style path natively: encode many sources into a spherical field, then do one binaural decode.
- Expected output: Native Ambisonics encode + native binaural decode path using the `.petalhrtf` table, with selectable order and performance/quality comparison.
- Dependencies/blockers: Need derive/precompute binaural decoder filters from the native HRTF dataset and choose normalization/order conventions.
- Status: not started.

### Phase 4 - Hybrid renderer

- Objective: Preserve clear localization for important sources while making many-source scenes cheap.
- Expected output: Policy such as "nearest/loudest K sources use per-source native HRIR; remaining sources use native Ambisonics bus or clustered sources".
- Dependencies/blockers: Needs Phase 1 quality results and Phase 2/3 performance results.
- Status: not started.

### Phase 5 - Steam Audio research

- Objective: Learn what Steam Audio is doing and decide which ideas to copy.
- Expected output: Notes with links/citations to available Steam Audio SDK/docs/source or AudioNimbus binding code; clear statement of what is unavailable.
- Dependencies/blockers: Steam Audio core may not be open-source.
- Status: not started.

## Verification Method

Correctness and regression checks:

```bash
cd ../petalsonic/petalsonic
cargo fmt --check
cargo test
cd /home/terence/code/re-flora
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Performance acceptance checks:

- Run all backend combinations in release mode:
  - Native direct + Native HRTF
  - Steam direct + Native HRTF
  - Native direct + Steam HRTF
  - Steam direct + Steam HRTF
- Stress source counts: at least 1, 8, 16, 36, 64, 128, 256.
- Report median, p95, and max per 1024-frame block.
- Keep the 1024-frame / 48 kHz budget in mind: one block is about 21.3 ms of audio.
- Confirm native direct remains cheap and the optimized HRTF work is the actual improvement.

Audio-quality checks:

- Use identical source positions and movement for each backend.
- Compare front/back, left/right, elevation, moving source, and many-source tree rustle scenarios.
- If possible, compare Steam and native using the same SOFA/HRTF dataset before judging algorithm quality.
- Record subjective notes separately for clarity, externalization, front/back confusion, and blur.

Verification gaps:

- No checked-in benchmark harness yet.
- No controlled same-HRTF-data comparison between Steam and native yet.
- No native Ambisonics decoder exists yet.
- No measured optimization result exists beyond the baseline stress test.

## Progress Log

- 2026-06-06: Added runtime GUI switching for Direct Path Backend and HRTF Backend, making listening comparisons possible without restarting.
- 2026-06-06: Confirmed default native path is `Native` direct + `Native` HRTF with direct occlusion and reflections disabled.
- 2026-06-06: Ran an ad hoc release stress benchmark outside the repo. Result: native direct is cheap, but native per-source HRTF is much slower than Steam's Ambisonics HRTF path at high source counts.
- 2026-06-06: Interpreted the performance gap as mostly algorithmic: per-source 256-tap HRIR convolution versus one summed Ambisonics binaural decode.
- 2026-06-06: Noted listening discrepancy: native sounds clearer/more correct to the user, while Steam sounds blurrier; likely causes are Ambisonics order and/or different HRTF datasets.
- 2026-06-06: Created this focused progress document for native HRTF optimization and verification.

## Open Questions / Risks

- Is the preferred native sound due to per-source HRIR rendering, the NH172 HRTF dataset, coordinate mapping, gain, or some combination?
- Can Steam Audio load the same HRTF dataset in the runtime-switch path for a fair comparison?
- What source count should native per-source HRTF support before falling back to bus/cluster rendering?
- What Ambisonics order is the best quality/performance tradeoff for re-flora: 2, 3, 4, or hybrid only?
- Will lower tap counts preserve enough localization and timbre quality?
- Direction-index caching may introduce zipper artifacts unless direction changes are smoothed or crossfaded.
- Native Ambisonics bus may reproduce Steam's blur if the order is too low.
- Steam Audio internals may not be available as source, limiting research to docs/bindings/black-box tests.
- Any optimization must stay off the realtime audio callback; PetalSonic's callback should continue consuming pre-rendered ring-buffer samples only.
