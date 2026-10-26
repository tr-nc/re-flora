# Recorded tree-leaf audio — implementation and validation

## 2026-10-27 recorded-only follow-up

The player requested removal of **Tree leaves: recorded breeze / strong wind** and
the old ambient implementation. Recorded breeze/strong-wind blending is now the
only canopy-wind source, not a default that can be switched back.

- Removed the checkbox and eight `tree_rustle_*` synthesis controls, their generated
  adjustables, the procedural resident-loop renderer, mode-switch/replacement
  plumbing and obsolete phase/gain state.
- Each adapter requires a loaded recording bank; each canopy Voice requires a blend
  control. Missing recordings report a load error, never a procedural fallback.
  Independent take/phase choices, one Voice per generation, local-wind blend,
  generation crossfades, mixer/mute controls and **+12 dB** makeup are unchanged.
- Old settings with either checkbox value load without rewriting the file. The
  loader discards retired fields; explicit Save removes them. Saved leaf-channel
  enable/scale, wind volume/response and kick settings remain independent.
- The finite **kick** leaf-contact cue is not ambient wind. Its shared synthesis
  is now private to `src/audio/tree_kick_rustle/synthesis.rs`; the ambient manager
  cannot select that implementation. Kicking, cicadas and other sound categories
  retain their behavior.
- No WAV, derivative recipe or license changed. Save the garden and restart the
  existing game to use the rebuilt Release binary; no visible session was stopped.

### Follow-up verification

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed.
- `cargo test audio:: -- --nocapture`: **75 passed, 5 opt-in tests ignored**.
- `cargo test recorded_tree_wind`: two settings/schema tests passed, including
  both legacy true/false values, read-without-write and explicit save/reload.
- `cargo test every_declared_generic_setting_saves_its_live_value`: passed.
- Explicit opt-in tests `recorded_leaf_mix_keeps` and
  `kick_rustle_has_exact_tree_placement`: both passed with
  `-- --ignored --nocapture`. Eight canopy generations retain their source IDs
  across five volume changes and calm/weak/strong wind; recorded gain, geometry
  crossfade/retirement and the independent bounded kick cue remain valid.
- `python3 scripts/prepare_tree_leaf_loops.py --check`: all eight derivatives match.
- Asset bundle notes/catalog now describe recorded-only use; inventory tables were
  regenerated, not hand-edited. The inventory checker already reported **30 existing
  unclassified files** before these note changes; it still exits 1 for those same
  entries (now without stale-table errors). No unrelated asset/rights audit is
  claimed. Inventory script unit tests and the unchanged offline report verifier pass.
- Native Release verification:

  ```sh
  env -u WAYLAND_DISPLAY RE_FLORA_TREE_LEAF_AUDIO_REVIEW=1 RUST_LOG=info \
    cargo run --release -- --canopy-audio-budget-diagnostic \
    --hidden --mute --auto-exit 18
  env -u WAYLAND_DISPLAY RUST_LOG=info \
    cargo run --release -- --hidden --mute --auto-exit 0.5
  cargo run --release -- --tail-latest-log 200
  ```

  Five canopies remained recorded-only through six stages: calm, continued calm,
  weak, strong, release-to-calm, strong again. Observed blends:
  **0 / 0 / 0.266667 / 1 / 0.002635 / 1** (the release stage retains normal
  smoothing). Review and ordinary startup log `recorded_only=true`; both shut
  down with `failures=0`, without ERROR/panic/VUID.
  Logs: `target/re-flora-logs/re-flora-20261027-000247.477-506363.log` and
  `target/re-flora-logs/re-flora-20261027-000336.462-506469.log`.

These are correctness checks, not listening approval or performance measurements.
The repository-wide suite was not rerun; its documented baseline failures are not
claimed fixed. The earlier offline report remains a historical delivery snapshot.

## Historical: 2026-10-26 default activation

The recorded breeze / strong-wind pair is now enabled in the shipped local
configuration. Normal startup was verified with `recorded_mix=true`; the legacy
procedural comparison remains available explicitly in Debug, not the default.
Existing wind response, per-canopy take/phase selection, mixer controls and one
Voice per canopy are retained. No new sound files or licensing claims were added.

