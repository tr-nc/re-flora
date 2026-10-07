# Short tree-kick sound at `bad-audio-case`

## Status and finding

Diagnosis only; no audio repair or saved gain/timbre adjustment has been shipped.
The user's camera snapshot is preserved in `config/camera_snapshots.toml`.

The real kick/audio path at this viewpoint produces a **cold-start transmission
burst**, not an early-ending clip:

1. The entire finite canopy rustle is assigned to the nearest single canopy
   sample, with `SourceExtent::Point` and `OcclusionProfile::PointExact`.
2. Its listener path is wood-occluded. Solved amplitude transmission is
   `[0.08, 0.035, 0.015]` (low/mid/high).
3. PetalSonic 0.9.2 initializes a new native direct-filter state at unity gain.
   Until a response exists, it retains that gain; thereafter it smooths toward
   the wood transmission with a 50 ms time constant.
4. Thus an initially loud opening is followed by a heavily attenuated tail.
   The nearest-point policy lets one obstructed path govern the whole tree's
   rustle. Changing viewpoints can choose a different point/path as well as a
   different HRTF direction.

A native-DSP replay matches the live per-Voice direct/source energy ratio to
approximately **6e-10 relative error**. In that replay, **97.13%** of direct
energy occurs in the first **100 ms**, and **95%** has occurred by **80 ms**.
These are pre-HRTF signal metrics, **not measured perceptual duration**. Hidden
muted runs do not establish audible naturalness or downstream device behavior.

## Relevant code

- `src/audio/tree_audio_manager.rs::play_kick_rustle`: nearest canopy point.
- `src/audio/spatial_sound_manager.rs::add_spatial_resident_one_shot`: point
  extent, exact-point occlusion and once-only playback.
- `src/audio/tree_kick_rustle.rs::KickRustlePlayer`: finite clip/retirement.
- `src/builder/contree/mod.rs::acoustic_material_for_voxel`: wood transmission.
- Shipped `petalsonic-0.9.2/src/spatial/processor.rs`:
  `NativeDirectSourceState::new`, `apply_native_direct_effect` and
  `apply_three_band_gain`. No sibling-backend limiter changes are active here.

## Fixture and exact placement

Game source baseline: `2664a145`, worktree `new-tree`, with the user's newly
saved snapshot. Normalization named it `bad-audio-case`, not `badaudiocase`.
The snapshot records a viewpoint, not a frozen garden/audio state.

- Camera: `(0.9601637, 0.4955372, 0.9404505)`, yaw `140.38507`,
  pitch `11.159441`, FOV `60`, walking mode.
- Runtime listener: `(0.9601637, 0.49553716, 0.9404505)`.
- View ray: `(0.6255688, 0.19353987, 0.7557817)`.
- Checked wood hit: `(0.990054, 0.5047847, 0.9765625)`, tree `0`.
- Selected canopy point: `(1.0080428, 0.69259095, 0.8606632)`.
- Actual acoustic distance: **55.78722 voxels / 3.2687824 acoustic metres**.
- Listener-local direct pose: `(-0.087757885, 0.19931512, 0.007787656)`.
- Descriptor sample: `520865337714723329`, authored wood clearance
  `3.769987` voxels. Clearance at the sample does not imply a clear listener ray.

Saved tuning was unchanged: duration `2`, body `0.5`, brightness `1`, contact
`0.2`, event gain `-3 dB`, Master scale `17`, Leaves scale `10`, input master
`+1 dB`. `config/gui.toml` SHA-256:
`c4c20e33963c225c500d674ab58225a852fe9947d1808578317e767fe0a77c85`.
Muted app runs substitute the runtime master input gain; energy **ratios** remain
comparable. The isolated unclipped-output probe uses the saved unmuted gains.

## Controlled real-app evidence

Temporary opt-in instrumentation performed one explicit diagnostic blocking
pick at the saved viewpoint, checked its tree owner/reach, then called the
normal kick admission and canopy-arrival path. It did not synthesize a direct
fixture sound or test OS K input/ordinary async admission.

Baseline and repeat selected identical source/listener positions and produced
identical source and direct energies. A single-variable control changed only
`DirectGeometry` to `BypassTransmission`; point placement, clip variant,
HRTF pose, distance, finite lifetime and environment routing stayed fixed.
This bypass was a **diagnostic**, not a proposed/shipped repair.

| Per-Voice metric | Baseline | Direct-clear diagnostic |
|---|---:|---:|
| Source energy | 1.651539294752477e-6 | 1.651539294752477e-6 |
| Direct energy | 2.704536903936544e-8 | 1.543650806917156e-7 |
| Direct/source energy ratio | 0.016375855618633 | 0.093467398070508 |
| Environment-send energy | 2.704536903936544e-8 | 2.704536903936544e-8 |

Direct-clear energy is **5.70763x / +7.56456 dB** larger. Environment transmission
remains wood-occluded in the control. This separates transmission from direction,
PCM variant, source placement and environment mixing.

Baseline host log times:

- Play: `01:24:34.652`.
- First-render observation: `01:24:34.669`, `environment_response: None`.
- First logged wood extent response: `01:24:34.685`.
- Energy summary: `01:24:36.710` (**2.058 s** after play).
- Emitter retirement: `01:24:36.919` (**2.267 s** after play).

