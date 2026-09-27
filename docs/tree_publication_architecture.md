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
surface; empty centers use common clipped-triangle coverage to keep fine twigs
visible. Live wood chooses the covered footprint point nearest the cell center,
then resolves depth ties. Its material uses perspective-correct barycentrics at
that point, not the source triangle centroid. Rigid small-model adapters retain
their existing coverage/material contract. There is no pre-baked view, fixed camera-angle list or rigid
whole-tree surface bank. The fragment pass writes the sampled scene depth into the
ordinary terrain/foliage/fruit depth attachment; it is not a whole-screen image filter.

Both modes interpolate the same per-vertex irradiance and share `shadeTreeWood`.
Only wood display changes. Shadows, queries, collision, attachments, leaf regeneration
and fruit state do not inspect the display switch. The current pixel grid is screen-
aligned, not world-locked. Large blocks expand thin silhouettes conservatively and
use one surface depth per cell; they are not exactly equivalent to A per fragment.
**No camera-motion flicker remains the acceptance requirement.** Screen alignment
is not an excuse for flashing edges, and static screenshots are not temporal acceptance.

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

### Camera-motion regression

The first implementation had two distinct continuous-surface sampling errors:

1. Center rays shaded the hit point, but coverage-only cells shaded the triangle
   centroid. A 0.0001-radian camera yaw step could therefore make a continuously
   occupied cell jump by **0.9848914 linear RGB** on a fixed lighting gradient.
2. Correcting barycentrics alone was insufficient: selecting coverage by depth
   could switch to a different triangle's corner elsewhere in the same cell.
   A coplanar split-quad replay still jumped by **0.014998436** at a triangle boundary.

Coverage now chooses the nearest projected footprint point before comparing depth,
then shades that point in homogeneous coordinates. This handles perspective and
vertices behind the near plane without dividing original vertices by their `w`.
The corresponding post-fix steps were **0.00038339943** and **0.000893116**. These
are controlled continuity measurements, not universal perceptual/performance thresholds.
Neither temporal smoothing, frozen views nor a different camera-angle bank is used.

Run `scripts/check_tree_pixel_motion.sh` for the opt-in native regression. It drives
**the production compute shader**, with isolated geometry, camera, palette and light
buffers (no saved settings changes). Thirty-six cases each sweep 121 camera samples:
front/oblique triangles, a split quad and near-clipped geometry; yaw, pitch and lateral
translation; block sizes 1/4/16. Stationary repeats must be identical, motion must
actually change the sampled surface, and continuously covered cells must change by
less than 0.01 linear RGB per 0.0001-radian/world-unit step. Retriangulating the quad
must preserve coverage, color and depth. Colors must remain finite and depth in [0,1].
The helper rejects runtime errors and requires clean shutdown; it is not a unit test.

The replay specifically guards **continuous-surface material flashes**. It records
coverage transitions but does not prove temporal stability of every silhouette,
disocclusion, terrain intersection, moving branch or dense forest. Whole-tree motion
captures at `target/tree-motion-review/{A,B4,B16}.mp4` use the same static mesh and
scripted moving camera, with GUI/camera files restored. Each includes 32 color
keyframes from a 96-frame run. Their raw frame-difference metrics include legitimate
motion and must not be described as flicker scores or performance results. Manual
confirmation of the full no-flicker criterion remains outstanding.

Post-fix validation passed formatting, `cargo check`, **1113 binary tests / 4 ignored**
and 4 library tests, the 36-case native motion replay, the strict original leaf
coverage/depth/RGBA oracle, the 160-frame tree/resize smoke, and real-leaf lifecycle
replay with pixelized wood enabled (29,144 released leaves, full pose/rotation
continuity). Hidden Release startup/shutdown and all final run logs were clean.
No generated files or saved defaults changed. No performance conclusion is claimed.

For dense color keyframes through the existing real-scene camera-motion runner, set
`RE_FLORA_TREE_PIXEL_MOTION_REVIEW=1` with `--hidden --mute` and
`--denoiser-bench <snapshot> <report.toml> --denoiser-bench-camera-motion`.
This only changes capture retention, not rendering,
wind, lighting or saved controls. Freeze wind/daylight via the ordinary saved settings
for controlled review and restore those settings afterward.

### Reproducible review commands

```sh
cargo build --release
scripts/check_tree_pixel_motion.sh
python3 scripts/check_raster_tree_static.py --thin-branches
python3 scripts/benchmark_tree_update.py --output target/tree-mesh-update-bench \
  --seconds 6 --warmup-seconds 2
python3 scripts/check_tree_terrain_edit_perf.py target/tree-mesh-terrain-edit
```

The historical capture command now compares **only normal/pixelized new mesh display**;
its old `--hybrid-lighting` option is rejected. It restores GUI/camera bytes, checks zero
terrain writes and equal geometry fingerprints in both bare/canopy captures. Four actual
captures passed with fingerprint `120bb3e71e5c6ea8`; the source contains 1,259 branch
segments with radii below half a voxel, now represented directly as geometry. Results are
under `target/tree-display-evidence/`. Twenty-one focused script tests, Ruff and Pyright passed.

The repaired terrain-edit replay passed three real edits, one initial wood compile and
**zero** tree rebuilds during editing, with no runtime errors. This is not a test of water
colliding with wood. A short RTX 3060 Ti single-tree Release measurement (5120×2880 output,
ordinary display, fixed camera/daylight, 6 s each, first 2 s excluded) recorded:

| Wind | Frame median / p95 | CPU tree update median | GPU surface update median |
| --- | --- | --- | --- |
| Off | 17.16 / 18.05 ms | 0.410 ms | 0.064 ms |
| On | 17.16 / 18.19 ms | 0.684 ms | 0.065 ms |

There are 227/219 frame samples but only 8/8 GPU scope samples. This measures the **new**
mesh's wind toggle, not old/new rendering, not display A/B, and not a performance win.

Manual appearance, dense-scene performance and full environment integration remain
unaccepted. In particular water's existing terrain SDF and acoustic terrain snapshots
are separate consumers: their former static voxel wood no longer exists. They need an
explicit mesh-consumer policy/validation, not an invisible old voxel tree retained as a
workaround. Legacy terrain archives containing already-baked tree wood also need a
migration policy; this change does not silently erase ambiguous saved wood materials.
