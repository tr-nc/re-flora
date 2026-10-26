# Sprinkler sound: tools, licensing and integration seam

Research date: **2026-10-21 UTC**. Garden delivery **scope v1**, work ID `audio`.
Repository inspected at `a66d9c5144b4a687a4e0c3c7cfa5f8ae7b6cc527` on
`agent/garden-audio`; initial working tree clean. This is research, **not implemented sound**.
No build, tests, app, GPU work, inference, model download, registration, license acceptance or
playback occurred. Only public text/source documents were fetched. No preference/save edits.

## Decision

Use **original procedural DSP** for the first usable spatial sprinkler sound. Author a quiet
water hiss and a short pulse/impact texture independently, then gate them with the existing
water valve phase. This supplies actual continuous sound at 100% duty and intermittent sound
at intermediate duty without model runtime cost or new sample/model licensing dependencies.
It does not mean the existing engine/dependencies have no licenses. No paid service is needed.

Three credible downloadable AI tool/model families were checked below. They can generate SFX,
but none is cleared here as an unrestricted commercial-game asset pipeline. **Open code,
available weights, permission to run weights commercially, and permission to distribute outputs
are four different questions.** Marketing use of “open source” is not proof of all four.
AI remains an optional offline authoring step, not a runtime game dependency. Research-only
models are not a reason to block the original procedural implementation.

## Comparison and separate license review

### 1. AudioCraft / AudioGen medium — free research tool, not commercial clearance

- **Availability/function:** Meta's public code, official model card and HF card are accessible.
  `facebook/audiogen-medium` is a 1.5B text-to-environmental-sound model using a 16 kHz EnCodec
  tokenizer; official usage shows five-second clips. This is the AudioCraft reimplementation,
  not the original paper checkpoint. [A1–A4]
- **Code:** MIT; retain copyright/license notice when redistributing code. [A1]
- **Weights:** **CC BY-NC 4.0**, explicitly separate from MIT. The official repository also
  provides `LICENSE_weights`; CC's legal text permits reproduction/sharing/adaptation for
  NonCommercial purposes, with attribution conditions. Not unrestricted open-source weights
  for commercial game use. [A2, A3, A5]
- **Outputs:** The inspected card does **not** provide an explicit blanket commercial-output
  grant. CC on weights does not automatically prove every output is a derivative work, nor
  does that ambiguity authorize commercial use of the weights. Generating game assets is not
  cleared by the MIT code license. Do not ship outputs from this pipeline without separate
  permission/legal review. Avoid implying the output itself is conclusively CC BY-NC. [A2, A3, A5]
- **Commercial redistribution:** Do not bundle these weights in the game. Offline generation
  for a commercial product is also not cleared merely because only WAVs ship.
- **Hardware/cost:** Official instructions require a GPU with **at least 16 GB memory** for
  the medium model. No local fit/speed/quality measurement was made. [A4]
- **Sprinkler suitability:** Environmental sound focus is relevant, but five-/ten-second
  generative clips would still need source isolation, seam editing and pulse synchronization;
  prompt timing is not a valve-clock contract. Reject for this delivery's production assets.

### 2. Stable Audio Open 1.0 / stable-audio-tools — relevant, conditional and access-blocked

- **Availability/function:** Stability's official announcement describes text-prompted samples
  up to **47 seconds**, specifically ambient sounds, foley and sound effects, rather than full
  songs. Public inference/training tooling is accessible. [S1, S2]
- **Code:** `stable-audio-tools` is **MIT**, not the license of every checkpoint it can load. [S3]
- **Weights:** Direct public requests to the **1.0 model card and LICENSE.md returned HTTP 401**.
  No signup/access acceptance was attempted. Therefore the exact currently attached 1.0 weight
  license and model-specific exceptions are **not fully verified** in this research. [S4]
