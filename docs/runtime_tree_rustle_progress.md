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

- `src/audio/tree_audio_manager.rs` hardcodes `TREE_LOOP_PATH = "assets/sfx/tree_sound_48k_pregain_40db.wav"`.
- `TreeAudioManager` spawns looping spatial sources through `SpatialSoundManager::add_looping_spatial_source()`.
- `src/audio/tree_audio_source.rs` currently samples wind and maps it mostly to source volume.
- `src/audio/spatial_sound_manager.rs` wraps PetalSonic static audio registration/playback.
- `src/app/core/vegetation.rs` creates tree audio sources from leaf clusters; recent logs show one debug tree with about `36` audio clusters.
- `src/app/core/mod.rs` creates `SpatialSoundManager::new(1024, ...)`, so the current world audio block is `1024` frames.

Known PetalSonic path:

- Local crate is at `/Users/bytedance/code/petalsonic`; use it first instead of the crates.io dependency.
- Relevant files include `petalsonic/src/world.rs`, `petalsonic/src/playback.rs`, `petalsonic/src/mixer.rs`, `petalsonic/src/spatial/processor.rs`, `petalsonic/src/engine.rs`, and `petalsonic/src/config/source_config.rs`.
- PetalSonic currently stores and plays `Arc<PetalSonicAudioData>` for each source.
- `PlaybackInstance` reads static samples; spatial processing separately fills a mono input buffer from the static clip.
- `PetalSonicEngine::set_fill_callback()` exists, but it is global and not appropriate for many spatial procedural sources.

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
- Need confirm desired source LOD: current per-leaf-cluster audio granularity is probably too fine for independent procedural synth voices in forests.

## Plan / Phases

### Phase 1 - Finalize runtime architecture

- Objective: Decide exact PetalSonic procedural source API and re-flora ownership boundaries.
- Expected output: agreed API sketch for `register_procedural` / procedural playback content / tree-rustle control handle.
- Dependencies/blockers: Need inspect PetalSonic internals enough to avoid fighting its mixer/spatial processor design.
- Status: in progress.

### Phase 2 - Add source-level procedural audio to PetalSonic

- Objective: Let PetalSonic play per-source procedural generators through the same spatial/non-spatial pipeline as static clips.
- Expected output: local PetalSonic changes with a trait/factory for procedural sources, playback content enum, registration API, config updates, stop/remove lifecycle, and tests/demo coverage.
- Dependencies/blockers: Need settle trait shape, reset/seek semantics, and how procedural sources report finite vs infinite duration.
- Status: not started.

### Phase 3 - Port tree rustle DSP to Rust

- Objective: Move the tuned Python model into Rust as the authoritative generator.
- Expected output: Rust `TreeRustleVoice`/params/control implementation that renders mono blocks, with deterministic seeds and no per-sample allocations.
- Dependencies/blockers: PetalSonic procedural source API, plus decision whether DSP lives in `src/audio/` or a small crate such as `crates/re-flora-tree-rustle`.
- Status: not started.

### Phase 4 - Integrate procedural rustle into re-flora tree audio

- Objective: Replace hardcoded WAV tree sources with procedural spatial sources driven by sampled wind.
- Expected output: `TreeAudioManager`/`TreeAudioSource` register procedural rustle sources, update per-source wind controls, and remove dependency on the static rustle WAV path.
- Dependencies/blockers: Phases 2 and 3.
- Status: not started.

### Phase 5 - Add source LOD/culling and performance instrumentation

- Objective: Keep runtime cost bounded as tree count and source count grow.
- Expected output: active-source cap or distance/importance LOD, lower source count for far trees, silence/cull below threshold, and logs/timing for procedural generation cost.
- Dependencies/blockers: Need initial integration to measure real source counts and cost.
- Status: not started.

### Phase 6 - Validate quality and retire prototype maintenance

- Objective: Confirm the in-game procedural source sounds natural and responds to wind, then document Python as non-authoritative.
- Expected output: updated docs, validation notes, and possibly removal or de-emphasis of live tuner sync expectations.
- Dependencies/blockers: Need playable implementation and user listening feedback.
- Status: not started.

## Verification Method

PetalSonic validation after API changes:

```bash
cd /Users/bytedance/code/petalsonic
cargo fmt --check
cargo test
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

Verification gaps until implementation exists:

- No runtime procedural source API exists yet, so end-to-end runtime validation is not currently possible.
- No Rust DSP benchmark exists yet.
- PetalSonic timing currently does not isolate procedural generation cost.

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

## Open Questions / Risks

- Exact PetalSonic trait/API shape: factory vs boxed source, reset/seek behavior, finite vs infinite procedural duration, and whether stereo procedural sources are needed later.
- Whether tree rustle DSP should live in `src/audio/` or a separate `crates/re-flora-tree-rustle` crate.
- How many tree rustle sources should be active per tree at each distance/importance level.
- Whether wind should drive both procedural timbre and volume, or mostly timbre while volume stays controlled by distance/source importance.
- Need avoid allocations, blocking locks, command spam, and expensive math in the audio render path.
- Need ensure procedural randomness remains stable and does not pop when wind/source parameters change.
- Mono generation plus PetalSonic spatialization may sound different from the stereo prototype; tuning may need adjustment.
- Local PetalSonic path dependency should be managed carefully before release/versioning.