Validation: asset preparation `--check` verifies all eight derivatives;
`recorded_tree_leaves` unit tests and the explicit `recorded_leaf_mix_keeps`
native lifecycle test pass. A Release five-canopy hidden/muted run passed all six
calm/weak/strong and mode-switch stages, with blends 0 / 0.266667 / 1 and no
ERROR/panic/VUID. Final ordinary Release smoke confirms recorded mode by default.
Release was rebuilt. Listening approval and the commercial/redistribution block
below remain unresolved; muted validation does not establish subjective sound.

The following sections describe the original comparison implementation/history.

Date: 2026-10-24. Research and first-party references:
[recorded-tree-leaf-audio.md](../research/recorded-tree-leaf-audio.md).

## Historical A/B try-out (retired)

In **Debug → Wind → Response → Sound**, enable
**Tree leaves: recorded breeze / strong wind (A/B)**. Searching for `recorded breeze`
also finds the checkbox. Unchecked is the previous procedural rustle; checked is the
recorded pair. Default is unchecked. **Save persists the choice**, including through the
standard configuration load/merge path; there is no App-only setting or special save hook.

Both modes use **Audio → Live mixer → Tree leaves enabled / scale (x)** and the existing
**Tree Wind Volume (dB)** input trim. Master and `M`/`--mute` still apply. Existing rustle
synthesis controls tune the unchecked mode only; the UI explains this. Kick rustles,
cicadas, sprinklers and apple impacts are unchanged by this switch.

## Ownership and signal behavior

- One existing weighted canopy generation remains one Emitter/Voice. The new feature does
  **not** introduce one emitter per leaf or duplicate the canopy's acoustic queries.
- Each generation independently draws a gentle take (four choices), a strong take (four
  choices), and two random phases. Draws remain stable while that source exists and across
  A/B switches within the session. A new session/generation may choose differently.
- The current weighted local wind response, saved response curve and attack/release drive
  overall activity and the blend. There is no separate sound-side gust simulation.
- Equal-power weights are `sqrt(1-r)` and `sqrt(r)`, with a 20 ms sample-domain full-range
  control ramp. Both layers keep advancing at original speed, even at zero layer gain.
  Changes in wind neither seek nor restart playback.
- PetalSonic blends resident PCM **before** the normal spatial/environmental path. The
  equal-frame/rate precondition is validated at the clip boundary. Resampling preserves
  both layers and their relative phase; source seeks/rate changes and silent advancement
  continue to use the same cursor. Render-time mixing needs no allocations or locks.
- A/B replacement preserves the logical source UUID, extent, bus and lifecycle. Each Voice
  reconciles its own content state, allowing failed replacements to retry rather than
  restarting sources already converted. Newly created and replaced canopy loops have a
  5 ms onset ramp; old Voices retain the existing bounded stop ramp.
- Normal geometry-generation crossfades and removal remain owned by the existing canopy
  lifecycle. No periodic variant rerolls or audio-event backlog are added.

## Assets and calibration

All eight stereo originals were restored byte-for-byte from `1067833b`; their historical
Git object IDs match. Their README provenance and Free-plan restrictions remain intact.

Only eight new `runtime/` derivatives are preloaded, bringing the runtime manifest to
**86** files. `scripts/prepare_tree_leaf_loops.py` produces mono PCM16 / 48 kHz, removes DC,
overlaps 100 ms of tail/head with equal-power weights, and matches RMS to −30 dBFS.
Each loop is **379,200 frames / 7.9 s** at original speed. Source and derivative SHA-256s
are recorded alongside each group in `runtime/manifest.json`.

The recorded source uses +12 dB reference makeup (−18 dBFS RMS at full wind before the
existing tree/mixer/spatial gains), instead of the procedural path's +36 dB. This avoids
applying the old procedural calibration blindly to new PCM. It is not a claim of equal
perceived loudness against procedural audio. Users can adjust the existing saved controls;
extreme combined gain can still drive the existing mixer limiter.

## Automated validation actually executed

