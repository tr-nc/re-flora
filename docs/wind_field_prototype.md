# Native wind-field prototype

The shared field is the sole wind path, now integrated into main alongside the Glass
improvements and terrain/vegetation snapshots. Saved vegetation response and individual-leaf
mechanics are unchanged. The separate terrain-material candidate is not included. The
art-directed model and validation limits documented below still apply after integration.

## Try it

From this checkout:

```sh
cargo run
```

No opt-in flag or legacy sampler remains. The Wind item is always available.
Temporary wind controls are not saved into the GUI configuration.
They now live in the main debug panel under **Wind → Background Wind** and
**Wind → Wind Item**, alongside Vegetation Wind Response. There are no separate
background-wind or wind-item windows. Collapsing settings does not hide the
world-space aiming/released-gust outlines or cancel a gesture. The global Main
direction slider and compass have been removed.

There is one transported field with an inlet A/B experiment. **Background inflow**
enables its edge input. For A, **Local disturbance** continuously controls
crosswind detail (zero disables it).
The old Saved/Turning/Detailed selector, Gentle/Strong sources, source config,
source buffers and CPU/GPU procedural sampler have been removed.
Disabling background inflow stops new boundary input, not existing wind: the
field still transports and decays, and the Wind item can still inject local wind.
No automatic gust event objects, Hold, Clear, or Restart operations exist. Explicitly released
gusts expire naturally.

### Inlet A/B experiment

Under **Wind → Background Wind**, toggle **B: Natural wind (A/B experiment)**.
Unchecked A is the original inlet and remains the startup default. B adapts the
HTML v4/v5 inlet recipe: continuous small variations with occasional smooth,
spatially varying strengthening and direction changes. These are variations of
one inlet, not independently spawned gust objects.

B starts with strength `0.9`, small variations `0.4`, occasional strengthening
`0.4`, and strengthening area size `108` voxels. The mapping is 24 game voxels per HTML
demo unit, an artistic starting point rather than a physical calibration.
A and B keep their own inlet settings during the run. Transport speed is shared.
Switching retains the grid, simulation time, manual gusts, and plant state;
allow new inflow time to cross the garden before comparing. Settings are not saved.

This experiment changes only the inlet. It keeps the game's Rusanov transport,
drag, grid resolution, and vegetation response, rather than porting the HTML's
kinematic carrier. The game may therefore smooth or attenuate the variations
differently. The defaults still need visual comparison on actual plants; this
is not a performance acceptance or a claim of physical airflow accuracy.

### Spatial contrast follow-up

The original port scaled continuous breeze detail along with strengthening area
size. Increasing every slider made neighboring locations more alike. The B inlet
now keeps breeze detail at a 48-voxel base scale (about three transport cells in
the default world) independently of the **Strengthening area size** slider, with
more weight on continuous variations. The field still transports all input from
the boundary; there is no per-plant random wind or instantaneous interior rewrite.

An explicit deterministic diagnostic samples five points over a 128-voxel
crosswind span at the center of a 512-voxel field, once per second from 20–60
seconds. Its contrast is `(max speed - min speed) / max speed`, then the median
over those snapshots. Before the fix: default B 17.8%, all B sliders and transport
at maximum 14.0%. Reducing only area size to 48 raised the latter to 35.3%; reducing
only transport speed to 50 yielded 15.2%. After the fix: default B 24.2%, all-max
23.4%. This isolates inlet scale coupling as a contributor; it does not prove
perceptual improvement on plants or cover every camera, location, or world size.

```sh
cargo test --bin re-flora natural_wind_spatial_contrast -- --ignored --nocapture
```

This multi-second diagnostic stays outside the normal unit suite. A fast unit
guard checks that, with strengthening disabled, changing area size leaves the
continuous breeze unchanged. A remains unchanged for visual comparison.
Follow-up validation passed formatting, `cargo check`, the normal binary suite
(930 passed, 2 ignored), and the explicit spatial diagnostic. A six-second release
hidden B run passed the manual-gust lifecycle smoke and exited with zero shutdown
failures and no logged errors; the existing butterfly-atlas warning remains.

Select the bottom **Wind** item or press **9**. Press on terrain to set the origin,
drag to set direction and speed, and release once. In world units, the horizontal
drag distance multiplied by 256 and **Speed multiplier** gives voxels per second.
The preview arrow shows one second of travel, including the multiplier.
Selecting Wind again returns to Hand; selecting another item cancels aiming.
Escape/right-click cancels an unfinished gesture; right drag otherwise rotates
the camera. A release over UI cancels instead of planting or emitting wind.

