# Wide-spray stripe investigation

## Requested change: 32 discrete poses

Commit `f4462cc4` doubles hardware poses from 16 to 32 (22.5° to 11.25°).
Native geometry, cached geometry/design indexing, and historical spray launch
orientation use the same count. Rust/Slang agreement is checked by the explicit
Slang executable regression. Cache tests now exercise every pose rather than
assuming sixteen poses per design. Spray angle, RPM, pulse timing, and water
quantity were not changed.

Validation: `cargo check`, 45 sprinkler tests including ignored tests,
`cargo build --release`, hidden/muted `rotary-design-switch` (seven steps), and
normal hidden/muted smoke passed. This is correctness evidence, not a performance
benchmark; doubling cached poses increases the sprinkler-specific cache workload.

## Impact scope

The user's screenshot shows conspicuous radial-looking wet/dry streaks despite a
reported 60° spray angle. Exact terrain, elapsed watering time, instance phase,
and runtime settings were not captured with the screenshot. The original scene
has **not** been reproduced. Do not interpret passing fixture assertions as a
regression test for that screenshot.

Three opt-in native diagnostics were added (none saves settings or terrain):

- `rotary-contact-wide`: flat soil, 60°, zero RPM, continuous valve, frame 180.
- `rotary-contact-wide-rotation`: flat soil, 60°, 5 RPM, production pulse frequency
  and 30% duty, frame 1800.
- `rotary-terraces-wide`: concentric voxel steps, 60°, zero RPM, continuous valve,
  frame 180.

Readback images of actual soil moisture showed a connected fan interior in the
stationary flat case, broadly filled interior after a revolution in the rotating
flat case, and stepped edges on the terrace case. They did **not** reproduce the
reported pronounced radial stripes. Random mottling and irregular outer edges
remain visible. Fixtures run with real frame timing; frame count is not a fixed
elapsed-time or pixel-identical comparison contract.

Measured flat readbacks:

| Fixture | Wet cells | Summed moisture | Active contacts at readback |
| --- | ---: | ---: | ---: |
| stationary | 4079 | 11117 | 8 |
| rotating | 22329 | 60321 | 4 |

These totals establish real GPU wetting, not absence of all possible stripes.

## Root cause

**Unconfirmed.** The flat experiments do not support claiming that 60° alone
leaves angular holes between 32 launch orientations. They also cannot distinguish
terrain-related contact behavior from settings or elapsed-time effects in the
original scene. No production wetting change was made on that basis.

## When introduced

Unknown. Neither an introducing commit nor a deterministic reproducer of the
reported symptom has been established. The pose-count change is a requested
behavior change, not a claimed stripe fix.

## Clean fix / next evidence

No stripe fix is claimed. Obtain the original terrain/save plus sprinkler
positions, runtime spray settings and watering duration, or a reproducible
placement recipe. Capture actual surface moisture in that scene before testing
individual causes. Do not widen contact footprints, blur soil moisture, or
change pulse timing merely to hide the screenshot's streaks.

Re-run diagnostics from this worktree:

```sh
RUST_LOG=info RE_FLORA_SPRINKLER_REVIEW=rotary-contact-wide \
  RE_FLORA_SPRINKLER_WET_STATS=/tmp/sprinkler-wide.json \
  cargo run --release -- --hidden --mute --auto-exit 6
RUST_LOG=info RE_FLORA_SPRINKLER_REVIEW=rotary-contact-wide-rotation \
  RE_FLORA_SPRINKLER_WET_STATS=/tmp/sprinkler-rotation.json \
  cargo run --release -- --hidden --mute --auto-exit 40
RUST_LOG=info RE_FLORA_SPRINKLER_REVIEW=rotary-terraces-wide \
  RE_FLORA_SPRINKLER_WET_STATS=/tmp/sprinkler-terraces-wide.json \
  cargo run --release -- --hidden --mute --auto-exit 6
```

Check the fixture completion marker and output file; slower machines may need a
longer auto-exit. Flat files contain a 321×321 surface in `voxels`; terrace files
contain a 185×185 `surface`. Moisture is `(byte >> 4) & 3`. Local untracked evidence
is retained under `target/sprinkler-stripes-validation/` (JSON, logs, grayscale
readback visualizations). The original rotating log predates the correction of a
hard-coded `frames=180` message; its readback actually occurs at frame 1800.
