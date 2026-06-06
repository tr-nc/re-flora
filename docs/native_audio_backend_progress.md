# Native Audio Backend Progress

## Goal

Replace the fragile and slow PetalSonic -> audionimbus -> Steam Audio spatialization path with a native, game-specific audio backend that re-flora can control and profile.

Done means:

- re-flora can run spatial audio without requiring Steam Audio / `libphonon` for the default path;
- direct sound, HRTF spatialization, occlusion/transmission, early reflections, and late reverb have native implementations appropriate for the game;
- reflection/ray work can use re-flora's own scene data and GPU/parallel query path where useful, without blocking the realtime audio callback;
- performance and audio quality are validated against concrete logs, benchmarks, and listening/reference checks;
- Steam Audio packaging and runtime fallback decisions are explicit.

## Current State

Known re-flora audio path:

- Current branch: `audio-optimization` in the main worktree. No worker worktree was created.
- `Cargo.toml` depends on local PetalSonic: `petalsonic = { path = "../petalsonic/petalsonic" }`.
- `src/audio/spatial_sound_manager.rs` creates `PetalSonicWorldDesc`, passes `hrtf_path`, `distance_scaler`, and `batched_any_hit_ray_tracer`, then registers static/procedural spatial sources.
- `src/audio/tree_rustle.rs` is already native Rust procedural DSP and feeds PetalSonic as mono procedural source content.
- `src/builder/contree/mod.rs` exposes `ContreeAnyHitRayTracer` and CPU terrain any-hit queries for direct occlusion.
- `config/gui.toml` has `audio_ray_tracing_enabled`, wired through `SpatialSoundManager::set_audio_ray_tracing_enabled()`.

Known PetalSonic state:

- Local crate is at `../petalsonic/petalsonic`.
- `../petalsonic/petalsonic/Cargo.toml` depends on `audionimbus = "0.12.0"`; audionimbus pulls in Steam Audio / `libphonon`.
- Relevant PetalSonic files include `src/spatial/processor.rs`, `src/spatial/hrtf.rs`, `src/spatial/effects.rs`, `src/acoustics.rs`, `src/engine.rs`, `src/world.rs`, and `src/config/world_desc.rs`.
- Steam Audio currently owns HRTF loading, direct effect, ambisonics encode/decode, simulator direct pass, and reflection effect.
- re-flora currently passes only a `BatchedAnyHitRayTracer`; it does not pass a `BatchedClosestHitRayTracer`, so Steam Audio reflections are effectively not connected from re-flora by default.
- PetalSonic reflection constants are currently minimal (`1` ray, `1` diffuse sample, `1` bounce, `1` thread, short duration), which is not enough for high-quality indirect simulation.

Relevant packaging/build state:

- `build.rs`, `docs/packaging.md`, and `scripts/package_release.py` still know about `libphonon` / `phonon.dll` packaging.
- Official packages currently bundle Steam Audio runtime libraries.

Constraints:

- Keep the audio callback realtime-safe: no blocking locks, allocations, command spam, GPU waits, file I/O, or expensive unpredictable work in the callback.
- Use release-mode app runs and logs for performance evidence; debug builds and unit tests are not performance proof.
- Run `cargo check` after Rust changes.
- Validate app-level audio/rendering health with `cargo run --release -- --hidden --auto-exit 0.5` and inspect logs.
- Do not remove Steam Audio packaging until the default backend no longer needs it and release behavior is confirmed.

Assumptions to confirm:

- The desired default direction is a native PetalSonic backend, with Steam Audio retained only as temporary fallback during migration.
- The current HRTF asset `assets/hrtf/hrtf_b_nh172.sofa` can be converted offline into a runtime-friendly table rather than parsed as SOFA at runtime.
- GPU ray-query integration should be asynchronous and double-buffered; the audio render path should consume the latest completed acoustic field, not wait for current-frame GPU results.

## Plan / Phases

### Phase 0 - Baseline and backend boundary

- Objective: Establish the current performance/quality baseline and add a clear backend boundary without changing default behavior.
- Expected output: Timing/log coverage for current PetalSonic spatial processing, source counts, direct occlusion cost, and reflection status; documented backend ownership boundary.
- Dependencies/blockers: Need inspect PetalSonic enough to choose a boundary that avoids churn in re-flora source lifecycle code.
- Status: in progress.

### Phase 1 - Native direct sound and occlusion prototype

- Objective: Implement native direct-path processing for distance attenuation, simple air absorption, direct occlusion, and optional transmission.
- Expected output: Native direct path that can use existing re-flora `ContreeAnyHitRayTracer` or equivalent cached query data and can be selected independently of Steam Audio.
- Dependencies/blockers: Need decide whether direct occlusion updates happen inside PetalSonic, re-flora, or a small shared acoustics module.
- Status: not started.

### Phase 2 - Native HRTF path