- **License evidence, not a substitute for the inaccessible file:** Stability's public Community
  License page was accessible (page text dated July 5, 2024). The developer-published copy in
  TangoFlux is also accessible. That copy says commercial model/material use requires
  registration, terminates above aggregate annual revenue USD 1,000,000, and distinguishes
  model derivatives from outputs. It says “As between You and Stability AI, You own any outputs
  ... to the extent permitted by applicable law”; use restrictions expressly include outputs,
  including AUP compliance and restrictions on foundational-model training. Do **not** transplant
  this copy into a claim of verified 1.0 terms. [S5, T3 §§III–IV]
- **Outputs/commercial redistribution:** Ownership language is not an unconditional output-use
  waiver or an infringement warranty. Recheck exact model license, incorporated AUP, commercial
  registration requirements and any model-specific conditions before generating game assets.
  Weight redistribution and WAV redistribution must be reviewed separately. Current no-signup
  authorization prevents taking that approval step now. Not approved for shipping here.
- **Hardware/cost:** PyTorch/offline model inference, not a small Rust runtime sound generator.
  No authoritative minimum inference VRAM was verified from the accessible sources; do not
  invent one or infer it from training batch-size instructions. No weights were fetched. [S2]
- **Sprinkler suitability:** Good candidate for future *offline* isolated water texture authoring
  if cleared, but still requires editing, looping, gain matching and the shared valve clock.

### 3. TangoFlux — fast published SFX research model, explicitly not game-ready licensing

- **Availability/function:** Official repository and `declare-lab/TangoFlux` HF card accessible.
  Text-to-audio, up to **30 seconds, 44.1 kHz stereo**. Repository claims approximately three
  seconds generation on a single **A40**; its table reports 3.7 s at 50 steps. These are author
  claims, not measurements on this computer and not a minimum hardware specification. [T1, T2]
- **Code:** The repository's actual `LICENSE.md` is **not MIT/Apache**: its opening states
  “This repository contains ... the ‘Model’ ... non-commercial, research-only” and applies
  Stability Community, WavCaps and original dataset terms. No separate permissive code grant
  was verified. Treat code reuse as **not cleared**, rather than assuming an older license or
  package classifier overrides the checked-in text. [T1, T4]
- **Weights:** HF card explicitly says **“non-commercial research use only”**, subject to Stable
  Audio Open, WavCaps and original dataset licenses. [T2 §License]
- **Outputs:** No unambiguous blanket commercial-output grant was found. The underlying
  Stability text's output-ownership clause does not erase TangoFlux's added research-only and
  data conditions. Do not claim all model outputs inherit dataset licenses automatically;
  the narrower finding is that the author has not cleared this commercial authoring path.
- **Commercial redistribution:** Do not ship weights or generated game assets under this
  research-only grant. Registration with Stability alone would not establish that every
  additional TangoFlux/dataset restriction has been cleared. [T2–T4]
- **Sprinkler suitability:** Technically relevant for hiss/transients and short ambience,
  but licensing is worse than the DSP fallback. No inference scheduled.

### 4. Original procedural DSP — selected zero-new-license-dependency fallback

- **Code:** Write original Rust using the existing engine/sample containers. Do not copy DSP
  library code or an example sample. Existing original tree-rustle generation demonstrates
  synthesis into `ResidentClip` outside the real-time callback. [R4]
- **Weights:** None. **Output restrictions:** no model/provider grant or third-party sample
  license needed for entirely original synthesized PCM; normal project authorship and
  applicable law still apply. Do not label generated clips CC0 unless the owner chooses that.
- **Commercial redistribution:** Bundle the authored PCM with ordinary project assets, not
  model weights/toolchains. Existing PetalSonic/project license obligations remain separate.
- **Hardware/cost:** Short CPU-generated buffers at initialization/offline; no GPU, network,
  Python dependency or runtime inference. CPU/RAM figures below are *budget proposals*, not
  benchmark results. Fidelity is an audition question, not a claim established by this note.

## Practical authored sound plan (next phase, not executed)

