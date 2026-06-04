# Runtime Tree Rustle Progress

## Goal

Replace the current static tree-rustle WAV loop with runtime-generated procedural rustle that responds to wind strength and tree/canopy state.

Done means:

- tree rustle in-game is generated at runtime, not played from `assets/sfx/tree_sound_48k_pregain_40db.wav` or any replacement rustle WAV asset;
- wind changes affect the sound model itself, especially low/mid bed, 3-6 kHz leaf contact, restrained 8-12 kHz air, and crackle/grain activity;
- PetalSonic supports source-level procedural audio that can still use existing spatialization, volume, occlusion/direct-path, and lifecycle handling;
- the Rust implementation becomes authoritative; Python prototypes remain reference/demo code only;
- performance is measured and acceptable for the active tree-audio source count, with LOD/culling if needed;
- validation includes listening, spectral/envelope checks, and release-mode runtime/audio timing checks.

## Current State

Known re-flora audio path:

- `src/audio/tree_audio_manager.rs` now registers procedural tree rustle factories instead of a hardcoded rustle WAV loop.
- `TreeAudioManager` spawns looping procedural spatial sources through `SpatialSoundManager::add_procedural_looping_spatial_source()`.
- `src/audio/tree_audio_source.rs` samples wind, writes it into a shared `TreeRustleControl`, and keeps PetalSonic source volume as a master/cluster gain.
- `src/audio/tree_rustle.rs` contains the authoritative Rust rustle generator and tests.
- `src/app/core/vegetation.rs` creates tree audio sources from leaf clusters; recent logs show one debug tree with about `36` source clusters before audio-source capping.
- `src/app/core/mod.rs` creates `SpatialSoundManager::new(1024, ...)`, so the current world audio block is `1024` frames.

Known PetalSonic path:

- Local crate is at `/Users/bytedance/code/petalsonic`; re-flora now uses it through `petalsonic = { path = "../petalsonic/petalsonic" }`.
- Relevant files include `petalsonic/src/procedural.rs`, `petalsonic/src/world.rs`, `petalsonic/src/playback.rs`, `petalsonic/src/mixer.rs`, `petalsonic/src/spatial/processor.rs`, and `petalsonic/src/engine.rs`.
- PetalSonic now supports `ProceduralAudioFactory` / `ProceduralAudioSource`, `register_procedural()`, and a static-or-procedural playback content path.
- Procedural sources render mono at the world sample rate and feed the existing non-spatial or spatial processing paths.
- `PetalSonicEngine::set_fill_callback()` still exists for global fill use cases, but tree rustle no longer depends on it.

Prototype/reference state:

- `tools/prototype_tree_rustle.py` and `tools/prototype_tree_rustle_live.py` contain the tuned procedural model.
- `docs/procedural_tree_rustle_progress.md` records reference metrics and prototype tuning decisions.
- The Python and live Web Audio tuner are now source/reference material only; they should not remain the maintained implementation.

Constraints and assumptions:

- No tree-rustle WAV asset should be used for the final in-game sound path.
- Runtime source output should be mono before PetalSonic spatialization.
- Expensive coefficient updates should happen per block/control tick, not every sample.
- The audio callback should stay real-time safe; procedural generation should run on PetalSonic's render/pump path, not directly in the callback.
- Wind control should probably use shared lightweight source controls, such as atomics, rather than sending PetalSonic commands every frame.
- Current first-pass source LOD caps tree audio clusters to the largest `8` clusters per tree.

## Plan / Phases

### Phase 1 - Finalize runtime architecture

- Objective: Decide exact PetalSonic procedural source API and re-flora ownership boundaries.
- Expected output: agreed API sketch for `register_procedural` / procedural playback content / tree-rustle control handle.
- Dependencies/blockers: Need inspect PetalSonic internals enough to avoid fighting its mixer/spatial processor design.
- Status: done.

### Phase 2 - Add source-level procedural audio to PetalSonic

- Objective: Let PetalSonic play per-source procedural generators through the same spatial/non-spatial pipeline as static clips.
- Expected output: local PetalSonic changes with a trait/factory for procedural sources, playback content enum, registration API, config updates, stop/remove lifecycle, and tests/demo coverage.
- Dependencies/blockers: Need settle trait shape, reset/seek semantics, and how procedural sources report finite vs infinite duration.
- Status: done.

### Phase 3 - Port tree rustle DSP to Rust

- Objective: Move the tuned Python model into Rust as the authoritative generator.
- Expected output: Rust `TreeRustleVoice`/params/control implementation that renders mono blocks, with deterministic seeds and no per-sample allocations.
- Dependencies/blockers: PetalSonic procedural source API, plus decision whether DSP lives in `src/audio/` or a small crate such as `crates/re-flora-tree-rustle`.
- Status: done.

### Phase 4 - Integrate procedural rustle into re-flora tree audio

- Objective: Replace hardcoded WAV tree sources with procedural spatial sources driven by sampled wind.
- Expected output: `TreeAudioManager`/`TreeAudioSource` register procedural rustle sources, update per-source wind controls, and remove dependency on the static rustle WAV path.
- Dependencies/blockers: Phases 2 and 3.
- Status: done.

### Phase 5 - Add source LOD/culling and performance instrumentation

- Objective: Keep runtime cost bounded as tree count and source count grow.
- Expected output: active-source cap or distance/importance LOD, lower source count for far trees, silence/cull below threshold, and logs/timing for procedural generation cost.
- Dependencies/blockers: No blocker for first pass; deeper distance/importance LOD and isolated procedural timing remain follow-ups.
- Status: in progress; first-pass source cap and DSP microbenchmark are in place, deeper distance/importance LOD remains follow-up.