- Objective: Replace Steam Audio HRTF/ambisonics binauralization for direct sources with native HRTF convolution/interpolation.
- Expected output: Runtime-friendly HRTF asset format, loader, direction lookup/interpolation, per-source stereo render path, and tests for stability/gain/channel behavior.
- Dependencies/blockers: Need choose offline SOFA conversion approach and confirm license/asset suitability for generated HRTF tables.
- Status: not started.

### Phase 3 - Native early reflections

- Objective: Build a game-specific early reflection system that can use batched scene ray queries and produce musically useful delay taps.
- Expected output: Asynchronous acoustic ray job path, reflection tap cache per listener/source region, smoothing, and render-time delay/FIR application.
- Dependencies/blockers: Need decide CPU vs GPU first implementation; GPU path must not stall audio. Need material model for terrain/vegetation/water or safe defaults.
- Status: not started.

### Phase 4 - Native late reverb / ambience field

- Objective: Add low-cost late reverberation suitable for outdoor/terrain scenes.
- Expected output: Parameterized FDN/Schroeder-style reverb or ambience send model driven by scene openness, occlusion, and reflection probes.
- Dependencies/blockers: Needs basic acoustic environment metrics from Phase 3 or a simpler initial heuristic.
- Status: not started.

### Phase 5 - Migration, fallback, and packaging cleanup

- Objective: Make the native backend the default if it meets quality/performance goals, then remove or demote Steam Audio dependency and packaging.
- Expected output: Config/feature-gated backend selection, updated package scripts/docs, no default `libphonon` dependency if native backend is accepted.
- Dependencies/blockers: Requires Phase 1-4 validation and decision on whether to keep Steam Audio fallback for development/comparison.
- Status: not started.

## Verification Method

Baseline checks before changing behavior:

```bash
git status --short --branch
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

PetalSonic checks when the local dependency changes:

```bash
cd ../petalsonic/petalsonic
cargo fmt --check
cargo test
```

Correctness checks:

- Direct path: source distance/gain curves match expected attenuation within tolerance; occluded and unoccluded rays produce expected gain/filter changes in deterministic tests.
- HRTF path: left/right symmetry sanity checks, no NaN/Inf output, bounded gain, stable output for static direction, smooth output under direction changes.
- Reflections: deterministic small-scene tests for wall/floor bounce distances; no audio-thread waits; reflection taps are smoothed and bounded.
- Reverb: impulse response decay is bounded, denormal-safe, and free of runaway feedback.
- App run: hidden release run exits cleanly and inspected log has no audio engine errors, callback panics, persistent underruns, or unexpected Steam Audio dependency use when native backend is selected.

Performance checks:

- Add/inspect timing for direct processing, HRTF processing, acoustic query submission/completion, reflection rendering, and total audio pump cost.
- Compare release-mode native backend timings against current Steam Audio/audionimbus baseline with similar source counts.
- Confirm audio callback only consumes pre-rendered/double-buffered data and never waits for GPU or scene locks.

Manual/audio quality checks:

- Listen with moving listener and fixed sources: front/back, left/right, near/far, behind obstacle, and open terrain.
- Compare tree rustle and ambience with native spatialization against current PetalSonic path.
- For reflections/reverb, use controlled scenes where a wall/terrain obstruction should create obvious but not exaggerated spatial response.

Verification gaps:

- Native backend does not exist yet, so acceptance criteria beyond baseline logging are not yet runnable.
- No current GPU acoustic ray job path exists for audio reflections.
- No offline SOFA-to-native-HRTF conversion tool has been selected or implemented.

## Progress Log

- 2026-06-06: Created branch `audio-optimization` in the existing worktree as requested; no new worktree was created.
- 2026-06-06: Inspected re-flora audio entry points and confirmed the game uses local PetalSonic through `src/audio/spatial_sound_manager.rs`.
- 2026-06-06: Inspected PetalSonic dependency state and confirmed spatial processing currently depends on `audionimbus` / Steam Audio.
- 2026-06-06: Confirmed current re-flora integration passes any-hit direct occlusion but not closest-hit reflection data, so Steam Audio reflections are not meaningfully integrated for the game yet.
- 2026-06-06: Decided the safest migration strategy is staged replacement: native direct path first, then native HRTF, then early reflections and late reverb, with Steam Audio retained as fallback until validation supports removal.
- 2026-06-06: Created this progress document to track scope, phases, verification, and risks before implementation.

## Open Questions / Risks

- Should native spatial audio live entirely inside PetalSonic, or should re-flora own some acoustics systems and feed summarized results to PetalSonic?
- What is the desired backend API: compile-time feature, runtime config, or both?
- Can the current SOFA HRTF asset be legally and technically converted into a compact checked-in table?
- Which scene materials matter acoustically for the first version: terrain only, vegetation, water, structures, or all of them?
- How much reflection quality is actually needed for re-flora's outdoor scene style versus a cheaper ambience/reverb approximation?
- GPU acoustic ray tracing may add latency and synchronization complexity; design must avoid stalling graphics or audio.
- Removing Steam Audio affects release packaging across Windows, macOS, and Linux and should happen only after native backend validation.
- Keeping both backends too long may increase maintenance burden; removing the fallback too early may make quality regressions harder to compare.