1. **Hiss bed:** deterministic seeded noise, DC removal, gentle high-pass around 500–900 Hz
   and low-pass around 7–10 kHz at 48 kHz sample rate. Add a little lower filtered noise for
   water body, not a pitched motor. Slowly modulate texture depth with seeded low-frequency
   variation **only for timbre**, never as a second emission simulation. These are initial
   art values, not measured sprinkler spectra.
2. **Seam:** warm filters before rendering; author a 4–8 second mono bed with 50–150 ms
   overlap/crossfade and inspect boundary sample/first-difference continuity. Four 8-second
   mono float beds at 48 kHz occupy about **5.86 MiB**; share buffers among voices. Randomize
   bed texture/seek by stable sprinkler ID, **not valve phase**.
3. **Pulse/impact transient:** original 10–40 ms band-passed noise burst with 1–3 ms attack
   and exponential decay; optional quiet damped resonant component. Trigger on authoritative
   valve opening, not on every rendering frame. This is a stylized valve/water pulse cue;
   current brass/pulse hardware arms are static and it must not pretend a synchronized
   mechanical impact animation exists. At 100% duty, play the hiss continuously, not a
   repeated valve-opening click. At 0%, no hiss or transient. Do not normalize away duty.
4. **De-click:** smooth the sample-domain gate with a bounded 2–5 ms attack and 5–15 ms
   release, shortened for very narrow pulses; preserve quiet short pulses rather than
   extending them into continuous spray. Keep fixed headroom (e.g. peak <= 0.3 per authored
   bed before mix) and test summed peaks; a limiter is not permission for unbounded voices.
   Removal/stop already has a 5 ms PetalSonic fade, but gain, pause and bus-mute smoothing
   must be checked separately: do not assume arbitrary `update_source_volume` calls ramp.
   [R3, R5]
5. **Spatial/lifecycle:** nozzle-centered mono point source, existing distance/occlusion and
   saved master mix, no new per-drop sources/rays or separate GPU readbacks. Quiet surface
   patter can be a filtered component of the same voice initially; exact impact-position
   sound is out of this placeholder's fidelity claim. Hardware appearance remains appearance.
6. **Pause/mute/removal:** world pause closes output through a short ramp and freezes the
   emission phase; resume from the supplied snapshot without catch-up ticks. Mute suppresses
   output but must not accumulate a burst queue. Deleted sources fade/retire and release
   registrations even if paused/muted. Failed audio creation must never roll back watering.
   Pending voices must cancel cleanly; retryable removal errors must retain ownership.
7. **Bound:** propose **16 selected sprinkler voices total**, including retiring voices,
   with hiss+transient summed in one voice where possible. Select nearest/audible sources
   using stable-ID tie breaks and hysteresis; if a retiring slot is occupied, delay a new
   sound rather than exceeding the cap. All sprinklers keep watering regardless of selection.
   Do not use an unbounded transient voice per pulse. Budget selection/retirement tests at
   0/1/4/8/16 and >16 sources; do not claim this cap is performance-accepted yet.

## Repository ownership and shared-clock contract

Inspected read-only at the base commit; paths below are exact likely wiring points, **not edits**.

- `src/sprinkler.rs::SprayClock::sample` integrates canonical world tick/fraction deltas. It
  stores bounded rotor angle, pulse phase, droplet time and quadrature. Its `[f32;4]` contains
  **pulse seconds within the current cycle** at index 1, not raw phase; multiply by the
  clamped frequency to recover normalized valve phase. Frequency is 0.25–20 Hz. [R1]
- `src/tracer/mod.rs` owns `sprinkler_clock` (around lines 1648/3189) and samples it during
  frame input update; `src/tracer/buffer_updater.rs` maps that array to shader inputs. Sampling
  currently happens inside the renderer. Audio must not instantiate a second `SprayClock`.
- `shader/slang/sprinkler_spray.slang` owns `sprinklerPulsePrimitive/Amount`. At source age zero,
  valve state is `frac(pulseTime * frequency + frac(animation_phase)) < duty`; airborne source
  samples subtract age. `mistRotarySource` uses the same per-instance phase for rotation and
  pulse offset. CPU sound should represent emission, not copy flight-time integration. [R2]
