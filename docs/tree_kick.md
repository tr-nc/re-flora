# Walking tree kick

Walk near visible tree wood and aim the center reticle at it. `K · Kick` appears below the reticle
while in reach and ready; press **K** once. Holding K does not auto-repeat. Editing/free-flight,
GUI keyboard/pointer ownership, camera transitions, stale scene answers and out-of-reach hits
cannot kick. Cooldown is player-wide. Each accepted kick sends bending from that tree's basal
axes along its actual branch paths. Canopy arrival, not the key press, triggers one decorative
leaf burst, independent apple-release attempts and leaf rustle. It does not damage wood, move
physical tree collision, or add a foot animation.

## Saved tuning

Debug → **Tree Kick** groups all controls by the interaction concern. Use the normal Debug **Save**:

| Setting | Default | Range |
|---|---:|---:|
| Tree kick impulse | 0.2 rad/s | 0–4 |
| Tree kick shake speed | 2.55 Hz | 1–7 |
| Tree kick damping | 0.3 | 0.1–0.9 |
| Tree kick reach | 30 voxels | 16–100 |
| Tree kick cooldown | 0.35 s | 0.15–1.5 |
| Tree kick wave speed | 600 voxels/s | 100–2000 |
| Fruit release chance scale | 0.25 | 0–1 |
| Decorative leaves per burst | 24 | 0–128 |
| Leaf rustle volume | −6 dB | −40…6 |
| Leaf rustle tail | 1.1 s | 0.25…3 |
| Leaf rustle body (low = crisper) | 0.28 | 0–1.5 |
| Leaf rustle brightness | 0.42 | 0–1 |
| Leaf contact detail | 0.18 | 0–1 |

Higher impulse is stronger; higher frequency is faster; lower damping rings longer. Lower wave
speed slows both actual branch bending and canopy consequences, not just the sound. Mechanical
frequency/damping/speed are captured per accepted impulse, so live retuning affects the next wave
without teleporting an in-flight front. Zero impulse disables the action. 256 terrain voxels
are one garden world unit. Old saves acquire missing declarations in memory, without being rewritten
on Load; existing values survive Save/reload. Kick response itself is transient, not a saved tree fact.

## One-shot leaf and fruit release

Mature fruit remains attached at the end of the saved **Fruit Growth (held until kick)** control.
Registration, loading, advancing the growth control, wind and elapsed time cannot schedule a drop.
Every authored apple gets a seed/anchor-stable random **connection strength** `s` in 0.1–0.9,
preserved across age rebuilds and reloads. Each accepted kick's canopy arrival draws new independent uniform luck
`r` for every still-held, fully-grown apple on that tree. It releases only if
`r > 1 - chance_scale * (1 - s)`; at scale 1 this is simply `r > s`. Stronger connections win less
often. A failed draw changes no fruit-release state (no weakening, extra fruit impulse or queued
drop); it remains attached and follows its branch. Later kicks reroll.
There is no quota, rounding-up, minimum winner or per-kick winner cap: **zero apples may fall**.
Already-pending/dropped apples cannot draw twice. GPU admission still bounds concurrent handoffs
at 256, deferring excess winners through the same frame-owned queue rather than omitting them.
Zero chance scale disables fruit attempts; zero leaf count disables the leaf burst. The old saved
fraction slider migrates to chance scale in memory with its numeric value unchanged; it no longer
promises that percentage will fall. Remaining canopy geometry is unchanged: falling leaves are the
existing independent decorative VFX, not a revived detachment/regrowth lifecycle. Ambient
wind-driven decorative leaves are unchanged.

Fruit stays visibly attached until collision along its fall path is ready and an asynchronous
handoff completes. Normal rendering copies only four published held-response float4 corners
(**64 bytes/fruit**) into frame-owned readback. The same frame's small attachment transforms,
pendulum geometry and velocities give its rigid body's starting pose. The acquisition seam
collects completed readbacks before fence reuse; buffer tracking keeps producers resident.
There is no dedicated kick/drop GPU submission or wait and no full-field readback. A changed tree
publication rejects an old handoff; changed fruit specs cancel pending release rather than dropping
a replacement crop. Falling bodies then use the existing gravity/contact/rendering path.

