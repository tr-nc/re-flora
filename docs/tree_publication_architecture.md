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

## Runtime display-only A/B

Debug → **Tree Rendering** provides saved declarative controls:

- `tree_pixelized`: **Pixelized Wood (B; Unchecked = Normal Mesh)**, default **false**.
- `tree_pixel_size`: **Wood Pixel Size (Scene Pixels; Current Camera)**, default **4**,
  range **1–16**, enabled in B. These are internal scene pixels, before output scaling.
- `tree_wind` and `tree_stiffness` remain independent. The old wind value migrates;
  `raster_tree_static` and `raster_tree_hybrid_lighting` are removed on load/save.

A draws the posed indexed mesh. B samples that same GPU mesh with the **current
perspective camera** and refit BVH each frame. Center rays retain their original
surface; empty centers use the common model-pixel clipped-triangle coverage rule to
keep fine twigs visible. There is no pre-baked view, fixed camera-angle list or rigid
whole-tree surface bank. The fragment pass writes the sampled scene depth into the
ordinary terrain/foliage/fruit depth attachment; it is not a whole-screen image filter.

Both modes interpolate the same per-vertex irradiance and share `shadeTreeWood`.
Only wood display changes. Shadows, queries, collision, attachments, leaf regeneration
and fruit state do not inspect the display switch. The current pixel grid is screen-
aligned, not world-locked: camera motion can produce pixel crawl. Large blocks expand
thin silhouettes conservatively and use one surface depth per cell. These are visible
art tradeoffs to review, not claims of exact per-fragment equivalence to A.

Storage reuses the common fence-slot allocator. Oversized frames split into contiguous
row bands under the portable storage-buffer budget; every band draws that frame.
No tree is rejected to satisfy a tile allocation. No production CPU pose/pixel readback
is added. Smoke-only readback explicitly uses transfer-capable storage.

## Current validation and remaining work

The display step passed formatting, check, **1113 binary tests / 4 ignored** plus
4 library tests, and the 160-frame hidden Release tree smoke with resize. This exercises
GPU poses/normals/refit, exact CPU/GPU rays, wind on/off, local-light data, age,
deletion/replacement, unchanged terrain revision, A→B→A, camera rotation, pixel sizes
4/1/16 and four GPU color/depth readbacks (89 B frames). No Vulkan errors remain.
Initial unsupported fragment-demote / draw-parameters declarations were removed by
using the existing transparent/far-depth contract and `SV_VulkanVertexID`; readback
buffers now explicitly declare transfer-source use.

The shared leaf native coverage/depth/RGBA oracle passed after extraction of the common
single-triangle coverage helper. The full leaf replay also passed with pixelized wood:
14,572 attached → stripped → regrown → stripped; 29,144 released leaves coexist,
including complete release-pose and attached-rotation checks.

The default mature source contains 7,768 vertices / 15,532 triangles. These are geometry
counts, not performance acceptance. Hidden bare-wood closeups are `target/tree-mesh-A.png`,
`target/tree-mesh-B.png` (4 scene pixels) and `target/tree-mesh-B16.png`. No visible game
was launched. These captures do not substitute for manual visual approval.

Manual appearance, dense-scene performance and full environment integration remain
unaccepted. In particular water's existing terrain SDF and acoustic terrain snapshots
are separate consumers: their former static voxel wood no longer exists. They need an
explicit mesh-consumer policy/validation, not an invisible old voxel tree retained as a
workaround. Legacy terrain archives containing already-baked tree wood also need a
migration policy; this change does not silently erase ambiguous saved wood materials.