- `src/app/core/placeables.rs::SprinklerRuntime` owns monotonic non-reused `u32` record IDs,
  base positions, per-instance `animation_phase`, revision and placement/removal transactions.
  Current `render_instances()` omits ID; do not key sound by vector index, position or raster
  light emitter presence. Add a read-only audio snapshot iterator **after committed placement/
  removal**, including ID and phase; preview/failed transactions must remain silent.
- `src/audio/spatial_frame.rs::SpatialFrame::advance` is the spatial frame transaction owner:
  drain telemetry once, observe listener, retire/prepare sources, publish the full generation,
  then start prepared voices. Failed publication aborts prepared footsteps. Add sprinkler
  prepare/commit/abort alongside it, not a second frame publisher or telemetry drain.
- `src/audio/spatial_sound_manager.rs` owns PetalSonic world, buses and registered sources.
  `add_clip_source` currently starts playback before marking structure dirty; **do not blindly
  reuse that convenience path for new clock-controlled voices** requiring publish-before-play.
  Its controlled transient prepare/play methods and the footstep transaction are the better
  lifecycle reference. `try_remove_source` preserves registration on destroy error, unlike
  the best-effort removal helper. [R3]
- `crates/petalsonic/src/world.rs` exposes controlled play/pause/stop; `render.rs` handles
  stop/destroy commands and `playback.rs` supplies a 5 ms retirement ramp. Public API reviewed
  here exposes immutable resident PCM, **not a verified custom streaming DSP callback**.
  Tree synthesis itself pre-renders PCM. A sample-domain valve envelope is therefore a real
  implementation seam to resolve, not a capability this note assumes already exists. [R4, R5]

### Recommended bounded interface

Prefer one app-owned sampled **`SprinklerFrameSnapshot`** in `src/sprinkler.rs`, passed both to
renderer inputs and audio. Move the existing clock ownership out of Tracer in one serialized
change; do not alter the shader ABI or independently reconstruct time in audio. Snapshot fields:
monotonic frame/revision, world tick/fraction, rotor angle, normalized pulse phase, clamped Hz,
duty, paused/emission-enabled status, existing droplet/quadrature values. Per-source facts:
`{ stable_id, base_position_ws, animation_phase }` from committed runtime records. No GPU contact
list or per-drop events needed. Keep pause/emission-enabled separate from optical density and
`--no-particles`; disabling visible mist must not mute water or stop irrigation. RPM 0 is not
valve closure. Wetting-speed artistic scaling is not sound volume authority. [R1, R2, R6]

If moving ownership is too disruptive during performance work, a typed **read-only accessor to
the exact sampled Tracer snapshot** is a coherent first seam, provided ordering guarantees the
same current snapshot reaches audio and GPU. Do not call `sample` again through an audio path.
The controller should choose one ownership design before concurrent production edits.

The eventual sound adapter should accept only `reconcile(snapshot, committed_sources, listener)`
and own bounded source selection, resident PCM and retirement. The renderer consumes the same
snapshot through `BufferUpdater::update_gui_input`. The app frame at
`src/app/core/mod.rs` (spatial advance around line 4110) joins these facts.

**Timing requirement:** Frame-rate-only gate updates alias at 20 Hz (and 30 Hz spatial pose
publication must not become a 30 Hz valve clock). Recommend a small explicit PetalSonic
sample-domain envelope control for resident voices, supplied with the authoritative phase,
rate, duty, revision and pause state. Bounded interpolation between current snapshots is
permitted solely to render samples; reset/re-anchor on every authoritative update, parameter
change, pause and discontinuity. Use latest-state replacement, no unbounded event queue, and
fade to silence on stale control rather than free-running indefinitely. Specify and test the
look-ahead/staleness bound and device latency before implementation. This requires careful
engine work, not a new independent simulation. A pre-baked looping hiss+click cycle that just
runs on the device clock without re-anchoring **does not** meet the clock invariant.

