# Local leaf flutter frequency

Implements the bounded tuning proposal in `leaf_flutter_frequency_research.md`.
No claim of a universal botanical frequency/wind law.

## Controls and defaults

Debug → Flora → Leaves → Wind Motion → Flutter Frequency:

- Base Flutter Frequency (Hz): 0.5–12, default 1.8. This is the mechanical natural
  frequency/time scale, not a metronome or a promise of that many visible cycles.
- Strong-Wind Frequency Scale: 0.5–2, default 1. The strong-wind endpoint in Hz is
  displayed alongside the baseline. Below 1 slows, above 1 speeds up; 1 is flat.
- Nested Frequency Wind Curve: Start 0.05, Full 1, Bias 0 by default. Independent
  saved parameters, smootherstep preview in Hz; wind strengths are game units.
  The curve plateaus at the endpoint, while the amplitude curve gates excitation.

All five settings use the declarative saved-config path. User changes to audio,
flutter amplitude and its existing wind curve remain untouched and unstaged.

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
not eliminate low-frame-rate aliasing: the UI displays approximate samples per
target cycle without secretly lowering the target frequency. The conditional
warning was removed at the user's request: crossing its threshold with normal
frame-rate variation repeatedly inserted/removed text. The numerical readout
and controls remain; a GUI regression checks absence at several frame rates.

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
