# Night flower lighting

The existing `forget-me-not` is the stable second authored model. Its petal UV
island emits cool palette-colored radiance after sunset, reaching full emission
at sun altitude -0.4. Calyx and stem are not emissive; all other species remain
unchanged. Both selected cached-angle and ordinary mesh paths use the same rule.

`src/app/core/night_garden.rs` supplies at most two point lights for 64-voxel
spatial clusters within two world units of the camera. This is a deliberately
bounded approximation of the small plant heads. All flower petals still glow,
but not all flowers receive an independent environment light. Sources use stable
spatial IDs in provider 4 and enter the existing eight-light direct/DDGI budget
after authored/terrain/raster providers. No extra texture/mesh or offline image
was imported. Ordinary palette changes remain live.

Updates run at 2 Hz; identical sources publish nothing. Twilight intensity uses
eight intervals to avoid restarting multi-frame indirect transport every frame;
visible emission is smooth. Removal, day transition or disabled flora clears
only this provider's sources. Light registry state is derived, not saved.

## Validation

- `cargo test night_garden`: three tests (day/twilight/night, only nearby blue
  flowers, stable bounded selection, unchanged revision, removal and ownership).
- `cargo check`, `cargo fmt --check`, `cargo build --release` pass.
- Hidden native screenshots: `target/garden-blue-night.png` (inspected: emissive
  flowers plus soft ground pool), `target/garden-blue-day.png`.
- Hidden muted smoke and review logs: no ERROR, panic or VUID found.
- Eight-second Release runs with two planted flowers/one light, frames >120:
  lights disabled 17.07 ms median / 17.84 ms p95 (448 logged frames);
  enabled 17.03 / 17.88 ms (452 logged frames). These historical `[PERF][FRAME]`
  records are conditional (every 30th frame, total >=16 ms, or queue work >=2 ms),
  so their distribution is **not all frames**. They do not isolate GPU lighting cost.
  Enabled run published exactly once, not each frame.
- Final-runtime repetition with `RE_FLORA_GARDEN_PERF=1` logs every frame. Same
  8-second fixture, frames >120: off median/p95 17.10/17.66 ms (471 samples), on
  17.10/17.88 ms (463). Maximums were 23.81/56.50 ms, at early frames 122/125 with
  pending terrain/collider work; the enabled outlier had 43.43 ms untracked CPU.
  Its cause is not isolated. This pair is not a dense-garden, 60-FPS, isolated-GPU
  or statistical-significance acceptance claim.

Reproduce with `RE_FLORA_GARDEN_PLANT_REVIEW=forget-me-not
RE_FLORA_NIGHT_REVIEW=flowers RE_FLORA_GARDEN_PERF=1 cargo run --release -- --hidden --mute --perf
--auto-exit 8` (environment assignments on one shell command). For the diagnostic
baseline, add `RE_FLORA_NIGHT_FLOWER_LIGHTS_OFF=1`. This opt-in test switch does
not save settings. Screenshots use `--screenshot garden-arrival <path>
--screenshot-delay 2 --auto-exit 3`; omit the night variable for daytime.