### Phase 6 - Validate quality and retire prototype maintenance

- Objective: Confirm the in-game procedural source sounds natural and responds to wind, then document Python as non-authoritative.
- Expected output: updated docs, validation notes, and possibly removal or de-emphasis of live tuner sync expectations.
- Dependencies/blockers: Need user listening feedback in an audible app run; hidden run only proves startup/render/audio-engine health.
- Status: in progress.

## Verification Method

PetalSonic validation after API changes:

```bash
cd /Users/bytedance/code/petalsonic
cargo test -p petalsonic
```

re-flora validation after using the local PetalSonic crate:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Runtime/audio checks:

- DSP microbenchmark: `cargo test --release render_perf_eight_voices -- --ignored --nocapture`.
  - Latest run on Apple M4 Pro: `8` voices, `4096` blocks, `1024` frames/block, `87.38s` of generated audio in `1394.210ms`.
  - Cost: `340.383us` per 1024-frame block for all 8 voices, `42.548us` per voice per block, `1.596%` of one realtime core for the capped 8-source tree case.
- Confirm no tree-rustle source path points at `assets/sfx/tree_sound_48k_pregain_40db.wav`.
- Confirm tree rustle still spatializes and responds to listener movement/occlusion.
- Change wind strength/GUI controls and verify timbre changes, not only volume.
- Add or inspect PetalSonic timing that reports procedural generation time per render iteration.
- Compare active procedural source count against timing and underrun logs.
- Run release-mode active-source benchmarks for representative counts, such as 1, 8, 36, 128, and the largest expected forest case.

Sound-quality checks:

- Use the existing reference targets from `docs/procedural_tree_rustle_progress.md` as qualitative and metric guidance.
- Verify 3-6 kHz contact rises with wind/leaf activity and 8-12 kHz air remains restrained.
- Verify crackle control remains meaningful without plastic-bag crinkle or white-noise harshness.
- Perform final listening in the real app, because spatialization, distance attenuation, and occlusion can change perceived balance.

Verification gaps:

- Final listening in a normal audible run is still needed.
- PetalSonic timing currently does not isolate procedural generation cost from other direct/spatial mixing work.

## Progress Log

- Tuned a dependency-free Python tree-rustle prototype against `docs/audio/wind_ref.wav` and documented the spectral/envelope targets.
- Synchronized the Python CLI renderer and live Web Audio tuner during prototyping.
- Decided the final game path should not use a generated or replacement WAV asset.
- Identified that current in-game tree audio uses a hardcoded static WAV loop and wind-driven volume only.
- Confirmed PetalSonic is an owned local crate at `/Users/bytedance/code/petalsonic`, so adding a proper procedural source API there is acceptable.
- Chose source-level procedural audio as the likely PetalSonic design, rather than a global fill callback or re-flora-only audio hack.
- Chose shared lightweight per-source controls as the likely way to update wind without per-frame playback command spam.
- Identified source-count risk: current fine leaf clustering can create dozens of audio sources for one tree, so runtime synth needs LOD/culling.
- Created this progress document to track the runtime migration plan before implementation.
- Added source-level procedural playback support to local PetalSonic and committed it there as `fee59b5 add procedural playback sources`.
- Switched re-flora to the local PetalSonic path dependency while this work is in flight.
- Added `src/audio/tree_rustle.rs` with a deterministic mono tree rustle generator, shared atomic wind/crackle controls, block-rate coefficient updates, bounded grains/creaks, and unit checks for wind-driven energy/brightness.
- Replaced tree rustle WAV spawning with procedural spatial source registration and per-source `TreeRustleControl` updates from sampled wind.
- Changed tree audio volume handling so wind drives synthesis timbre/activity, while PetalSonic source volume acts as master/cluster gain with a silence gate.
- Added a first-pass cap of `8` procedural audio clusters per tree to avoid one synth per fine leaf cluster.
- Added an ignored release microbenchmark for the Rust DSP path: `cargo test --release render_perf_eight_voices -- --ignored --nocapture`.
- Measured capped Rust DSP cost at about `340us` per 1024-frame block for all `8` voices (`43us` per voice/block, `1.60%` realtime CPU for 8 voices) on the Apple M4 Pro test machine.
- Validated re-flora with `cargo fmt --check`, `cargo check`, `cargo test`, and `cargo run --release -- --hidden --auto-exit 0.5`; latest hidden run exited successfully with no new audio errors or underruns in the inspected log.

## Open Questions / Risks

- PetalSonic procedural API is implemented locally; later upstream/versioning should decide whether the current trait names and infinite-stream semantics are final.
- Tree rustle DSP currently lives in `src/audio/tree_rustle.rs`; a separate crate remains optional if reuse/bench tooling grows.
- Current cap is `8` procedural sources per tree; distance/importance LOD for larger forests is still open.
- Current design drives synthesis with wind and uses source volume as master/cluster gain plus a silence gate; audible tuning may require adjustment.
- Need avoid allocations, blocking locks, command spam, and expensive math in the audio render path.
- Need ensure procedural randomness remains stable and does not pop when wind/source parameters change.
- Mono generation plus PetalSonic spatialization may sound different from the stereo prototype; tuning may need adjustment.
- Local PetalSonic path dependency should be managed carefully before release/versioning.
