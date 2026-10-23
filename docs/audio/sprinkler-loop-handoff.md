# Handoff: drive the sprinkler from the selected audio rhythm

## Superseded asset selection — 2026-10-23

The user requested deletion of unused assets, including the selected sprinkler
bundle. It is no longer in the current tree. The timing below is historical,
not an instruction to integrate a missing or commercially uncleared recording.
Choose an approved replacement before resuming audio integration; do not change
the other machine's unpushed driver based on this archived handoff.
See [cleanup record](../assets/cleanup-unused.md); the original bundle/provenance
can be inspected with `git show 1067833b:assets/sfx/sprinkler/README.md`.

## Status and boundary

**Not implemented.** The user confirmed that the newer sprinkler driver and its frequency/duty-cycle Debug controls exist as **unpushed changes on another machine**. Do not recreate, replace, or remove those changes based on this older checkout. Perform runtime integration on the machine/worktree containing that implementation, after inspecting its current diff and instructions.

This handoff was prepared on branch `sfx`, after:

- `c5fecec2` — import the selected loop and original source; remove rejected audio/prototype assets.
- `f7f83ccf` — document the source/settings/edit recipe and require self-contained provenance notes for new image/audio resources in `AGENTS.md`.

No Rust, shader, runtime configuration, or generated files were changed for this handoff. No in-game sprinkler audio has been connected here. Changes and assets remain local; no push was requested.

## User request

1. Detect the cadence of the selected sound and make the sprinkler's motion/emission frequency match it.
2. Remove adjustable **sprinkler frequency** and **duty cycle / 占空比** controls from the Debug panel. The user wants a fixed sound-matched behavior, not another tunable oscillator.
3. Use this WAV as the sprinkler's in-game sound, preserving its generated water/impact texture and rhythm.
4. Preserve the forceful, distinct action of the newer sprinkler driver. The earlier slow cosine-volume HTML experiment felt too drawn-out; do not reintroduce that audio treatment.

## Asset bundle to carry over

Keep this entire directory together:

```text
assets/sfx/sprinkler/
  README.md                     # authoritative provenance, generation settings, edit recipe, hashes
  sprinkler-original.wav        # 8 s original; editing/reference only
  sprinkler-first-6-loop.wav     # 1.56 s selected in-game loop
```

See the self-contained asset note (deleted from the current tree; see the historical Git path above). It records the exact user-supplied ElevenLabs prompt, **SFX v3**, **loop on**, **8 s**, **Prompt Influence 30%**, and **prompt improve on**. The effective improved prompt was not supplied. The new provenance rule applies to all future image/audio imports, not just this sound.

Use `sprinkler-first-6-loop.wav`, **not** the original, previous 5.22-second crop, de-clicked versions, or HTML-prototype water bed. Those alternatives were rejected/removed. Later parts of the original had clicks the user disliked; the chosen loop uses only its first six bursts.

## Measured timing contract

| Quantity | Value | Meaning |
| --- | --- | --- |
| Sample format | Stereo PCM16, 48,000 Hz | Preserve normal playback speed/pitch |
| Loop length | 74,880 frames = 1.560 s | One WAV repetition |
| Bursts per loop | 6 | Confirmed by short-window energy peak analysis |
| Mean interval | 12,480 frames = 0.260 s | One burst / nozzle-pair switch |
| Burst cadence | 6 / 1.56 = **3.846153846 Hz** | Use this if the driver defines frequency as bursts/sec |
| Two-pair cycle | 0.520 s = **1.923076923 Hz** | Use this if the driver defines frequency as Z→X→Z cycles/sec |
| WAV repeat rate | **0.641025641 Hz** | Not the spray frequency |

Measured energy peaks (5 ms analysis resolution):

```text
0.075, 0.335, 0.595, 0.860, 1.115, 1.380 seconds
```

Peak-to-peak spacing is approximately 255–265 ms, including the 255 ms wrap from the sixth peak to the first peak of the next loop. This small natural variation is in the selected recording; a fixed 260 ms cadence matches its mean without accumulated loop-length error. There are three complete two-pair cycles per WAV repetition.

**Phase matters:** sample zero is not the first impact peak. Align the driver's visible forceful phase to the approximately 75 ms first peak (or refine onset/peak alignment by listening to the real animation). Do not invent a sample-accurate event timing from this coarse energy measurement.

**Duty cycle is not established.** The loop's 15 ms crossfade is an audio edit, not spray duty. Inspect the newer driver's existing strong attack/recovery and the clip's audible burst duration; choose a fixed, documented shape. The request does not specify a numerical duty value or authorize replacing the new driver with this checkout's older motion curve.

## Integration guidance for the receiving machine

### Inspect before editing

- Confirm the worktree/branch and save/commit the other machine's existing sprinkler work as appropriate without overwriting it.
- Locate the actual frequency and duty controls, their persistent fields/defaults, and all consumers (mesh/head animation, nozzle direction, particles, moisture application). They were **not found** in this checkout's Debug configuration.
- Determine whether the current driver calls one burst or a full opposing-pair round-trip a "cycle"; apply the correct rate above rather than blindly setting a field to 3.846.

### Give timing one owner