Dropped fruit is consumed for the current crop; it does not automatically respawn with time. The
explicit Debug growth scrub below that fruit's growth start rearms it. Saved world loads restore a
held crop; loose bodies and harvested state remain transient, as before. Opt-in drop/visual fixtures
now request explicit diagnostic kicks instead of relying on growth thresholds.

## Leaf rustle sound

Each accepted kick's canopy arrival with an acoustic sample plays one finite spatial leaf rustle,
even without wind. It reuses the existing leaf-body/friction/micro-grain synthesizer with a fast attack,
energy decay and subtle back-and-forth shake modulation. The former 1.14 warm-body weight is now
saved/tunable at 0.28 by default; the air bed is reduced and brightness/contact detail use a
clearer mapping. Ambient tree synthesis is unchanged. Branch creaks remain off; no foot/trunk
thump is added. The sound source sits at a wood-clear canopy sample on
that tree, retaining normal spatial attenuation, transmission and environmental acoustics.

Four immutable 48 kHz variants are prepared before K, not synthesized in the realtime callback.
Live sound/frequency tuning rebuilds that small bank on the main thread; Release default preparation
measured **20.598 ms** for this clearer candidate in the final hidden Release fixture (the former
9.562–9.638 ms measurements predate this change), not an app performance acceptance. At most eight finite
sources overlap; wall-time retirement and shutdown cleanup prevent accumulation. The ordinary
wind ambience/volume does not gate this event. Sound failure is reported without rolling back a
successful shake/leaf/fruit event.

Volume, duration, body, brightness and small-contact detail use normal saved declarations. These are
sound-design choices, not a measured tree-kick IR. Findings and primary sources are in
[`docs/research/tree_kick_leaf_sound.md`](research/tree_kick_leaf_sound.md). Listening remains manual;
a muted run only verifies playback admission/lifetime and runtime health.

## Ownership and rendering

The prompt uses the existing nonblocking GPU hover. A press captures a fresh ray/settings and asks
for a frame-owned `Kick` pick: it never blocks for a dedicated query. The current publication's exact
triangle bindings resolve its canonical tree ID in O(3), rather than guessing the closest origin.
Mixed/unbound/out-of-range triangles are rejected. Terrain/model occlusion, scene-version checks,
250 ms intent age, current walking/reach checks and context cancellation apply before the impulse.
A quickly changed camera direction immediately hides old hover prompts. Only one kick is pending.

The former **rigid whole-tree root tilt is removed**. Each canonical `TreePose` owns at most eight
analytic bend packets. Only packet ages advance on CPU. A **272-byte/tree** header/packet bank
feeds the existing GPU hierarchy job. For each joint, the signal uses elapsed time minus its
cumulative authored branch-start path / saved wave speed; before that front arrives its added
bend is exactly zero. Sum-of-packets tilt is limited locally to 0.2 rad, rather than instantly
rescaling old waves when another kick arrives. A full bank rejects admission without truncating
an accepted wave/event; the prompt hides until space returns.

The GPU keeps the original wind hierarchy separately, then adds the delayed **absolute** kick
orientation. A child cannot inherit an instantaneous root kick rotation. Its pivot still follows
the parent's endpoint: small connected rigid translation can precede local tip vibration, rather
than separating/cracking the skeleton. Roots stay anchored and segment lengths remain fixed.
Compact joint state/readback is **96 bytes/joint** (formerly 64), not a dense vertex/leaf copy.
Skinning, normals, BVH, attachments and canopy drawing all use this one final pose publication.
No new query submission/wait, wood-topology publication or collider refit is added. An impulse
does not advance the skeleton generation or reset the whole GPU solver allocation.

