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

Background modes are **A Original**, **B Turning**, and **C Local detail**.
No mode automatically releases gusts. There are no Hold, Clear, or Restart
operations. Explicitly released gusts expire naturally.

Select the bottom **Wind** item or press **9**. Press on terrain to set the origin,
drag to set direction and speed, and release once. In world units, the horizontal
drag distance multiplied by 256 and **Speed multiplier** gives voxels per second.
The preview arrow shows one second of travel, including the multiplier.
Selecting Wind again returns to Hand; selecting another item cancels aiming.
Escape/right-click cancels an unfinished gesture; right drag otherwise rotates
the camera. A release over UI cancels instead of planting or emitting wind.

The Wind panel directly exposes **Width**, **Depth**, **Edge softness**, strength,
lifetime, and speed multiplier. Width/depth are full extents in voxels, not radii.
Softness sets how much of each half-extent fades to the edge: larger values give
a broader gradual fade; smaller values retain a wider strong center.
Defaults are 192 voxels wide and 48 deep; dimensions can be adjusted independently.
Radial click retains its ring mode, with ring thickness and a base speed because
a click has no drag length.

Settings affect aiming and subsequent emissions; existing gusts retain their
settings. The band preview uses one outline and a smoothly shaded mesh, not
nested boxes. Both aiming and released footprints retain the clicked height.
The field itself remains horizontal and does not vary with height; this is not
terrain-following airflow. The compass is world-oriented, not camera-oriented.

## Ownership and limits

- The player-tool runtime owns mutually exclusive Wind selection.
- The demo adapter owns gestures, speed mapping, controls, and preview.
- The wind field owns evolution, bounded event snapshots, and expiration.
- The shared GPU sampler combines background wind and explicitly released gusts.
- At most 16 gusts exist at once. No pause/reset/automatic-emission lifecycle exists.
- No CFD, terrain obstruction, or divergence-free guarantee is claimed.
- Audio and free-particle motion are not newly integrated.
- A wind snapshot is held across vegetation substeps in one render frame;
  full frame-rate independence has not been established.
- Performance acceptance remains separate from visual review.

## Validation

The production-path GPU guard checks directional/radial force, expiration,
background-mode independence, shape bounds, symmetric falloff, adjustable
softness, and independently adjustable width:

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
