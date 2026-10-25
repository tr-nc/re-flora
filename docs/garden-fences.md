# Voxel garden fences

Use **Thin pole** or **Thick pole** in the toolbar to place an upright post on
soil/rock. Each placement has a stable new ID and deterministic variation in
height (48–64 voxels), radius (thin 2–3 / thick 4–5), taper and small irregularities.
Posts are actual Oak Wood terrain, not render-only props.

Select **Rope**, click one post, then another. Continue clicking posts to extend
an arbitrary chain; click the current last post again to finish/reset the chain.
Connections are not limited to a fence prefab or grid direction. A sagging,
face-connected voxel rope joins below the caps. Branches/loops can be made by
starting another chain at an existing post. Endpoints are currently at a fixed
height below each cap, not separately adjustable by click height.

Rope geometry is ordinary editable Oak Wood terrain too. It is deliberately not
a tension/cloth simulation: removing a post does **not** make existing rope
terrain fall or disappear. It remains available to the normal terrain editor.
New links reject missing/edited-away caps. Post identity metadata is retired when
a cap is removed, without reusing its ID. Invalidating edits mark only intersected
caps; one batched byte read revalidates those caps. Idle frames do no GPU work for
fences. Placement drains pending cap validation before creating a new identity.

The existing terrain snapshot stores actual geometry; a backward-compatible
optional `fences` field stores post recipes and the monotonic ID counter. Older
saves default to no registered posts, and loading clears any unfinished click
chain. All persisted recipes/coordinates/IDs are validated before restoration.
There is a 1,024-post metadata cap. Geometry and material are generated in Rust;
no external model, image or generation service was used.

## Validation

- Three focused tests: variable recipes and JSON round-trip/invalid IDs,
  local cap invalidation/extreme-coordinate rejection, sagged bounded and
  face-connected ropes. Terrain persistence and text-toolbar suites pass.
- `cargo check`, formatting and Release build pass.
- Native fixture: three posts (1,365 / 3,408 / 1,365 authored cells), two
  non-collinear links (91 and 112 sampled rope cells, including join cells).
- Actual terrain save/reload preserves all sampled rope cells and three post
  identities. Removing a cap rejects a new connection, retires that post and
  leaves `next_id=4` rather than reusing its ID.
- Inspected `target/garden-fence.png`. Final placement/load/edit logs contain no
  ERROR, panic or VUID. Initial screenshot used walk mode and was reframed with
  an unsaved free-flight review camera; this is not a gameplay camera change.
- Physics/lighting use the ordinary terrain pipeline. No manual walking or
  subjective art approval is claimed. Dense construction performance is not
  benchmarked; long links perform a bounded regional clearance read on click.

```sh
RE_FLORA_FENCE_REVIEW=1 cargo run --release -- --hidden --mute \
  --terrain-save target/garden-fence.rflterrain \
  --screenshot garden-arrival target/garden-fence.png --screenshot-delay 2 --auto-exit 3
RE_FLORA_FENCE_REVIEW=load cargo run --release -- --hidden --mute \
  --terrain-load target/garden-fence.rflterrain --auto-exit 1
RE_FLORA_FENCE_REVIEW=edit cargo run --release -- --hidden --mute --auto-exit 1
```

Use `env -u WAYLAND_DISPLAY` on this machine if the existing Wayland portal times
out. Review hooks are unsaved; explicit `--terrain-save` owns the test file.
