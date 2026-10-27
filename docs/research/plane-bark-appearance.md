# London plane bark appearance

## Botanical reference

NC State Extension's [London plane entry](https://plants.ces.ncsu.edu/plants/platanus-x-acerifolia/)
describes light brown outer bark exfoliating to reveal creamy olive inner bark.
This supports adjacent warm grey/brown, olive and cream pigments, not arbitrary dark patches.
The source does not specify a physical peeling depth; no depth measurement is inferred here.

## Rendering decision

The previous fragment shader independently hashed 4 × 7 × 4 voxel cells into three
pigments (sRGB 0.76/0.75/0.60, 0.53/0.57/0.39 and 0.39/0.37/0.29). Its darkest pigment
was far darker than the cream, so a material boundary could resemble a lighting discontinuity.
This is an art/material issue; it does not establish a defect in irradiance computation.

Use contiguous smooth-noise regions with warped boundaries and a narrower warm palette:
0.65/0.63/0.53, 0.69/0.70/0.59 and 0.76/0.74/0.64 (sRGB). A 3 × 5 × 3 voxel noise lattice
keeps several peeling regions visible on the reduced native tree size, without independently
coloring each cell. Keep rest-space, per-cell pigment sampling and the existing lighting path.

Do not remove or extrude whole voxels to represent a thin peeling layer. At this art scale,
that would change the trunk silhouette into pits/ridges rather than subtle bark relief.
This is a rendering choice, not a claim that real bark is perfectly flat.

## Validation

- `cargo check` regenerates and compiles shader-derived output.
- Release native capture fixture: `RE_FLORA_TREE_SPECIES_REVIEW=plane`, same seed/default
  camera, `--hidden --mute --screenshot player-default target/plane-bark-after.png
  --screenshot-delay 3 --auto-exit 4`.
- Before/after captures remain untracked under `target/`; no external image assets imported.
- Visual acceptance remains with the user; no performance improvement is claimed.
