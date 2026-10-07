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

Pending at the first-stage commit; subsequent evidence will replace this section.
