# Recorded tree-leaf audio A/B — implementation and validation

Date: 2026-10-24. Research and first-party references:
[recorded-tree-leaf-audio.md](../research/recorded-tree-leaf-audio.md).

## Try-out

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