- `cargo fmt --check` and `cargo fmt --manifest-path crates/petalsonic/Cargo.toml --check`.
- `cargo check` and final `cargo build --release` succeeded in the main worktree.
- Full `cargo test`: **1688 application tests passed, 13 ignored**, plus 4 collision tests
  and 1 display-authorization test passed; existing opt-in integration tests remain ignored.
- Full `cargo test -p petalsonic`: **199 unit + 6 integration + 2 doc tests passed**;
  12 opt-in tests ignored. New checks use real playback admission/rendering for blend
  endpoints, control ramping, wrap/seek/rate/silent advancement, 48→44.1 kHz resampling,
  incompatible inputs, onset ramp behavior and zero realtime allocation/deallocation.
- Final GUI subset: **117 passed**. This includes the generic settings persistence
  round-trip, which exercises the newly declared A/B field without a per-setting save hook.
- Selection tests cover 256 sources, all 16 take pairings, independent phase/control state,
  stable selection and time-advanced re-entry. Repeats across sources are allowed.
- Explicit native lifecycle test:
  `cargo test recorded_leaf_mix_keeps -- --ignored --nocapture` passed. Eight generations,
  five A/B changes, calm/weak/strong wind, stable source UUIDs, one additional generation
  during a geometry crossfade, then complete Emitter/Voice retirement. The intentionally
  unavailable output device produces expected ALSA probing notices, not audible playback.
- Three deterministic preparation tests passed; `prepare_tree_leaf_loops.py --check`
  verified all eight derivative bytes/manifests. Targeted Ruff and Pyright checks passed
  through `uvx` (no global Pyright was installed).
- The asset inventory generator/check and its unit tests were run after staging restored
  assets so the tracked-file ledger includes originals, derivatives and provenance.

### Real Release scenes

1. Single-canopy scene:
   `RE_FLORA_TREE_LEAF_AUDIO_REVIEW=1 cargo run --release -- --canopy-audio-diagnostic --hidden --mute --auto-exit 12`
   passed six stages: procedural/calm → recorded/calm → recorded/weak → recorded/strong →
   procedural/strong → recorded/strong. Wind blend observed 0, approximately 0.266667 and 1.
   Log: `target/re-flora-logs/re-flora-20261024-020456.175-188226.log`.
2. Constrained-budget scene with **five genuine canopy sources**:
   `RE_FLORA_TREE_LEAF_AUDIO_REVIEW=1 cargo run --release -- --canopy-audio-budget-diagnostic --hidden --mute --auto-exit 14`
   passed all six stages. Layered source count followed **0 → 5 → 5 → 5 → 0 → 5**.
   Per-tree logs show independent take/phase choices and identical choices on re-entry.
   Fixture stages advance only after their observation, so rendering hitches cannot skip them.
   Log: `target/re-flora-logs/re-flora-20261024-021314.647-189914.log`.
3. Final ordinary smoke:
   `cargo run --release -- --hidden --mute --auto-exit 0.5`, followed by `--tail-latest-log 120`.
   Log: `target/re-flora-logs/re-flora-20261024-021724.728-190762.log`.

Scene logs were inspected: successful exits, no unexpected ERROR/panic/VUID, review shutdown
`failures=0`. Review inputs are disposable and never saved. Detailed command outputs are
retained under `target/recorded-tree-leaf-validation/`. These are correctness checks, **not
Release performance benchmarks**.

The same request's sprinkler direction fix is committed as `f9d0f2f4`; its 3,840-case Slang
regression and native particle/contact evidence are in [sprinkler-audio.md](sprinkler-audio.md).

## Remaining boundaries and handoff

- No visible game was automatically launched. Hidden/muted runs and PCM tests **do not prove
  human listening approval, acceptable physical-device loudness, or subjective loop quality**.
- These files retain the documented ElevenLabs Free-plan commercial/redistribution block.
  Local implementation was authorized; no new license, commercial clearance or push is implied.
- The user's separate `config/gui.toml` mixer values (Sprinklers 30×, Apple impacts 1×) were
  preserved and excluded from the implementation commit. No configuration save was performed.
- Release binary is ready in `target/release/re-flora`. Changes and verification notes are
  committed locally; publication was not requested. Shutdown is requested only after the
  final verification and local commit, as explicitly instructed by the user.