Likely new files: `src/audio/sprinkler_audio.rs`, `src/audio/sprinkler_synthesis.rs`. Shared
serialized edits: `src/sprinkler.rs`, `src/tracer/mod.rs`, `src/tracer/buffer_updater.rs`,
`src/app/core/{mod,placeables}.rs`, `src/audio/{mod,spatial_frame,spatial_sound_manager}.rs` and,
if the envelope needs it, `crates/petalsonic/src/{world,playback,render}.rs` plus its command/
control types. Performance worker owns Tracer changes first; agree snapshot contract before
editing it. Wetness should consume existing moisture, not extend this sound snapshot into a
second wetness authority. Do not touch generated structs by hand.

For first delivery, routing through existing `AudioCategory::Terrain` is the smallest coherent
mix change (document that it affects sprinklers). A dedicated saved sprinkler channel is optional
and would require additive migration in `src/audio/mixer.rs`, declarative GUI configuration and
`docs/agents/gui-settings.md` review, not an App-only slider or writing existing preferences.

## Required later acceptance (not run in research)

- Deterministic PCM checks: finite/DC/peak/RMS, loop seam and maximum transition discontinuity,
  de-click starts/stops, repeatable seeds, bounded buffers/commands/voices including retirement.
- Shared-clock regressions: duty 0/100/intermediate, 0.25/4/20 Hz, RPM changes including zero,
  frequency changes preserving phase, tick wrap, frame skips, low/high render rates, time pause,
  mute/unmute and source removal/replacement while paused. Compare emission gates to the
  production shader convention, not only to a duplicate audio helper. No delayed burst replay.
- Publish-before-play, failed publication/creation/removal, missing output device, cap/hysteresis,
  master/category mix and shutdown cleanup checks. App sound disabled must not alter watering.
- Later source changes need the project validation ladder and Release build under the shared
  heavy-resource lock, then hidden muted runs and actual log inspection. Offline signal export
  may supply a listenable preview; **no listening approval exists** until the user auditions it.
- Measure bounded incremental Release CPU/acoustics cost at matched sprinkler populations;
  do not infer performance from the procedural design or unit tests.

## Primary sources and retained evidence

All links accessed **2026-10-21** unless marked inaccessible. Public text only. `main` URLs can
change; short quotes above and full SHA-256 below retain the decision-critical evidence even
if disposable worktree `target/` evidence is cleaned. Full fetched texts currently live in
`target/garden-delivery/audio/`; no audio/model files exist there. This single Markdown is the
only committed research artifact.

