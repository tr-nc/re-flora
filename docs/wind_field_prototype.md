# Native wind-field prototype

Isolated visual experiment, not a production wind redesign. Saved vegetation
response and individual-leaf mechanics are unchanged. Main-worktree terrain
material experiments are not included in this branch.

## Try it

From this checkout:

```sh
RE_FLORA_WIND_PROTOTYPE=1 cargo run
```

Without the flag, the original wind sampler is active and no Wind item appears.
Temporary wind controls are not saved into the GUI configuration.
They now live in the main debug panel under **Wind → Background Wind** and
**Wind → Wind Item**, alongside Vegetation Wind Response. There are no separate
background-wind or wind-item windows. Collapsing settings does not hide the
world-space aiming/released-gust outlines or cancel a gesture. The global Main
direction slider and compass have been removed.

Inflow modes are **Saved inflow**, **Turning inflow**, and **Detailed inflow**.
They choose the inputs at scene edges, not different global samplers. Saved
sources no longer overwrite direction throughout the scene when adjusted.
No mode automatically releases gusts. There are no Hold, Clear, or Restart
operations. Explicitly released gusts expire naturally.

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
- Rusanov fluxes and CFL-bounded substeps provide local transport. Drag and a
  velocity cap keep this pressureless, art-directed model bounded. This is not
  a physically calibrated air simulation or a pressure/incompressibility solve.
- The grid is deliberately coarse (about 16.5 voxels per sample in this world).
  It smooths fine source details; the preview describes the emitter, not exact
  instantaneous force bounds. Residual wind advects and decays after an emitter
  expires, rather than being deleted everywhere at once.
- At most 16 gusts exist at once. No pause/reset/automatic-emission lifecycle exists.
- No CFD, terrain obstruction, or divergence-free guarantee is claimed.
- Audio and free-particle motion are not newly integrated.
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
RUST_LOG=info RE_FLORA_WIND_PROTOTYPE=1 RE_FLORA_WIND_PROTOTYPE_SMOKE=1 \
RE_FLORA_VEGETATION_RESPONSE_VALIDATE=1 cargo run --release -- \
  --hidden --mute --authored-flora-bench
```

The native smoke selects the actual Wind item and emits two scripted events.
It does not synthesize mouse gestures. CPU regressions exercise the actual
gesture conversion and display-center helper: release retains clicked height,
the one-second preview matches drag length and multiplier, and modes never emit
events themselves. The height test was observed failing with the old fixed-height
display before the fix.

For the Rust suite, skip only the pre-existing unrelated fixture
`patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof`.
Manual aiming/camera/appearance acceptance still requires a user try-out.
