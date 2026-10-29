# Grass wind audio — implementation and validation

2026-10-29. Implements the user-approved [HTML proposal](grass-wind-audio.html).
Local changes only; no release or push. The existing audio files are unchanged.

## Saved controls

Search **grass rustle** in the Debug panel to show all seven controls together.

- **Audio → Live mixer → Grass rustle**: enabled/mute and logarithmic volume scale.
- **Wind → Response → Sound**:

| Control | Default | Range / meaning |
| --- | --- | --- |
| Grass rustle recording gain | +12 dB | −24…+36 dB; explicit playback makeup for the very quiet source, not file normalization |
| Full-response wind strength | 1.0 | 0.1…5; lower values make gentle wind reach full response sooner |
| Listening range | 1.5 world units | 0.25…2; these are not meters (engine propagation uses a 15× distance scale) |
| Fade-in | 0.10 s | 0.02…1; time to approximately 99% of target amplitude |
| Fade-out | 0.25 s | 0.05…2; time to approximately 99% decay |

Use the existing **Save** button. Tuning is declared in `config/gui.toml`; the
mixer is bound to `SavedCustomSettings.audio_mix.grass_rustle`. Older saves obtain
defaults through the existing migration/default paths. No App-only sliders or
per-setting persistence hooks were added. Runtime counters stay in logs.

## Ownership and behavior

### Coverage comes from committed vegetation

`src/builder/surface/grass_audio.rs` derives small immutable summaries, not a
second editable root list. It reads the committed tall/short grass streams,
matching hydration by root identity, and the existing packed growth-potential
field. Visible mass includes individual mature height, packed growth progress,
competition, hydration and the Debug growth override. Invisible seedlings have
zero mass. Flowers and canopy leaves never become grass sound sources.

`src/flora/grass_acoustics.rs` groups roots into **64-voxel / 0.25-world-unit 3D
cells**. Each cell has at most four occupied horizontal subcells and a deterministic
real-root representative per subcell, placed in the visible stem. Elevated patches
remain distinct. IDs are world cells and subcell IDs, not listener-relative points.

Mass is visible height / 8 per root, saturated at 256 mature-blade equivalents
per subcell. This is a perceptual density model, not a friction simulation. The
representative resolution is deliberately bounded; it is not per-blade occlusion.

Structural publications (append, GPU compaction, mowing, restore and terrain root
removal) invalidate old coverage immediately. Growth, hydration and competition
are soft revisions; their last committed summary can remain available until the
bounded refresh serves them. Growth-override changes invalidate visibility too.
Surface jobs track pending writes; projection does not read an in-flight stream.
GPU build/edit/growth paths now transition the instance buffer to **HostRead** and
finish their existing fence before publication. The allocator's coherent mapped
memory is checked; `read_back_range()` itself is not treated as synchronization.
There are no new GPU waits in the acoustic projection or audio callback.

### Bounded work and playback

- At most **one chunk / 80,000 roots** summarized per application frame. Soft
  revisions are coalesced to at most 10 Hz per chunk; structural edits bypass that
  throttle. Oldest eligible chunk first avoids starvation.
- Listener range is clamped to 2 world units. Candidate discovery visits at most
  6³ neighboring chunk coordinates, not an unbounded world scan, and retains at
  most **128 nearby patches**. No additional per-root CPU cache is retained.
- Selection runs up to 10 Hz, ranks estimated audible power, and biases retained
  voices by 30%. It selects at most **12 steady voices**. Prepared, fading and
  audio-thread-retiring ownership share a **16-reservation total ceiling**.
- Grass uses the actual `WindField` frame, not tree-only diagnostic wind overrides.
  Each representative samples its local wind. Power is
  `mass × clamp(wind_strength / full_wind, 0, 1)²`.
- Extent weights are normalized **relative power**; PCM amplitude is separately
  `sqrt(sum(power) / 256)`. Distance and occlusion belong to PetalSonic and are not
  applied again as gain. There is no normalization across selected voices that
  would boost survivors after mowing.
