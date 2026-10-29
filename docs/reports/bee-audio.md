# Bee flight loop integration

Implemented 2026-10-29. Uses the approved first-two-seconds crop with the 50 ms
circular crossfade: `assets/sfx/bee/bee-loop.wav` (1.95 s, stereo PCM16 / 48 kHz).
Audio bytes and generation provenance are unchanged; only runtime integration was
added. Rights remain unresolved as recorded in the asset bundle.

## Behavioral contract and ownership

- `particles::bees::Bees` owns resident identity and flight state. Its read-only
  `flying_sources()` publishes `(hive ID, resident slot)` identity and position
  only for outbound/return flight. Home and landed flower visits produce no buzz.
  Removing/restoring hives clears the corresponding facts; audio cannot create
  gameplay residents or keep a deleted one alive.
- `audio::BeeAudio` owns at most 16 emitter/voice reservations, including queued
  and retiring render ownership. It selects nearby finite positions (2 world-unit
  radius) by distance, with retained-source hysteresis and ID tie breaking.
- `SpatialFrame` reconciles these facts before its existing single spatial
  publication, and starts new loops only after successful publication. Failed
  publication gates and retires voices. Movement updates spatial poses, never
  restarts playback. Each resident has a stable, distinct initial loop phase.
- Pause, saved bee/master mute or zero gain, landing, range exit and world clear
  remove active loops. M / `--mute` still silence master output without bypassing
  processing. The existing 200 ms stale-control envelope bounds sound after a
  stalled control loop; render retirement remains reserved until reclaimed.
- `AudioCategory::Bees` has a saved neutral-default channel under **Audio → Live
  mixer**, using the existing saved-field controls and common serialization. Old
  saves default to enabled / 1× without altering other channels. Engine bus capacity
  is derived from `AudioCategory::ALL.len()`, not the previous default limit of 8.
- `runtime_assets` is the only playback/preload path owner. The manifest now has
  87 clips; neither the bee original nor the unlooped 2 s reference is preloaded.
  Existing generic transient emitter movement/seek helpers were renamed from
  sprinkler-specific names; the sprinkler's behavior is unchanged.

No new source normalization, pitch shift or speed change was introduced. Source
trim is 0 dB, followed by the existing sample envelope's 0.72 continuous gain,
spatial rendering and mixer gains. Final perceptual balance needs user listening;
muted validation is not evidence of loudness approval.

## Validation

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed. No generated
  Rust/shader changes, no shader edits, and no new warnings.
- Asset inventory tables regenerated from the updated catalog; all 7 inventory
  script tests pass. `asset_inventory.py --check` has no stale-table report but
  still exits 1 for **32 unrelated unreviewed existing assets**. No unrelated
  classification/license claims were added to make that check green.
- `cargo test bee`: 14 passed, one native lifecycle test deliberately ignored in
  the normal suite. Includes flight/landing/night/home/removal facts, stable IDs,
  phase variation, deterministic nearest selection/cap/hysteresis, invalid poses,
  and independent old-save/mixer serialization.
- Explicit native bee lifecycle test passed with a pinned nonexistent output
  device: actual 93,600-frame cached clip; prepare creates no playing voices;
  successful publication starts 16; movement reuses emitters without starts;
  disable/resume/removal reclaim ownership; failed publication and closed engine
  do not leave live reservations. This device/engine check remains `#[ignore]`.
- Explicit native sprinkler lifecycle regression passed after shared helper
  renaming and bus capacity change.
- Full `cargo test`: **1,741 passed, 9 failed, 14 ignored**, plus 4 passing collision
  tests. Failures are the same two config-default and seven raster-tree assertions
  observed before this change. Common custom-setting save/reload and runtime
  manifest/cache tests passed.
- `cargo run --release -- --hidden --mute --auto-exit 0.5`: passed; inspected the
  worktree log using `--tail-latest-log`. First frame ready and successful exit.
- Real Release garden fixture, with isolated copied config and asset symlink:
  `RE_FLORA_BEE_REVIEW=voxel RUST_LOG=info cargo run --release -- --hidden --mute
  --auto-exit 23` (also set `RE_FLORA_ROOT` to the isolated directory).
  Log confirms **87 clips**, bee loop **93,600 frames / 48 kHz / 1.95 s**, **23 flower
  visits / 18 returns**, **5 active bee voices / 46 cumulative starts** at the
  20-second checkpoint, and shutdown `failures=0`. No ERROR/VUID/synchronization
  hazard/panic or bee audio warning. User config was not rewritten.

Local evidence: `target/bee-audio-{check,tests,all-tests,native-test,release,smoke,
smoke-inspected,sprinkler-regression}.log`, plus
`target/bee-audio-review/run.log` and `target/bee-audio-review/inspected.log`.
No visible game launch, listening approval, performance acceptance or publication
was performed.