The same packet clock crosses a representative canopy path (median terminal endpoint distance),
and emits its leaf/fruit/sound consequence once. Longer trees and lower wave speed increase both
rendered propagation and this onset. This is a saved art-directed travelling-bend model, not a
measured wood elastodynamics/velocity model; the single canopy burst/source is an aggregate, not
per-leaf audio contact simulation. Tree replacement/removal discards pending tree-owned waves;
a suspended frame cannot revive an already-passed canopy event. Wind off resets ambient branch
deflection but does not suppress kick transmission.

## Validation and limits

- `cargo fmt --check`, `cargo check`, Release build and hidden muted launch/log inspection.
- Wave tests (5) cover front travel, away/rebound/settle, frame-rate equivalence, local repeated
  impulse bounds, causal re-kicks, backpressure without lost arrivals, and invalid/zero/suspended
  inputs. Pose tests (13) cover delayed upper axes, original wind-state preservation, connected
  pivots, anchored roots, path/size/speed
  and translation invariance, unchanged wind behavior, and generation preservation.
- Tests cover checked triangle ownership, reach/facing, migration without implicit disk writes,
  generic saved-field roundtrips, and single ownership of each Debug control.
- Hidden raster smoke injects an explicit impulse with wind off. GPU hierarchy matches its CPU
  oracle; actual wood moves while authored rest geometry and physical collision remain unchanged:
  `[TREE_KICK][GPU] passed wind_off=true moved=true rest_collision=true rest_geometry=true`.
  At slow diagnostic wave speed, **268 upper axes remain unbent by the kick**, with sound and
  fruit release still absent. The fixture then seeks the same mechanical packet clock to canopy
  arrival; normal pose updates (not a direct fixture drop/play call) trigger consequences once.
  It settles before unrelated wind toggles. Production input never performs these seeks.
  It also checks overlapping high-energy waves with ambient wind enabled using the same GPU
  hierarchy/skin oracle. Current evidence: `/tmp/tree-kick-wave-smoke-final.out`.
- Fruit/roulette tests, particle/app burst tests, kick migration, unified Save and Debug grouping
  pass. The current Release smoke pins only its diagnostic RNG (live play is not pinned), starts
  with **17/17 fruit held**, attempts scale 1, releases **9** and retains **8** from one kick,
  emits **24** leaves, and records a **576-byte** async handoff with
  `frame_fence=true query_wait=false`. Regression tests cover all-lost attempts, equality retaining
  attachment, fresh retry draws, stronger/weaker thresholds, unchanged strength, no double draw,
  more than 256 independent winners, and saved quota-to-chance migration. Existing frame-owned pick, posed rest-space edits, leaf
  response, replacement/removal, Conifer, lighting and exact rollback smoke remain green.
  Roulette evidence: `/tmp/tree-roulette-smoke.out`. The former 5/17 fixed-quota result in
  `/tmp/tree-kick-release-smoke-02.out` is historical, not the current release policy.
- The clearer four-variant PCM test increases the high/low energy proxy (high-pass residual above
  2.5 kHz / low-pass below 800 Hz) from **0.241 to 2.240** over four deterministic seeds. This
  proves the spectral change, not subjective naturalness. Evidence: `/tmp/tree-kick-wave-sound-spectrum.out`.
- Excluded intermediate runs: an earlier binding-4 collision (kick packets use binding 5), and
  `/tmp/tree-kick-wave-smoke-01.out`, whose fixture consumed an unpublished next hierarchy before
  comparing its CPU oracle to the older skin palette. The pairing was fixed without loosening
  the existing GPU position/normal tolerances.

Manual walking/K prompt and shake feel are not yet visually reviewed. The GPU smoke verifies the
nonzero response and rendering/collision correctness, not an actual OS keyboard-driven interaction.
Full repository test-suite success and performance acceptance are not claimed. No visible game was
automatically launched; the Release binary in this worktree is ready for user review. Inspected
short-run log: `target/re-flora-logs/re-flora-20261008-002246.105-2317102.log`.
