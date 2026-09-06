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
- **C Gusts:** B plus finite directional packets or outward-moving radial rings.
- **D Local detail:** C plus evolving, spatially correlated vector noise.

In orbit/edit camera mode, enable **WIND DEMO**. Press on terrain, drag to aim,
and release to emit one directional gust. Drag distance does not control strength.
Choose **Radial click** to emit an outward-moving ring instead. Right-click or
Escape cancels an unfinished drag; ordinary camera rotation remains available.
Choosing a normal placement tool disables the demo tool. Releases consumed by
the settings UI cancel the preview instead of editing terrain or emitting wind.

The compass is world-plane oriented, not camera oriented. Blue circles are
approximate event footprints on a fixed horizontal reference plane, not a
terrain-following visualization or sampled vector field.

**Hold new wind** pauses this field's clock only; vegetation inertia can continue
settling. **Restart wind** resets the field, not the vegetation solver. The original
sampler in A retains its original clock. Automatic gusts use a fixed mathematical
sequence with frame-driven emission, not a cross-frame-rate replay guarantee.

## Ownership and limits

- `wind_field` owns field evolution, finite gusts, and a bounded GPU snapshot.
- The native demo adapter owns pointer gestures, preview, and temporary controls.
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
- Rust: 4 build-helper tests and 877 application tests passed, with 1 ignored
  test and the existing known fixture exception
  `patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof` explicitly
  skipped. This is not a claim that the unfiltered suite passes.
- Release hidden runs passed with the prototype enabled and disabled. The
  enabled screenshot was visually inspected for panel placement and rendering.
- GPU directional/radial/expiration checks passed; runtime event-expiry smoke
  passed. Run logs reported successful shutdown without errors.
- Saved GUI and camera files retained their pre-run checksums.

The GPU validation path exercises directional support, radial directions, a finite
radial center, and zero force after expiration using the actual shader sampler:

```sh
RUST_LOG=info RE_FLORA_WIND_PROTOTYPE=1 RE_FLORA_WIND_PROTOTYPE_SMOKE=1 \
RE_FLORA_VEGETATION_RESPONSE_VALIDATE=1 cargo run --release -- \
  --hidden --mute --authored-flora-bench
```

The scripted native smoke also verifies that directional and radial events are
removed from the runtime after expiration. This does not synthesize pointer
gestures. Manual acceptance of aiming, camera coexistence, and the appearance of
wind is still required. Performance acceptance is a separate release-mode stage
after visual approval; no performance improvement is claimed.