- Tiny inaudible powers share a silence threshold. Calm/empty/removed patches
  fade out; pause, channel/master mixer mute, failed spatial publication and world
  replacement immediately silence controls and retire ownership. **M / --mute**
  remains the existing global output mute, so hidden muted runs still exercise
  audio processing.
- Live voices never reseek when the listener or wind moves. New activations use a
  deterministic cell offset plus the domain clock, preserving phase progression
  through calm intervals. Clearing/pausing resets that clock. All instances still
  share the recording's **1.95-second period**; offsets do not remove repetition.
- Missing new-world facts do not reuse old-world loops. The existing world-load
  gate and `SpatialFrame::clear_world_sources()` own the reset. Retiring render
  owners keep their reservation until released; failed commands are not counted
  as successful playback.

Only `assets/sfx/grass-wind/grass-wind-loop.wav` is added to the runtime manifest
(**88 clips** total). The original and exact two-second crop remain reference
files; their [provenance and rights caveats](../../assets/sfx/grass-wind/README.md)
remain with the bundle.

### Engine support, not a frozen-extent workaround

Previously, spatial frames updated attached Voice poses, but each Voice retained
its admission-time extent. Updating only the emitter would leave a playing grass
loop's geometry/weights stale.

PetalSonic now offers **`SourceExtent::live_weighted_samples`**, an explicit opt-in
with at most four samples stored inline. Both render and acoustic snapshot owners
observe the same complete spatial frame. Updating an active live extent copies
bounded inline data, without allocating or freeing on the render callback. A
queued Play also adopts the current frame when admitted. Existing captured
extents and detached voices retain their old behavior. Live emitters cannot switch
back to a captured domain in an update; create a new emitter for a policy change.

`SampleEnvelopeControl::anchor_gain` adds sample-domain continuous gain with
independent attack/release and the existing 200 ms fail-silent freshness lease.
Stale or explicitly stopped controls de-click to zero within 1 ms, including large
gains. Legacy sprinkler valve and bee-envelope behavior is unchanged. The app's
smoothed amplitude estimate is only for scheduling priority/retirement; it is not
multiplied into PCM a second time.

Grass participates in **prepare → complete spatial publication → start**, without
canopy IDs, telemetry claims or a second scene. It uses the normal looping acoustic
path; the engine's finite-one-shot-only acoustic-start option is not used.

## Validation

### Automated and native checks

- `cargo check` and `cargo build --release` pass; Release binary prepared in this
  worktree. Generated change: `src/app/generated/gui_adjustables_gen.rs` only.
- Fast grass tests cover bare/calm silence, stable occupied representatives,
  vertical separation, visible growth/competition/hydration/override, local wind
  weighting, additive power, candidate/voice limits and pending publication.
- CPU/Slang contract: the actual renderer's extracted `sampleGrassHeight` and
  `grassGrownHeight` match the CPU checksum over 1,920 root/species/growth/limit
  combinations. Shader extraction is behavior-preserving, not a rendering redesign.
- Opt-in native engine lifecycle test plays the real 93,600-frame clip and checks
  publication-before-play, stable voice identity under movement/reordering,
  thinning gain, transition reservations, mute/pause, calm, clear and publication
  failure. A closed engine rejects new reservations. This is not a device-recovery
  or perceptual listening test.
- PetalSonic tests check live render and solver updates with **zero real-time memory
  activity**, captured/detached compatibility, extent limits and sample-domain
  envelopes. Full serial run: **202 unit + 6 integration + 2 doc tests passed,
  12 ignored**. An earlier parallel run timed out in the existing asynchronous
  `failed_runtime_spatial_publication_preserves_the_pose_and_extent_for_the_next_voice`
  test; the full serial rerun passed.
