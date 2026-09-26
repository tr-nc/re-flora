# Garden Tree Publication

`GardenTrees::{place, replace, replace_batch, remove}` owns one canonical tree
publication. Wood, foliage, fruit, shadow history and canopy audio commit together;
observer failure compensates the already-published geometry and observers before
leaving the previous canonical record authoritative.

## Continuous mesh cutover

Trees are no longer authored into terrain voxels. `Tree` still owns deterministic
branches, age-dependent geometry and attachment identities. `tree_gen::mesh::WoodMesh`
compiles a continuous indexed wood surface directly from that description. A child
extrudes a parent quad, reusing its four indices; forks are welded rather than a
collection of intersecting capped cylinders. Shared vertices have one skin binding.

The four-sided swept surface is an initial faceted art candidate. Authored centreline
subdivision remains visible; a connected topology does not prove freedom from
self-intersection under arbitrary authored parameters or extreme wind. Visual review
of junction proportions remains necessary.

`RasterTreeMesh` converts local generator units to world space and assigns the tree
identity to bindings. The same posed surface supplies color, sun shadows, software
triangle queries and exact character/rigid-body collision. Disabling wind retains a
static query/collision mesh; it does not restore terrain voxels. Leaf and fruit
attachments retain their existing published bone transforms.

The old occupancy extraction, tree-cell lookup, uncertain voxel-normal lighting,
rest-voxel query exclusions, tree voxel stamp/erase transactions and shovel
rest-coordinate editing path are retired. Terrain stays voxel-editable; a nearer
wood surface blocks the terrain brush. Whole-tree removal continues through
`GardenTrees`. Ground-height queries deliberately exclude tree geometry.

## Publication protocol

Preparation supplies the target records plus retained trees from the canonical owner.
The physical transaction contains complete before/after mesh-scene descriptions. Its
single execution path compiles the desired scene, completes previous GPU readers,
publishes resident geometry, bindings, attachments and collision, then publishes the
observer actions. Only after all actions succeed are the records canonical. Restoration
runs the same path with the previous scene; restoration failures propagate too.

Retained trees no longer need a chunk-overlap ownership index or clear-and-restamp
union. Age replacement does not publish a terrain revision or rebuild terrain chunks.
The resident mesh is currently rebuilt as a scene on topology changes, not incrementally
per tree. Ordinary wind only updates poses, surface positions and acceleration bounds.
GPU resource constraints remain checked explicitly by the existing scene allocator.

The branch hierarchy, GPU pose solver, skinning, refit and physical collision foundation
are retained; removing voxel rendering does not mean deleting these shared capabilities.
Wood irradiance is now evaluated spatially per mesh vertex and shared by display modes,
not once at the centre of an entire tree. Current-frame scene queries continue to include
the mesh for DDGI and local-light visibility.

## Current validation and remaining work

The continuous-mesh step passed formatting, check, **1110 binary tests / 4 ignored**
plus 4 library tests, hidden Release startup, and the 160-frame tree smoke with resize.
The smoke checks GPU pose, positions/normals, complete refit, exact CPU/GPU rays, wind
on/off, local-light data, age, deletion/replacement and unchanged terrain revision.
The default mature source contains 7,768 vertices / 15,532 triangles. These are geometry
counts, not performance acceptance. `target/tree-mesh-A.png` is a hidden bare-wood closeup;
no visible game was launched.

The next step adds a saved normal-mesh/pixel-display comparison using **the current
camera**, with no small-model surface bank or discrete view angles. Both modes must
consume this one geometry/pose/lighting publication and leave simulation unchanged.

Manual appearance, dense-scene performance and full environment integration remain
unaccepted. In particular water's existing terrain SDF and acoustic terrain snapshots
are separate consumers: their former static voxel wood no longer exists. They need an
explicit mesh-consumer policy/validation, not an invisible old voxel tree retained as a
workaround. Legacy terrain archives containing already-baked tree wood also need a
migration policy; this change does not silently erase ambiguous saved wood materials.