The **Wind Item** subsection directly exposes **Width**, **Depth**, **Edge softness**, strength,
lifetime, and speed multiplier. Width/depth are full extents in voxels, not radii.
Softness sets how much of each half-extent fades to the edge: larger values give
a broader gradual fade; smaller values retain a wider strong center.
Defaults are 192 voxels wide and 48 deep; dimensions can be adjusted independently.
Only directional drag is supported. A click without dragging emits nothing;
the radial mode, ring preview and ring-force calculation have been removed.

Settings affect aiming and subsequent emissions; existing gusts retain their
settings. The band preview uses one unfilled outline, without shaded fill or
nested boxes. Softness still affects the actual wind. Both aiming and released footprints retain the clicked height.
The preview outlines the moving input region, not the entire downstream wake.
The field itself remains horizontal and does not vary with height; this is not
terrain-following airflow.

## Ownership and limits

- The player-tool runtime owns mutually exclusive Wind selection.
- The demo adapter owns gestures, speed mapping, controls, and preview.
- The wind field owns a persistent 32 by 32 horizontal vector grid, bounded
  event snapshots, and expiration. Scene dimensions come from the live world.
- Global inflow enters through edge fluxes; local emitters inject momentum only
  inside their moving band. Both evolve through the same finite-volume transport.
- The shared GPU sampler bilinearly reads that grid; it never separately adds
  global direction or manual wind. CPU and GPU share packed grid coordinates.
- Tree rustle reads the same published grid at its weighted canopy positions;
  fallen-leaf emission reads it at the emitter position. Leaf particle motion
  already uses the GPU wind-volume path, which now samples only this field.
- Rusanov fluxes and CFL-bounded substeps provide local transport. Drag and a
  velocity cap keep this pressureless, art-directed model bounded. This is not
  a physically calibrated air simulation or a pressure/incompressibility solve.
- The grid is deliberately coarse (about 16.5 voxels per sample in this world).
  It smooths fine source details; the preview describes the emitter, not exact
  instantaneous force bounds. Residual wind advects and decays after an emitter
  expires, rather than being deleted everywhere at once.
- At most 16 gusts exist at once. No pause/reset/automatic-emission lifecycle exists.
- No CFD, terrain obstruction, or divergence-free guarantee is claimed.
- The opt-in canopy-audio diagnostic uses an explicit uniform field snapshot
  for its controlled audio fixture; it does not retain a procedural wind source.
- A wind snapshot is held across vegetation substeps in one render frame;
  full frame-rate independence has not been established.
- Performance acceptance remains separate from visual review.

## Validation

The production-path GPU guard checks that local input enters the unified field,
leaves distant positions unchanged initially, and that GPU plant responses match
uniform reference fields populated from CPU grid samples. Deterministic tests
check near-before-far boundary propagation, no instant interior turn, finite
values, local injection and decay. Source geometry tests retain width/softness
coverage; strict zero force immediately outside an emitter is no longer the
contract because the shared field carries a wake.

```sh
RUST_LOG=info RE_FLORA_WIND_PROTOTYPE_SMOKE=1 \
RE_FLORA_VEGETATION_RESPONSE_VALIDATE=1 cargo run --release -- \
  --hidden --mute --authored-flora-bench
```

The native smoke selects the actual Wind item and emits two scripted events.
It does not synthesize mouse gestures. CPU regressions exercise the actual
gesture conversion and display-center helper: release retains clicked height,
the one-second preview matches drag length and multiplier, and background settings never emit
events themselves. The height test was observed failing with the old fixed-height
display before the fix.

Consumer regressions check weighted canopy sampling, spatially gated leaf
emission, published CPU/GPU coordinate agreement, background-off manual input,
and the disturbance slider's zero/nonzero behavior. The UI test checks that the
retired mode/source controls are absent.

For a hidden B startup and manual-gust lifecycle check:

```sh
RE_FLORA_WIND_AB_SMOKE=1 RE_FLORA_WIND_PROTOTYPE_SMOKE=1 \
  cargo run --release -- --hidden --mute --auto-exit 6
```

The B override logs `[WIND_AB] variant=B-natural`; it does not exercise clicking
the checkbox. Unit tests check state retention across A/B switches and continuous,
nonuniform B inlet variation, plus the shared field and background-off behavior.
The A/B implementation passed `cargo fmt --check`, `cargo check`, and
`cargo test --bin re-flora` (929 passed, 1 ignored). Release hidden runs completed
for default A (0.5 seconds) and B (6 seconds), with the B manual-gust smoke passing
and no logged errors. Both runs reported the existing multiple-butterfly-atlas
warning. These short runs are correctness checks, not performance measurements.
Manual aiming/camera/appearance acceptance still requires a user try-out.