- Full app suite: **1,748 passed, 9 failed, 15 ignored**, plus four collision tests
  passed. The failures remain the known two saved-default assertions
  and seven raster-tree assertions. No unrelated defaults/geometry were changed to
  hide them. The manifest-count expectation was updated from 87 to 88.
- `cargo fmt --check` remains blocked by existing formatting differences across the
  repository. New modules and the engine changes are rustfmt-formatted; unrelated
  formatting churn was reverted. `git diff --check` passes.
- Asset inventory tables were regenerated and are current. Its check still exits
  1 for **32 unrelated unreviewed assets**, not stale tables or grass provenance.
- Existing generic/custom save–reload tests pass. A hidden Release screenshot with
  Debug search `grass rustle` was inspected: all five tuning sliders and both mixer
  controls are visible, with correct paths and defaults (`target/grass-wind-controls.png`).

### Real hidden Release runs

`RE_FLORA_GRASS_AUDIO_REVIEW=1` runs an opt-in owner-boundary/GPU acceptance fixture,
then restores terrain and flora, including on a returned assertion error. It tests
bare ground, tall/short grass, two elevations, zero override, actual GPU mowing and
growth, reversible removal/undo, snapshot load, hydration and terrain support
removal. Its bounded projection benchmark is outside ordinary unit tests.

On **Apple M4 Pro**, final sampled Release results:

- Max-filled chunk, **80,000 roots**, 12 samples: projection median **1.054 ms**,
  maximum **1.092 ms**; cached query mean **0.7 µs** over 100 calls.
- Real planted fixture, **1,012 roots / 4 patches**, two local gusts: reached four
  active voices and four reservations; starts stayed at four during the sampled
  steady interval. Sampled cached query costs were **0–2 µs**; one logged summary
  refresh was **196 µs**. The **entire spatial transaction**, not grass alone, was
  **3–12 µs** at logged frames.
- Logs show no ERROR/panic/VUID from these runs. Initial calm transport correctly
  allocated no voices before wind reached the patch. Default bare garden produced
  no grass voices.

Evidence: `target/grass-runtime-final.log`, per-worktree run log
`target/re-flora-logs/re-flora-20261029-154021.148-96934.log`, and the test logs under
`target/grass-*`. These are scoped measurements, **not** an aggregate frame-time,
audio-worker/callback overhead budget or cross-platform performance acceptance.

Reproduce (hidden/muted, no visible game launch):

```bash
cargo check
cargo test grass_
cargo test real_grass_loops_publish_retain_fade_budget_pause_clear_and_failure -- --ignored --nocapture
cargo test -p petalsonic -- --test-threads=1
cargo build --release
cargo run --release -- --hidden --mute --auto-exit 0.5
RUST_LOG='warn,re_flora::audio::grass_wind=debug,re_flora::builder::surface::grass_audio_review=info,re_flora::app::core=info' \
RE_FLORA_GRASS_AUDIO_REVIEW=1 \
RE_FLORA_GRASS_SOIL_COVER_REVIEW=visual RE_FLORA_WIND_PROTOTYPE_SMOKE=1 \
cargo run --release -- --hidden --mute --perf --camera-snapshot garden-arrival --auto-exit 5
cargo run --release -- --latest-log
cargo run --release -- --tail-latest-log 200
```

The existing soil-cover visual fixture plants through production interfaces; the
wind smoke releases two finite gusts. They are explicitly requested validation
fixtures, not defaults and not saved controls. Slang CPU contracts can be run via
`scripts/run_slang_tests.py` with the repository's pinned compiler/runtime.

## Remaining acceptance

Manual listening is still required for loudness versus leaves/bees/footsteps,
short-loop repetition, directional balance and transitions. No listening approval
is inferred from muted runs or waveform statistics. Dense mixed-audio worker and
callback overhead, live device recovery, and other GPU/driver platforms remain
unverified. Commercial/standalone distribution rights remain unresolved as stated
in the asset bundle. No visible game was launched automatically.
