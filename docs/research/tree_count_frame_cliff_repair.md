# Tree-count frame cliff repair

Worktree: `new-tree`. This records validated implementation stages, not official release notes.
The independent red reproduction and hardware/configuration are in
[`tree_count_frame_cliff_diagnosis.md`](tree_count_frame_cliff_diagnosis.md).

## World-space ownership

The independently reproduced `unbound GPU tree vertex` came from repeating ownership in translated
local floating-point space after extraction had already established ownership in world space.
The binder now uses the exact stamped world-space cones. A pinned float-boundary regression failed
before the change and passes after it; there is no tolerance expansion or arbitrary fallback bone.

## Complete demand-sized GPU publications

All tree buffers and their lookup tables are allocated from actual published demand, with device,
index, individual-publication (256 MiB), and old-plus-new logical peak (512 MiB) preflight. Prepare
validates bindings/BVH and fills a new resource set without mutating the resident publication.
Every consuming pipeline is selected by actual reflection over the complete pipeline collection;
all its descriptors are prepared/validated before a quiesced, infallible commit. Allocation errors
return rather than panicking. Typed mapped buffer writes now enforce allocation bounds.

Validation: `cargo fmt --check`, `cargo check`, tree/cache/layout/lookup tests (20), raster mesh tests
(11), pipeline topology tests (4), mapped-write bounds test, `cargo build --release`, hidden muted
0.5-second launch, and complete `--raster-tree-smoke --perf` pass. The smoke compares CPU/GPU wood
positions, normals/confidence, BVH, collision/query pose, hybrid lighting, wind on/off, stiffness,
age rebuild, posed terrain edit, deletion/replacement, and Garden Tree/Conifer.

Uncontended authoritative replay: `target/tree-scaling-capacity-03`, two runs of six size-16 trees
through the real placement owner, normal rendering/wind/shadows/DDGI enabled:

| Tree count | Run 1 median ms | Run 2 median ms |
| --- | ---: | ---: |
| 1 | 17.106 | 17.231 |
| 2 | 16.876 | 17.060 |
| 3 | 16.684 | 16.616 |
| 4 | 20.264 | 19.742 |
| 5 | 26.355 | 25.732 |
| 6 | 31.884 | 31.525 |

Zero preparation errors; no frame cliff. Before repair the same sixth-tree input stayed at
111.226 / 110.239 ms. Six trees now genuinely publish 146904 vertices and 101120 triangles; they
are not silently omitted. Normal CPU scaling is still present and is the next stage.

**Invalid intermediate experiments:** `tree-scaling-capacity-01` exposed an incorrectly inferred
f64 initialization palette and aborted at shutdown. `-02` lacked coordinated descriptor updates;
the subsequent GPU smoke caught zero confidence in an unwritten new output. Neither is valid
performance evidence. Both correctness failures were fixed before this stage's commit; `-03` is
the corrected replay. The mapped-write guard prevents the first category becoming memory damage.

## Remaining implementation stages

Integrate derived-surface preparation with the `GardenTrees` transaction and rollback, record
unchanged failed inputs without marking them compiled, and optimize healthy deformation/collision
and unaffected-tree compilation. These are not claimed complete by the capacity-stage evidence.
