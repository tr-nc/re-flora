# Tree-kick acoustic-start and distributed-canopy repair

## Stage 1: correct finite spatial acoustic start

Completed before beginning the canopy-emission change. Saved GUI tuning is untouched.

The shipped dependency was PetalSonic 0.9.2. Re: Flora now pins the same published
source under `crates/petalsonic` with a narrow MIT-licensed patch, via
`[patch.crates-io]`. This is reproducible on other machines and does not reference
an unpublished git commit or another local checkout. Upstream identity and checksum
are recorded in `crates/petalsonic/VENDORED.md`. The sibling backend's later limiter
is **not** implicitly adopted.

`PlayOptions::with_acoustic_start()` is an opt-in finite-spatial contract:

- Keep the original PCM cursor while the initial transmission response is absent
  or deferred. Do not render a unity fallback or silently eat the attack.
- Initialize point, distributed-lobe and environment filters to the first usable
  targets before consuming PCM. Ordinary subsequent changes still smooth normally.
- Processing is nonblocking. A 100 ms render-time maximum bounds the initial wait;
  timeout retires silently without natural completion, with opt-in timeout telemetry.
- Explicit stop/destroy cancels an unrendered Voice without waiting/consuming PCM.
- Disabled acoustics and bypass-only routes do not require a geometry response.
- Reject the contract on looping/non-spatial playback. Default/ambient playback,
  synthesis, material transmission and mix gains are unchanged.
- Empty solved lobe sets remain held rather than becoming unoccluded on a later
  missing response.

The finite resident-clip adapter enables this contract for kicks. Its lifetime
includes the maximum start delay so delayed admission cannot cut the original clip.

### First-stage snapshot evidence

Same `bad-audio-case` viewpoint, same nearest point, resident variant 0 and saved
2-second/body-0.5 tuning as the original diagnosis:

| Metric | Original | Acoustic-start fixed, still a point |
|---|---:|---:|
| Source energy | 1.651539294752477e-6 | 1.651539294752477e-6 |
| Direct energy | 2.704536903936544e-8 | 2.3783392393834295e-10 |
| Direct/source ratio | 0.016375855618633 | 0.000144007425 |
| First render has usable response | no | yes |

The fixed energy equals the previously captured native replay initialized
consistently with wood. The artificial opening burst is gone. This intermediate
point is legitimately wood-obstructed, so it remains quiet; no volume compensation
was added. Full PCM source energy is preserved.

Temporary snapshot probes drove a checked diagnostic pick followed by normal kick
and canopy-arrival handling, not OS K/ordinary async admission. All temporary
`[DEBUG-badaudio]` probes were removed before the clean handoff build.

Artifacts: `/tmp/kick-acoustic-start-{red,tests-final,suite-final,snapshot,check-final,clean-build,clean-smoke}.out`.

### First-stage validation

- Red real-processor regression: `cargo test --manifest-path crates/petalsonic/Cargo.toml acoustic_start_keeps_attack -- --nocapture` failed before renderer fix.
- Backend tests: **189 passed / 8 ignored**, runtime-ownership **6 passed**, doctests **2 passed**.
- New tests cover delayed/missing/deferred responses, point/lobe/environment bootstrap,
  full cursor completion, bounded timeout/telemetry, explicit stop, bypass start,
  and invalid looping/non-spatial requests. No normal test opens an audio device.
- Game/backend formatting checks and `cargo check` passed.
- Clean `cargo build --release` and hidden muted Release run passed; inspected
  `target/re-flora-logs/re-flora-20261008-023747.033-2398040.log`, shutdown `failures=0`.

No audible-naturalness or current sustained-performance claim is made by muted
runs. The original diagnosis remains historical evidence:
`docs/research/tree_kick_snapshot_short_sound.md`.

## Stage 2: coherent distributed canopy emission

Completed after first-stage commit `07865aa7`.

The finite Voice captures the current canonical tree's entire existing acoustic domain,
not the nearest sample to the listener. The listener is no longer an input to
`TreeAudioManager::play_kick_rustle`. The origin is the stable tree-local weighted
sample center; the unchanged sample IDs/positions/normalized power are rebased about it.
This puts the finite event's attenuation anchor in the canopy, not at the trunk/root.
Distance attenuation is the existing center-based extended-source model, not a new
per-leaf propagation simulation. In-flight immutable events do not reselect their domain.

`WeightedSamples` uses exact transmission (`OcclusionProfile::PointExact`), grouped into
at most four directional lobes. Extent and occlusion policy are orthogonal: **no ambient
attenuation floor is borrowed** to brighten a blocked cue. Unobstructed samples contribute
only their normalized power; obstructed samples keep the material response. Still one
finite Voice per event, maximum eight events; no sixteen-full-volume-Voice multiplication.
Ambient callers keep their original zero/tree-root sample anchor and policy unchanged.

The root-cause start regression also covers the muted render shortcut: pending PCM stays
unconsumed and has the same bounded cancellation rather than bypassing DSP indefinitely.
Default muted/immediate playback remains unchanged.

### Original snapshot, after both fixes

- Same canonical tree `0`, variant `0`, saved duration `2`, same complete PCM source
  energy **1.651539294752477e-6** in the muted runtime.
- Stable canopy origin **(1.0274838, 0.7554647, 1.0493407)**; **16** samples,
  **7 wood hits / 9 visible**. Visible normalized source power **0.5511977**.
- Exact aggregate amplitude **[0.7443588, 0.7427971, 0.7424949]**, identical raw/filtered
  gains (no attenuation floor). The formerly selected wood-obstructed point is still
  present with its original **[0.08, 0.035, 0.015]** response and **0.08107397** power.