Host drain times are not exact sample-onset timestamps. The native replay below
identifies one initial clear 1024-frame block before the target changes. The
summary/retirement logs refute premature cleanup as the cause in this fixture.
All three instrumented hidden runs exit successfully with shutdown `failures=0`.

## Minimized native-filter replay

The ignored diagnostic workspace includes the game's real leaf synthesis,
current finite excitation/tuning, and the exact published native three-band
filter/state implementation. It uses 48 kHz, 1024-frame blocks, crossovers
400/4000 Hz, 50 ms smoothing, the captured distance and wood gains.

| Direct-filter case | Direct/source energy | Time to 95% energy | First 100 ms fraction | After 250 ms fraction |
|---|---:|---:|---:|---:|
| Fully clear | 0.093467397935 | 594.125 ms | 34.63% | 27.56% |
| One clear block, then wood; production unity initialization | 0.016375855609 | **80.000 ms** | **97.13%** | **0.27%** |
| Wood target immediately; production unity initialization | 0.008220132196 | 84.854 ms | 96.50% | 0.52% |
| Wood target and initial gain consistent from onset | 0.000144007425 | 593.063 ms | 35.37% | 28.28% |

The last case is another **diagnostic**, not an implemented startup policy. It
changes initialization alone and shows that steady wood filtering does not by
itself collapse the temporal envelope. Even an immediately available response
still creates a burst if the native state starts at unity and ramps down.

Red-capable signal-proxy command, executed with expected exit **101**:

```bash
CARGO_TARGET_DIR=target cargo run --release --offline \
  --manifest-path target/kick_audio_distance_probe/Cargo.toml \
  -- "$PWD" --badaudio-replay --assert-tail
```

```text
cold-start wood attenuation collapses the same finite cue:
t95=0.080000s versus consistently occluded 0.593063s
(signal proxy, not listening acceptance)
```

This remains a failing diagnostic, not a passing game regression test or an
acceptance threshold for arbitrary legitimately obstructed sounds.

## Clipping, direction and remaining uncertainty

The previous synthetic near-field probe demonstrated clipping capability, not
clipping at this viewpoint. At the **captured pose and distance**, all four
resident variants produce isolated pre-clamp peaks **0.603643–0.690395** and
**zero** hard-clipped samples using the shipped HRTF/headroom and saved unmuted
mix. Clipping is not reproduced as the explanation for this isolated cue.
Concurrent sounds, downstream output and device-rate resampling are not covered
by that isolated probe. The published backend still hard-clamps overloaded mix
output; that independent risk is not repaired here.

HRTF direction changes remain real, but cannot explain the matched-DSP timing
collapse or the one-variable transmission-control difference at a fixed pose.
Manual listening and reproduction of the user's exact OS/device mix are pending.

## Artifacts and cleanup

Ignored local artifacts are intentionally retained as clearly marked diagnosis
material, not runtime source:

- `target/kick_audio_distance_probe/re-flora-snapshot-diag`
  (SHA-256 `6000493f15084d27ddc3046ac010a943e6b7db7754791fa7b160c7865a588eaf`).
- `target/kick_audio_distance_probe/snapshot_instrumentation.patch`.
- `target/kick_audio_distance_probe/evidence/badaudio-{baseline,direct-clear,baseline-repeat,snapshot-hrtf,direct-replay,tail-red}.out`.

The archived instrumented binary accepts the explicit diagnostic environment
variables; the ordinary rebuilt Release binary does **not**:

```bash
env -u WAYLAND_DISPLAY RE_FLORA_KICK_AUDIO_DIAG=1 RUST_LOG=info \
  target/kick_audio_distance_probe/re-flora-snapshot-diag \
  --hidden --mute --camera-snapshot bad-audio-case --auto-exit 8
# Diagnostic-only direct transmission control: also set RE_FLORA_KICK_AUDIO_CLEAR=1.
```

All `[DEBUG-badaudio]` instrumentation is removed from tracked game source.
No gain, brightness, body or material transmission workaround is committed.
A repair should separately address correct cold-start transmission initialization
and coherent canopy-wide emission, preserving legitimate obstruction and ambient
behavior. It must then repeat this fixture and include manual listening; simply
lengthening the clip does not repair this temporal gain transition.

## Clean handoff validation

After restoring all temporary game-source edits:

- `cargo fmt --check`, `cargo check`, `git diff --check`: passed.
- Snapshot TOML parsed and exact fixture position/mode checked with `uv`.
- `cargo build --release`: passed (39.25 s; two existing diagnostic dead-code
  warnings). Ordinary binary SHA-256:
  `2ac618fdbfd80433c12b6415d0907e07adbd0b5ea3e3b54f26f302050c1daf35`.
- `RUST_LOG=info cargo run --release -- --hidden --mute --auto-exit 0.5`:
  passed; inspected `target/re-flora-logs/re-flora-20261008-015431.716-2360650.log`,
  no errors, shutdown `failures=0`.
- Saved GUI Settings hash remained unchanged; no generated files changed.

No full-suite, audible acceptance, sustained-performance or repaired-regression
claim is made. The red diagnostic is intentionally still red pending a repair.
