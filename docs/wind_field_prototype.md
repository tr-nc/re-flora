# Native wind-field prototype

Isolated visual experiment, not a production wind redesign. The existing saved
vegetation response and individual-leaf mechanics remain unchanged. Main-worktree
terrain-material experiments are not included in this branch.

## Try it

From this prototype checkout, run:

```sh
RE_FLORA_WIND_PROTOTYPE=1 cargo run
```

Without the environment flag, the original wind sampler remains active. Prototype
controls are temporary and are not serialized into the saved GUI configuration.

- **A Original:** existing saved wind layers, without automatic turning.
- **B Turning:** mean wind, smooth direction wandering, and transported noise.
- **C Auto gusts:** B plus automatically generated directional wind bands.
- **D Local detail:** C plus evolving, spatially correlated vector noise.

These modes control the background only. Manual local gusts work in all four
modes, including A's original saved wind. Switching to A/B hides automatic
gusts, but does not cancel released manual gusts or deselect the Wind item.

In orbit/edit camera mode, select the **Wind** item in the bottom toolbar, or
press **9**. There is no separate WIND DEMO enable switch. Press on terrain, drag to aim,
and release to emit one directional gust. Drag distance does not control strength.
Choose **Radial click** to emit an outward-moving ring instead. Right-click or
Escape cancels an unfinished drag; ordinary camera rotation remains available.
Selecting Wind again returns to Hand. Choosing another item cancels aiming;
selection is owned by the existing player-tool runtime, with no second active flag.
The Wind item's settings appear only while it is selected, and are independent
of the automatic gust settings in the background panel. Releases consumed by
the settings UI cancel the preview instead of editing terrain or emitting wind.

Directional gusts are rectangular wind bands, four times wider across the wind
than along it. Strength falls smoothly from the center toward both side edges,
and toward the front/back. The size control scales both dimensions together;
there is no shape menu. Radial gusts retain their outward-moving ring behavior.

The compass is world-plane oriented, not camera oriented. Blue bands and circles
show event footprints on a fixed horizontal reference plane, not terrain-following
flow. Nested band fills indicate stronger wind near the center; they are not
separate gusts or an exact sampled vector-field visualization. During aiming, the
band preview is drawn at the initial terrain-hit height.

**Hold new wind** pauses this field's clock only; vegetation inertia can continue
settling. **Restart wind** resets the field, not the vegetation solver. The original
sampler in A retains its original clock. Automatic gusts use a fixed mathematical
sequence with frame-driven emission, not a cross-frame-rate replay guarantee.

## Ownership and limits

- `wind_field` owns field evolution, finite gusts, and a bounded GPU snapshot.
- The existing player-tool runtime owns the mutually exclusive Wind selection.
  The native demo adapter owns aiming gestures, preview, and temporary controls.
- The shared procedural wind sampler supplies the new field to existing wind
  consumers; it does not depend on demo input or terrain editing.
- At most 16 gusts exist simultaneously. Events snapshot their parameters on
  emission and expire automatically. Repeated gestures emit new independent events.
- Wind remains horizontal and height-independent. No terrain obstruction, CFD,
  or divergence-free guarantee is claimed. Audio and free-particle motion are not
  newly integrated with this field.
- Direction changes accumulate displacement rather than re-evaluating all past
  travel using the new heading. This does not change the existing discrete plant
  presentation cadence.
- The new field snapshot is held across vegetation substeps within a rendered
  frame. Frame-rate independence of this authoring prototype is not established.

## Validation and remaining acceptance

Validated on this branch:

- Formatting and `cargo check` passed; shader-derived structs regenerated.
- All 332 Python script tests passed.
- Rust: 4 build-helper tests and 881 application tests passed, with 1 ignored
  test and the existing known fixture exception
  `patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof` explicitly
  skipped. This is not a claim that the unfiltered suite passes.
- Release hidden runs passed with the prototype enabled and disabled. The
  enabled screenshot was visually inspected for panel placement and rendering.
- GPU directional/radial/expiration checks passed; runtime event-expiry smoke
  passed. Run logs reported successful shutdown without errors.
- The directional-band revision additionally passed GPU checks for the 4:1
  aspect ratio, monotonic center-to-side falloff, matching left/right response,
  zero force beyond either edge, and nonzero force inside rectangular corners.
- The toolbar revision passed GPU checks for identical isolated manual-gust
  response in A/B/C/D, including the original sampler path. CPU checks cover
  mutually exclusive tool selection, stopping terrain strokes, independent
  manual/automatic settings, and mode changes retaining manual events.
- Saved GUI and camera files retained their pre-run checksums.

The GPU validation path exercises directional support, radial directions, a finite
radial center, and zero force after expiration using the actual shader sampler:

```sh
RUST_LOG=info RE_FLORA_WIND_PROTOTYPE=1 RE_FLORA_WIND_PROTOTYPE_SMOKE=1 \
RE_FLORA_VEGETATION_RESPONSE_VALIDATE=1 cargo run --release -- \
  --hidden --mute --authored-flora-bench
```

The scripted native smoke selects the actual Wind toolbar item and verifies that
manual directional and radial events expire even while automatic gusts continue.
This does not synthesize pointer
gestures. Manual acceptance of aiming, camera coexistence, and the appearance of
wind is still required. Performance acceptance is a separate release-mode stage
after visual approval; no performance improvement is claimed.