- First-render telemetry already contains a usable scene response. Source starts
  only after preparation; the log's creation-to-first-render observation is about 32 ms.
- Direct energy **4.817138401757878e-8**; natural energy summary follows the complete
  clip, before bounded source retirement. No gain/timbre adjustment or bypass control.

Farther walking placement and a walking viewpoint on the other side both check-hit the
same tree, preserve the same origin/domain/PCM energy and start with valid responses.
Accepted far listener **(0.79085577, 0.49835917, 0.7892942)**, direct energy
**2.736184807050929e-8**; opposite-side listener **(1.0398363, 0.4866406, 1.0595495)**,
direct energy **6.306550221288221e-8**. These are legitimate geometry/distance changes,
not a listener-driven source switch. All three accepted captures exit successfully,
shutdown `failures=0`. Temporary camera entries were restored byte-for-byte.

Excluded far attempt: the walking controller moved the camera and the unchanged view ray
missed the scene. That run produced no kick/audio evidence; the retry explicitly looked
at the previously captured trunk hit from its actual post-grounding listener position.
No failed run is included in the accepted audio comparison.

### Actual PCM + native spatial-DSP replay

The explicit ignored backend test `captured_canopy_pcm_keeps_its_temporal_tail_and_output_headroom`
replays the four game-synthesis PCM variants with the captured solved lobes, listener basis,
canopy origin, native HRTF/direction-field renderer and saved unmuted gain/headroom.

| Variant | Stereo peak before hard clamp | Clipped samples | First 100 ms energy | After 250 ms energy | Time to 95% energy |
|---|---:|---:|---:|---:|---:|
| 0 | 0.307245 | 0 | 35.33% | 27.37% | 595.937 ms |
| 1 | 0.267983 | 0 | 36.33% | 27.71% | 612.667 ms |
| 2 | 0.331527 | 0 | 33.16% | 32.14% | 624.667 ms |
| 3 | 0.337908 | 0 | 35.89% | 27.80% | 604.937 ms |

Contrast the diagnosed **97.13% first-100-ms / 80-ms direct-energy t95** burst.
The current numbers are binaural DSP metrics, not perceptual duration. No stereo cue
clips in this isolated captured-viewpoint replay; concurrent mix/device-rate resampling,
downstream hardware and manual listening remain outside this test. The independent
published backend hard-clamp/overloaded-mix risk is not claimed fixed.

The replay requires current 48-kHz mono `variant-0..3.f32` files (raw little-endian)
in `RE_FLORA_KICK_PCM_DIR`; these were exported using the real game leaf synthesizer,
current saved tuning and production seeds in the ignored diagnostic workspace.
It is not an ordinary device/I/O unit test:

```bash
RE_FLORA_KICK_PCM_DIR="$PWD/target/kick_audio_distance_probe" \\
  cargo test --release --manifest-path crates/petalsonic/Cargo.toml \\
  captured_canopy_pcm -- --ignored --nocapture
```

### Final validation and cleanup

- Call-site regression `kick_rustle_has_exact_tree_placement_no_wind_dependency_and_bounded_voice_lifetime`
  went red on the original point domain, then passed on actual distributed registration.
- Game pure kick/source tests **5**, canopy tests **18**, explicit audio-runtime test **1**,
  pose tests **13**, packet tests **5** passed.
- Backend **190 passed / 9 ignored**, ownership **6**, doctests **2** passed; ignored
  captured-PCM replay explicitly passed for all four variants.
- Existing isolated Release 8-Voice/16-sample direction-field gate passed:
  p50 **1042 us**, p95 **1134 us**, p99 **1175 us**, realtime CPU **4.0985%**.
  This is a backend synthetic renderer gate, not sustained app/walking FPS acceptance.
- Formatting (both manifests), `cargo check`, clean Release build and inspected short
  hidden muted launch passed. Two existing game diagnostic dead-code warnings remain.
- Full hidden raster/vegetation-response smoke passed delayed upper-axis bending,
  wind-off and overlapping wind-on impulses, **9/17** seeded roulette, **24** decorative
  leaves, **576-byte** fenced handoff/no query wait, exact picking, rest collision,
  leaf history, lighting, age/replacement/Conifer and exact publication rollback.
- Inspected logs:
  `target/re-flora-logs/re-flora-20261008-034327.768-2422062.log` and
  `target/re-flora-logs/re-flora-20261008-034333.849-2422110.log`; no errors,
  shutdown `failures=0`.
- All temporary `[DEBUG-badaudio]` and camera-adjustment probes removed from game source.
  No generated files changed; no visible game auto-launched.
- Saved Settings SHA-256 unchanged:
  `c4c20e33963c225c500d674ab58225a852fe9947d1808578317e767fe0a77c85`.
  Camera snapshot file unchanged:
  `d45b3c3875c2896a7b5c83b19520f8401bf29fc7208063fde5bb95191bf7f29a`.
- Final ordinary Release SHA-256:
  `0796f5b9c04975ce4e9283701de9e02df92539b6623be2e9415c3fb1285d897d`.

Artifacts: `/tmp/kick-canopy-{snapshot,back,far-facing,pcm-replay,render-budget}.out`,
`/tmp/kick-audio-final-*.out`. Local ignored archive:
`target/kick_audio_distance_probe/evidence/repair/` and the instrumented
`re-flora-fixed-canopy-diag` binary. The archived diagnostic does not include the later
muted-shortcut guard; the ordinary validated Release does.

Both requested implementation causes are closed with regressions and real-app evidence.
No human-hearing, OS-keyboard, full-repository-suite or sustained-game-performance
acceptance claim is made, and no further user reply is required to finish this repair.
