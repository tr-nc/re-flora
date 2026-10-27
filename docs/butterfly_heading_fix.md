# Butterfly heading follows completed motion

## Impact and reproduction

Airborne flower visitors could face to the side of, or opposite to, their actual travel.
The real `ButterflyEmitter` → guided integrator → `ParticleSystem` snapshot test reproduced
this without wind, at tick 27 in `Approach`: model-forward/travel dot product `-0.3599654`
(about 111 degrees). This is not merely an apparent turn caused by display frame holding.

Red/green command:

```sh
cargo test --bin re-flora butterfly_foraging_is_always_active_and_flight_stays_integrated
```

Ordinary unguided/no-target heading already used velocity. A butterfly resting on a moving
flower intentionally follows the flower surface rather than the displacement of that flower;
that is not a flight-heading error and remains unchanged.

## Confirmed cause

The visitor behavior independently smoothed `goal - position` into a heading and published
that explicit body pose **before** the flight integrator computed the next velocity. The
renderer correctly honored that body override, so inertia, braking, orbit steering and wind
could move the animal in a different direction. `Takeoff`/`Recover` returned before updating
the goal-based heading, allowing the stale orientation to persist there too.

The model's front is local **-Z**. Departure and recovery additionally used local +Z to choose
their escape target. The landing surface basis, in contrast, intentionally stores local +Z;
its yaw extraction is preserved (and renamed `model_back` for clarity).

Position, heading and animation already share one publication clock. No separate render
clock or model-axis conversion defect was needed to reproduce this failure.

## History

`cbb70580` (2026-10-08, `feat: add saved A/B butterfly flower visiting behavior`) introduced
the airborne behavior-body override before integration. This identifies the source change;
no release-by-release bisection was performed to identify the first shipped manifestation.

## Fix and ownership

- After each completed guided integration step, observe the resulting velocity and wingbeat
  bank, then publish the visitor's body pose. Navigation remains responsible for goals,
  not a second airborne heading. All airborne visitor phases use the same velocity-derived
  model-forward convention as free flight.
- Keep the last valid orientation when velocity is zero/nonfinite, rather than inventing
  a target direction at rest. The integrator's existing finite-input guards are unchanged.
- Record the current surface orientation while feeding and depart/recover toward local -Z.
  Landing yaw, flower attachment, relaxed wings, identity, settings persistence and
  animation/publication cadence remain intact. No renderer-only compensating rotation.

## Validation

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed.
- `cargo test --bin re-flora butterfly`: **58 passed**. The strengthened real-emitter test
  checks actual displacement through complete visits in calm/moderate wind. A render-stream
  regression runs the real emitter, frame publication and serialized model instances at
  8/60 presentation FPS with wingbeat coupling both off/on. Additional tests cover a goal
  behind the current velocity, zero-speed hold, reversal, and departure/recovery forward axes.
- Existing feeding/sway, host replacement, fixed-step partition and population tests pass.
- Full `cargo test`: 1714 app tests passed, 13 ignored, and the same 9 previously established
  baseline failures remain (two GUI-default assertions and seven raster-tree fixtures).
  The four collision-bench tests passed. The unfiltered suite is not claimed green.
- Hidden muted Release smoke (`--auto-exit 0.5`) passed.
- Native production fixture: `RUST_LOG=re_flora=info,re_flora_vkn=warn
  RE_FLORA_BUTTERFLY_FORAGING_REVIEW=visits cargo run --release -- --hidden --mute --auto-exit 8`.
  Log confirms `complete_cycle=true distinct_hosts=true`, shutdown `failures=0`, no errors/warnings.
- No shader, generated file, model asset or saved config changes. No visible window was
  automatically launched; subjective turn aesthetics remain available for user review.