- Keep the measured loop metadata and resulting fixed cadence in one authoritative place, preferably deriving the 260 ms interval from the selected clip duration/frame count and six-burst count.
- Have the rendering, particle and directional-moisture paths consume the same sprinkler timing/phase facts. Preserve the newer driver's intended impact shape and mean watering behavior.
- Do not round 260 ms to an integer number of the configurable world ticks (for example, five 50 ms ticks would become 250 ms and audibly drift). Do not maintain separate CPU and shader frequency/duty constants.
- Use a coherent playback/animation time origin and account for each sprinkler's phase. Prefer an existing playback clock/cursor interface if available. Otherwise explicitly establish the shared start/seek phase and validate long-run drift and pause/resume behavior; do not claim sample-accurate synchronization merely because two constants match.
- Preserve the loop's natural rhythm; do not repeatedly restart it on every burst or apply the rejected slow volume envelope. Avoid corrective per-frame seeking that can create clicks.
- Six alternating bursts return to the starting nozzle pair, so loop wrap must not swap parity.

### Audio ownership

- Create one positional looping source per live sprinkler, with explicit ownership and retirement. Placement, movement, brush deletion, other removal paths, shutdown, and any world reset that actually removes sprinklers must update that ownership correctly.
- Route through the game's existing audio system, spatial attenuation, and master mute/volume. If an independent sprinkler mix channel is added, use the saved-field-bound mixer controls under Audio; do not introduce new frequency/duty sliders or App-only audio controls.
- The selected WAV peaks around **−22.99 dBFS**. Listen in context to choose runtime gain; do not silently normalize/overwrite either bundled file. Document any new derivative processing in the bundle note.
- Do not replace the original/reference WAV or accidentally loop it. Do not restore rejected preview artifacts.
- Respect the receiving driver's pause/time-scale behavior explicitly. Audio mute should not independently reset animation phase. Missing/failed audio must not leave orphaned sources or cause unbounded per-frame creation attempts.

### Remove the obsolete controls at their owner

- Remove the frequency/duty controls and stop reading their previous saved values, rather than merely hiding active overrides.
- Keep old saved configuration loadable where practical; ignore/migrate obsolete keys at the existing config boundary. Do not introduce ad-hoc save hooks.
- Regenerate tracked shader/config outputs with `cargo check` if their sources change. Do not hand-edit generated structs.

## Useful seams in this older checkout

These are navigation pointers, not instructions to overwrite the newer implementation:

- `src/app/core/placeables.rs`: `SprinklerRuntime`, placement/update/removal plans, `SprinklerEmitter`, direction selection, moisture source records. The older version continuously generates droplets and alternates nozzle pairs on a nominal one-second world-tick schedule.
- `shader/slang/sprinkler.vert.slang`: older cosine cap displacement driven by `flora_tick`; not the newer forceful driver.
- `src/tracer/sprinkler_resources.rs`: instance layout and upload. Keep GPU instance/shader layout changes synchronized if this seam is used.
- `src/app/core/particles.rs`: sprinkler emitter advancement.
- `src/app/core/moisture.rs`: directional sprinkler moisture application.
- `src/audio/spatial_sound_manager.rs`: cached clips, positional loops, source movement, source removal, and internal initial-phase support. `add_looping_spatial_source` currently exposes randomized-phase selection, while the internal clip-source adapter accepts explicit phase; inspect available APIs on the receiving machine before extending them.
- `src/audio/audio_clip_cache.rs`: recursively preloads WAVs under `assets/sfx`; no new path registry was needed to make the files discoverable here.
- `src/audio/mixer.rs`, `src/app/gui_config/audio_mix.rs`: saved audio-category routing/controls.
- `src/app/core/lifecycle.rs`: explicit audio shutdown.
- `docs/agents/gui-settings.md`: authoritative Debug settings/persistence rules.

## Acceptance and validation

### Behavior to verify

- Exactly six bursts and three full opposing-pair cycles per 1.56-second playback loop; nominal pair switching every 260 ms, with correct parity across wrap.
- Initial phase includes the recording's pre-impact lead-in; observe multiple loops and a longer run for drift. Test non-default world-tick settings and frame-rate variation so quantization does not change cadence.
- Multiple sprinklers have coherent individual motion/audio phases; moving one does not restart its clip. Removing one stops only its source, and repeated placement/removal does not leak voices.
- Master mute/unmute, intended pause/resume behavior, world reload/removal paths, and shutdown are correct.
- Frequency and duty sliders are gone; obsolete saved settings cannot change the fixed cadence and do not break loading.
- Strong audible/visible bursts remain intact, no new loop clicks, and the spatial mix is comfortable when multiple sprinklers are present. This requires human listening; muted logs cannot establish sound quality.

### Commands/checks on the implementation worktree

```sh
cargo fmt --check
cargo check
cargo test
cargo build --release
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Also run a real-app fixture that places, moves and removes sprinklers: a normal hidden startup may contain none and is not sufficient lifecycle evidence. Inspect concrete source-creation/retirement and timing logs. Launch visible Release mode for a listening pass only when the user requests it, without `--perf` unless asked.

## What was verified here

- Selected clip: 48 kHz stereo PCM16, 74,880 frames / 1.56 s, six energy peaks at the times above.
- The asset README's exact circular-seam recipe reproduces the selected PCM, and both SHA-256 hashes match.
- Both WAVs decode correctly; no new signal processing was applied during documentation work.
- Documentation-only work: `git diff --check` and `cargo build --release` passed (existing unused-code/import warnings). No generated changes.
- **Not verified:** the other machine's unpushed driver/controls, in-game sound, audio/motion phase coupling, lifecycle behavior, loudness balance, or runtime duty selection. These are the receiving implementation's work, not completed fixes.