- **A1** [AudioCraft code MIT](https://github.com/facebookresearch/audiocraft/blob/main/LICENSE).
- **A2** [official AudioGen model card](https://github.com/facebookresearch/audiocraft/blob/main/model_cards/AUDIOGEN_MODEL_CARD.md), quote: “Code is released under MIT, model weights are released under CC-BY-NC 4.0.”
- **A3** [official weights license](https://github.com/facebookresearch/audiocraft/blob/main/LICENSE_weights).
- **A4** [official AudioGen instructions](https://github.com/facebookresearch/audiocraft/blob/main/docs/AUDIOGEN.md), quote: “at least 16 GB of memory”; [HF card](https://huggingface.co/facebook/audiogen-medium/blob/main/README.md).
- **A5** [CC BY-NC 4.0 legal text](https://creativecommons.org/licenses/by-nc/4.0/legalcode.txt), §2(a)(1).
- **S1** [Stability official announcement](https://stability.ai/news/introducing-stable-audio-open) (redirects to `/news-updates/introducing-stable-audio-open`).
- **S2** [stable-audio-tools README](https://github.com/Stability-AI/stable-audio-tools/blob/main/README.md).
- **S3** [stable-audio-tools MIT](https://github.com/Stability-AI/stable-audio-tools/blob/main/LICENSE).
- **S4** [1.0 model card](https://huggingface.co/stabilityai/stable-audio-open-1.0/raw/main/README.md) and [model license](https://huggingface.co/stabilityai/stable-audio-open-1.0/raw/main/LICENSE.md): **HTTP 401**, not bypassed. No statement here that the current gated text was read.
- **S5** [Stability Community License Agreement](https://stability.ai/community-license-agreement), accessible official HTML; exact model applicability still requires S4.
- **T1** [official TangoFlux README](https://github.com/declare-lab/TangoFlux/blob/main/README.md).
- **T2** [official TangoFlux model card](https://huggingface.co/declare-lab/TangoFlux/blob/main/README.md), §License research-only quote above.
- **T3** [developer-published Stability license copy](https://github.com/declare-lab/TangoFlux/blob/main/STABILITY_AI_COMMUNITY_LICENSE.md).
- **T4** [actual repository LICENSE.md](https://github.com/declare-lab/TangoFlux/blob/main/LICENSE.md), [Notice](https://github.com/declare-lab/TangoFlux/blob/main/Notice). Guessed `/LICENSE` and `/LICENSE_CODE` returned **404**; GitHub contents API located the real file. No license was inferred from those failed requests.
- **R1** [`SprayClock`](../../src/sprinkler.rs), [`BufferUpdater`](../../src/tracer/buffer_updater.rs), [`Tracer`](../../src/tracer/mod.rs).
- **R2** [production spray source](../../shader/slang/sprinkler_spray.slang), [sprinkler ownership/control docs](../sprinkler-spray.md).
- **R3** [spatial transaction](../../src/audio/spatial_frame.rs), [manager](../../src/audio/spatial_sound_manager.rs), [controlled footstep precedent](../../src/audio/local_player_footsteps.rs).
- **R4** [original ambient tree synthesis (historical)](../procedural_tree_rustle_progress.md), [retained finite kick synthesis](../../src/audio/tree_kick_rustle/synthesis.rs), [PetalSonic public API](../../crates/petalsonic/src/lib.rs).
- **R5** [playback fade](../../crates/petalsonic/src/playback.rs), [render command processing](../../crates/petalsonic/src/render.rs), [world controls](../../crates/petalsonic/src/world.rs).
- **R6** [committed source identity/transactions](../../src/app/core/placeables.rs), [moisture staging](../../src/app/core/moisture.rs), [saved mix](../../src/audio/mixer.rs).

Fetched-text SHA-256 (not model/checkpoint hashes):

| Source / local text basename | SHA-256 |
| --- | --- |
| A1 `audiocraft-license.txt` | `da6d3703ed11cbe42bd212c725957c98da23cbff1998c05fa4b3d976d1a58e93` |
| A2 `audiogen-full-card.md` | `b4e4679785970dcb7543515193b05904a988bec6a3e7d4bdf2acd3292676c536` |
| A3 `audiocraft-weights.txt` | `336255dc30193e8e15d689d9481bb05673d89055718f3a96923a7ffb99adbbaf` |
| A4 `audiogen-docs.md` | `02089980a05137bdd075f318abdd70cb3c48da7d5564d43b34d2ff2dcb9e570b` |
| S1 `stable-announcement.html` | `990de205e12a7065dc585b8d7387d60213eb9ec24783cae4ef3ab1c2778476fe` |
| S3 `stable-tools-license.txt` | `a1fac33b7bcd791b74fb33aeb439f825e7277e239fc119fb7d2ab6f084a0c101` |
| T2 `tangoflux-card.md` | `6e53403c2d5a46accaad3543ec2b0f5babe7cfdc59c1bb27afad504d5f83b8e6` |
| T3 `tangoflux-stability-license.md` | `5f42db121bd67b99f549100ef665d92765a154abe3430a6e36c638ed2fbd96ca` |
| T4 `tangoflux-license.md` | `500719bcf10a22d4b6e6ac5f7d848a93ac10d7fc12d68662049d724e555c2a8c` |

These are engineering clearance recommendations, not a legal opinion. Recheck exact versions
and license texts before any future model acquisition or commercial authoring.
