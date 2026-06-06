# Native HRTF Optimization Progress

## Goal

Make PetalSonic's native HRTF path fast enough for re-flora while preserving the clearer direct-source localization heard in the native path.

Done means:

- Native HRTF has detailed per-stage profiling for direction lookup, direct processing, HRIR/FIR work, Ambisonics encode/decode, and total per-block cost.
- Native HRTF performance is acceptable under re-flora-like stress loads, or a hybrid fallback policy is explicit.
- Listening comparisons use the same high-quality custom HRTF dataset as the primary comparison target; Steam Audio's default HRTF is only a separate baseline, not the main native-vs-Steam comparison.
- Listening comparisons can separate three variables: direct backend, HRTF dataset, and rendering method.
- re-flora has a clear GUI/control model for Ambisonics use: one `Use Native Ambisonics` checkbox. Backend selection stays native in gameplay.
- The chosen default remains realtime-safe: no audio-callback locks, allocations, file I/O, GPU waits, or unpredictable work.

## Current State

Known facts:

- re-flora defaults to `DirectPathBackend::Native`, `HrtfBackend::Native`, `use_ambisonics=false`, and `AmbisonicsBackend::Native` in `src/audio/spatial_sound_manager.rs`.
- re-flora's GUI exposes only `Use Native Ambisonics`; direct path, HRTF, and Ambisonics backend are fixed to native in gameplay.
- Direct occlusion and reflections are intentionally disabled in re-flora now:
  - `direct_occlusion_enabled=false`
  - `reflections_enabled=false`
  - `native_early_reflections_enabled=false`
- Native HRTF uses `assets/hrtf/hrtf_b_nh172.petalhrtf`, converted from `assets/hrtf/hrtf_b_nh172.sofa` by `../petalsonic/tools/sofa_to_petalhrtf.py`.
- Native per-source HRTF renders every source independently with nearest-direction lookup plus 256-tap time-domain FIR convolution. The current scalar renderer caches stable direction indices and avoids `% taps` inside the FIR loop.
- Native Ambisonics order-2 encode/decode now exists. It sums sources into a 9-channel ACN/N3D-style field, then decodes once through binaural FIR filters derived from the `.petalhrtf` table.
- Steam Audio backend comparisons are no longer exposed through the re-flora GUI. The benchmark harness can still run same-dataset Steam comparisons when needed.

Relevant files:

- PetalSonic native HRTF: `../petalsonic/petalsonic/src/spatial/native_hrtf.rs`
- PetalSonic native Ambisonics: `../petalsonic/petalsonic/src/spatial/native_ambisonics.rs`
- PetalSonic spatial processor: `../petalsonic/petalsonic/src/spatial/processor.rs`
- PetalSonic Steam effects: `../petalsonic/petalsonic/src/spatial/effects.rs`
- PetalSonic runtime backend switch: `../petalsonic/petalsonic/src/engine.rs`
- re-flora audio integration: `src/audio/spatial_sound_manager.rs`
- re-flora GUI config: `config/gui.toml`

Latest checked-in benchmark result, `cargo run --release --bin petalsonic_spatial_bench -- --sources 16,64,128,256 --warmup 4 --blocks 20`, 1024-frame block, 48 kHz, no occlusion/reflections:

| sources | Native direct + native per-source HRTF | Native direct + native Ambisonics + native HRTF | Native direct + Steam Ambisonics + Steam HRTF custom |
|---:|---:|---:|---:|
| 16 | 2.75 ms | 1.54 ms | 0.21 ms |
| 64 | 11.00 ms | 1.63 ms | 0.53 ms |
| 128 | 22.18 ms | 1.75 ms | 0.96 ms |
| 256 | 44.49 ms | 2.02 ms | 1.81 ms |

Previous pre-optimization ad hoc result for native per-source HRTF was about 22.7 ms at 64 sources and 88.9 ms at 256 sources, so the scalar FIR cleanup roughly halved that cost. Native Ambisonics makes native HRTF cost mostly source-count-independent after encode.

Assumptions to confirm:

- The native path sounds more correct mainly because it renders per-source HRIRs and/or uses the NH172 dataset; not necessarily because Steam Audio is wrong.
- Steam's blur is likely from order-2 Ambisonics and/or the Steam default HRTF dataset, but this still needs controlled listening tests using the same custom HRTF.
- Gameplay is now intentionally native-only; use the benchmark harness or a dedicated test mode for same-HRTF Steam comparisons.
- Steam Audio core source may not be available; if only SDK/docs/bindings are available, research must rely on docs, AudioNimbus bindings, and black-box benchmarks.

## Plan / Phases

### Phase 0 - Profiling harness and metrics

- Objective: Make current costs reproducible and visible.
- Expected output: A checked-in release benchmark or hidden-app stress mode that reports source count, backend combination, HRTF taps, direction lookup time, FIR/render time, Ambisonics encode/decode time, simulation time, and total time.
- Dependencies/blockers: Need decide whether the harness lives in PetalSonic, re-flora CLI, or `tools/`.
- Status: done. `src/bin/petalsonic_spatial_bench.rs` is checked in and reports per-mode total/direct/encode/decode/HRTF/native-FIR timings.

### Phase 1 - Controlled audio-quality comparison

- Objective: Separate HRTF dataset differences from rendering-method differences.
- Expected output: Listening matrix where the primary native-vs-Steam comparison uses the same NH172/custom HRTF dataset, plus a clearly labeled Steam-default-HRTF baseline. Include per-source HRIR vs Ambisonics bus, and order 2/3/4 if available.
- Dependencies/blockers: Steam Audio now loads `assets/hrtf/hrtf_b_nh172.sofa` in the re-flora runtime switch path. Remaining blocker is manual listening validation.
- Status: in progress.

### Phase 2 - Low-risk native FIR optimizations

- Objective: Improve current per-source native HRTF without changing the algorithmic model.
- Expected output: Faster renderer with identical or near-identical output for static directions.
- Candidate work:
  - cache nearest direction index per source and update only when direction changes enough;
  - remove `% taps` from the inner FIR loop by splitting the delay line into contiguous ranges or using a duplicated/ring buffer layout;
  - store HRIR data in cache-friendly left/right contiguous arrays;
  - test tap-count variants such as 64/128/256 taps;
  - consider SIMD after scalar layout is clean.
- Dependencies/blockers: Further SIMD/cache-layout work should use Phase 0 profiling to prove value.
- Status: in progress. Direction-index caching and modulo-free scalar FIR are implemented and measured; SIMD, tap-count variants, and storage-layout work remain open.

### Phase 3 - Native Ambisonics bus prototype and GUI controls

- Objective: Recreate the cheap Steam-style path natively: encode many sources into a spherical field, then do one binaural decode consumed by the HRTF renderer.
- Expected output: Native Ambisonics encode + native binaural decode path using the `.petalhrtf` table, with selectable order and performance/quality comparison.
- GUI/control model:
  - checkbox: `Use Native Ambisonics`;
  - direct path, HRTF, and Ambisonics backend stay fixed to native in gameplay.
- Dependencies/blockers: Higher-order Ambisonics and normalization/listening validation remain open.
- Status: done for order-2 prototype and simplified GUI control. Native encode, native binaural decode filters derived from `.petalhrtf`, and the `Use Native Ambisonics` checkbox are implemented.

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
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Performance acceptance checks:

- Run all backend combinations in release mode, using the same custom HRTF dataset for native-vs-Steam comparisons when possible:
  - Native direct + Native per-source HRTF
  - Steam direct + Native per-source HRTF
  - Native direct + Native Ambisonics + Native HRTF
  - Steam direct + Native Ambisonics + Native HRTF
  - Native direct + Steam Ambisonics/HRTF with the custom SOFA HRTF
  - Steam direct + Steam Ambisonics/HRTF with the custom SOFA HRTF
  - Steam default HRTF only as a separately labeled baseline
- Stress source counts: at least 1, 8, 16, 36, 64, 128, 256.
- Report median, p95, and max per 1024-frame block.
- Keep the 1024-frame / 48 kHz budget in mind: one block is about 21.3 ms of audio.
- Confirm native direct remains cheap and the optimized HRTF work is the actual improvement.

Audio-quality checks:

- Use identical source positions and movement for each backend.
- Compare front/back, left/right, elevation, moving source, and many-source tree rustle scenarios.
- Primary native-vs-Steam judgments must use the same NH172/custom HRTF dataset. If a run uses Steam Audio default HRTF, label it as `Steam default HRTF baseline` and do not treat it as a fair renderer comparison.
- Record subjective notes separately for clarity, externalization, front/back confusion, and blur.

Verification gaps:

- Manual same-HRTF listening comparison between Steam and native is still pending.
- Native Ambisonics currently supports order 2 only.
- No source-priority hybrid policy exists yet.
- Native Ambisonics decoder filters use simple equal-weight spherical integration over the HRTF table; quality should be validated by listening.

## Progress Log

- 2026-06-06: Added runtime GUI switching for Direct Path Backend and HRTF Backend, making listening comparisons possible without restarting.
- 2026-06-06: Confirmed default native path is `Native` direct + `Native` HRTF with direct occlusion and reflections disabled.
- 2026-06-06: Ran an ad hoc release stress benchmark outside the repo. Result: native direct is cheap, but native per-source HRTF is much slower than Steam's Ambisonics HRTF path at high source counts.
- 2026-06-06: Interpreted the performance gap as mostly algorithmic: per-source 256-tap HRIR convolution versus one summed Ambisonics binaural decode.
- 2026-06-06: Noted listening discrepancy: native sounds clearer/more correct to the user, while Steam sounds blurrier; likely causes are Ambisonics order and/or different HRTF datasets.
- 2026-06-06: Created this focused progress document for native HRTF optimization and verification.
- 2026-06-06: Clarified that future native-vs-Steam comparisons should use the same NH172/custom HRTF dataset; Steam Audio default HRTF should only be a labeled baseline.
- 2026-06-06: Added the planned Ambisonics control model: prefer separate `Use Ambisonics` checkbox plus `Ambisonics Backend` dropdown, but allow a combined rendering-mode dropdown if Ambisonics and HRTF cannot be decoupled.
- 2026-06-06: Implemented native order-2 Ambisonics encode and native binaural decode filters derived from `.petalhrtf`.
- 2026-06-06: Added runtime Ambisonics controls in re-flora: `Use Ambisonics` checkbox and `Ambisonics Backend` dropdown.
- 2026-06-06: Updated re-flora to provide both custom HRTF formats: `.petalhrtf` for native and SOFA for Steam Audio, avoiding Steam default HRTF in normal comparisons.
- 2026-06-06: Simplified re-flora GUI to native-only gameplay controls: removed Direct Path Backend, HRTF Backend, and Ambisonics Backend dropdowns; kept only `Use Native Ambisonics`, default off.
- 2026-06-06: Added detailed timing fields for spatial source count, direct processing, Ambisonics encode/decode, HRTF rendering, native direction lookup, and native FIR convolution.
- 2026-06-06: Added checked-in release benchmark `src/bin/petalsonic_spatial_bench.rs`.
- 2026-06-06: Optimized native per-source FIR by caching stable direction indices and removing `% taps` from the inner convolution loop; release benchmark shows roughly 2x improvement versus previous ad hoc baseline.
- 2026-06-06: Validated with PetalSonic `cargo fmt --check`/`cargo test`, re-flora `cargo fmt --check`/`cargo check`/`cargo test`, benchmark runs, and hidden release app run.

## Open Questions / Risks

- Is the preferred native sound due to per-source HRIR rendering, the NH172 HRTF dataset, coordinate mapping, gain, or some combination?
- Manual listening still needs to confirm whether native Ambisonics quality is acceptable versus native per-source HRTF.
- Native Ambisonics normalization and orientation should be validated by listening.
- Should the default eventually turn `Use Native Ambisonics` on for high-source scenes, or stay per-source for clarity?
- What source count should native per-source HRTF support before falling back to bus/cluster rendering?
- What Ambisonics order is the best quality/performance tradeoff for re-flora: 2, 3, 4, or hybrid only?
- Will lower tap counts preserve enough localization and timbre quality?
- Direction-index caching may introduce zipper artifacts unless direction changes are smoothed or crossfaded.
- Native Ambisonics bus may reproduce Steam's blur if the order is too low.
- Steam Audio internals may not be available as source, limiting research to docs/bindings/black-box tests.
- Any optimization must stay off the realtime audio callback; PetalSonic's callback should continue consuming pre-rendered ring-buffer samples only.
