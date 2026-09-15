# Local leaf flutter frequency

Implements the bounded tuning proposal in `leaf_flutter_frequency_research.md`.
No claim of a universal botanical frequency/wind law.

## Current controls (direct curve editor)

Debug → Flora → Leaves → Wind Motion → Flutter Frequency:

- One Frequency Scaling slider multiplies the whole curve, range 0.25–2.
- Drag the two blue endpoints to set wind thresholds and corresponding natural
  frequencies (0.25–24 Hz before scaling). Drag the gold middle handle to shape
  the transition; it supports rising and falling curves. Values appear on hover.
- No Base/Strong-Wind slider, threshold/bias sliders, target-frequency summary,
  frame-rate readout or explanatory paragraphs in this frequency menu.
- Wind zero is the left edge. Below/above the two thresholds the curve retains
  its endpoint frequency; amplitude still determines whether calm leaves move.

All six curve/scaling fields use declarative persistence, edited directly rather
than copied into temporary App state. Old saved base/multiplier values migrate
on load to low=base, high=base*oldMultiplier, scaling=1; explicit new fields win.
Migration removes the old IDs on the next normal Save, with no per-setting save
hook. This checkout's saved 1.5 / 2 becomes 1.5→3 Hz, scaling 1; wind thresholds
0.05/1 and bias 0.8 are preserved. Other saved GUI settings remain unchanged.

## Motion and display

Scale both mechanical frequency and the two seeded smooth-noise rates together,
retaining damping ratio and independent amplitude controls. No periodic sine
replacement, new emitter or separate world wind field. Natural frequency is
shared by the setting; stable per-leaf noise rates/seeds retain individual motion.

Each authoritative leaf state owns two fractional noise coordinates and two
unsigned integer cell counters. Retuning integrates future phase increments;
it never multiplies accumulated world time by a new frequency or resets pose.
Fractions remain [0,1), integer wrap preserves the noise interpolation endpoints.
Maximum-frequency integration subdivides locally at least 32 steps per target
cycle in normal frames, with a finite substep budget for stalls. A +/-1 radian
mechanical travel stop removes outward velocity on impact at extreme retunes;
the interior angle and velocity remain continuous. The existing 5-voxel local
excursion limit remains unchanged.

Local flutter rendering now reads the current computed angle every rendered
frame. Only the whole-leaf offset still reads the 4 staggered pose buckets.
This removes the former per-leaf 10 Hz flutter presentation bottleneck without
changing grass, fruit, world tick or the overall discrete-pose setting. It does
not eliminate low-frame-rate aliasing or secretly lower the target frequency.
At the user's request, the conditional warning, target-frequency summary,
frame-rate/sample readout and explanatory paragraphs were removed. This menu
keeps only adjustable controls and the curve preview; a GUI regression checks
that the removed summaries stay absent while all controls render once.

Former held-angle storage is reused for the phase/cell fields: response state
stays 112 bytes, with fractional phase at byte 96 and integer cells at 104.
Push constants grow 48→80 bytes. Regenerated GPU structs and GUI accessors are
build outputs from cargo check, never hand-edited. Particle/leaf vertex ABI and
audio dependencies are unchanged.

## Deterministic evidence

- Rust main 982 + library 4 tests, 2 ignored; persistence round-trips, actual
  GUI control labels and push/state ABI checks included.
- 14 Slang tests: frequency endpoints, decreasing/increasing response, amplitude
  curve, calm decay, sustained motion, independent leaves, integer clock wrap,
  extreme retuning bounds, zero-time retuning continuity, existing leaf optics.
- Equal normalized-time trajectories at 2/4 Hz: error 0. Normalized-time output
  spectral tests at target 1.5/3/6/24 Hz measured peaks 0.4219/0.8438/1.6875/6.75 Hz;
  RMS angle 0.184621 throughout. The spectrum scales correctly, but the strongest
  stochastic spectral peak is explicitly NOT the natural-frequency parameter.
- Maximum 24 Hz target, 60 vs 240 Hz integration: max angle error 0.00017691 rad.

## Runtime evidence and acceptance boundaries

Locked release hidden/muted smoke:
`target/re-flora-logs/re-flora-20260915-020749.028-80388.log`.
Actual settings logged base 1.8, scale 1, independent curve [0.05,1,0] and
`local_flutter=current_frame`; normal exit, failures=0, no ERROR/panic/VUID.
Existing multiple-butterfly-atlas warning remains. Runtime retained the local
PetalSonic limiter override; no dependency release or lockfile change is part of
this feature. No visible game auto-launched; subjective motion and performance
acceptance remain separate from correctness smoke.

Instrumented GPU validation also passed:
`target/re-flora-logs/re-flora-20260915-020829.505-82525.log`.
GPU target 2/4/24 Hz normalized trajectory errors: 0/0.00000580/0.00001286 rad;
zero-time retuning and full phase/pose lifetime remap were bit-preserving.
Sustained angle excursion 0.070170686 rad at wind 0.3; calm residual 0.00000002.
Existing whole-vegetation cadence/reversal tests passed. Shutdown failures=0,
no ERROR/panic/VUID; the instrumented run logged a 137.783 ms physics hitch.
No performance acceptance claim is made from that diagnostic workload.

## Direct-editor validation (2026-09-15)

- fmt/check, 985 main + 4 library tests, 2 ignored; 14 Slang tests passed.
- Actual egui pointer press/drag/release moved a curve handle; normal Save/load
  retained both edited coordinates. Rising/falling shape handles tested.
- Legacy file load, migration idempotence and every unrelated parameter's exact
  schema/value preservation tested. A 401-point shader comparison confirms the
  saved 1.5/2/Bias 0.8 curve is unchanged after converting to 1.5/3/Scaling 1.
- Worktree GUI text outside frequency declarations compared identical to the
  user's just-saved file. Their other settings remain outside the commit.
- Locked release hidden/muted GPU validation:
  `target/re-flora-logs/re-flora-20260915-234839.505-21526.log`.
  Actual migrated frequency payload `[1.5,3,1,0]`, curve `[0.05,1,0.8,0]`;
  saved amplitude thresholds `[0.5,1.5,0,0]` preserved. GPU sweep/remap passed,
  shutdown failures=0, no ERROR/panic/VUID. Existing atlas warning and diagnostic
  physics hitch 138.241 ms remain; no performance claim.
- GUI bindings regenerated by cargo check. State/vertex/push-constant byte
  layouts did not grow; the frequency vector now means low/high/overall scale.
- No visible restart, merge, push, or audio release. Manual curve-editing feel
  and dynamic appearance remain for the user's try-out.
- Ordinary locked release hidden/muted smoke also passed, failures=0 and only
  the existing atlas warning:
  `target/re-flora-logs/re-flora-20260915-234944.065-25281.log`.
