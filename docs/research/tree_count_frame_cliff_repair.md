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

## Complete garden transactions and failed-input suppression

`GardenTrees` now prepares its explicit desired complete forest after trunk publication, including
initial skin/attachment poses, every descriptor, and off-side collision geometry, before publishing
observers or committing canonical records. Surface, observer, and final-commit failures restore
place/replacement/removal/age batches while retaining the resident surface. Saved restoration uses
one complete-forest transaction against the restored edited atlas, not sequential removal/reinsertion.
Fruit-cycle rollback retains the physical bodies owned by its checkpoints.

A real hidden GPU smoke independently exposed a further rollback bug: inverse cone stamping was
not an exact checkpoint. Rejected placement filled an existing hand-carved hole and/or changed
underlying soil bytes. `/tmp/tree-exact-rollback-red.out` fails the before/after byte comparison.
The concrete host now completely captures clipped old/new/retained atlas regions before any write,
preflights a 256 MiB CPU checkpoint budget, and restores full bytes (material, moisture, ownership).
`/tmp/tree-exact-rollback-green.out` passes that same rejected-placement comparison, canonical and
resident-surface checks, GPU surface validation, and the full Garden Tree/Conifer smoke.

The failed `(terrain revision, canonical revision)` remains dirty and is not recompiled every frame.
A changed revision or explicit invalidation permits retry. Ordinary committed terrain edits use a
static authoritative-voxel fallback on preparation failure rather than rendering stale deformation;
this does not change saved wind configuration or pretend publication succeeded. New tree operations
reject atomically instead. External VRAM becoming free is not automatically detected: known resource
condition changes must invalidate the cache, and an explicit new tree operation prepares anew.

Transaction replay `target/tree-scaling-transaction-02` passes CPU/GPU positions/normals/confidence,
BVH, lighting and collision/query comparisons at **every** count, including 146904 vertices at six
trees. Six-tree medians are **31.723 / 31.582 ms**, `errors=0`, `cliff=False`, `contended=False`.
`-01` is invalid evidence: the dynamic-query smoke fixture required at least one voxel of movement
while the performance scene's first phase was at rest. The opt-in validator now checks real queries
without imposing artificial wind, while the full motion smoke retains its displacement requirement.

Validation: vegetation tests 42, tree/cache tests 21, prepared collision regression, 7 diagnostic
parser tests, Ruff, formatting/check, Release build, full raster GPU smoke and hidden launch.
The previously attempted full suite is not claimed green (five unverified-baseline GUI/config failures
and a long-running vine timeout); global pyright was unavailable. No generated files changed in this
transaction stage.

The final exact-checkpoint replay `target/tree-scaling-transaction-03` repeats all six-count GPU
checks with medians **31.670 / 31.355 ms**, zero errors/cliffs/contention. Its Release binary is saved
as `target/tree-scaling-pre-cpu` for the identical healthy-CPU comparison.

## Exact healthy CPU deformation and collision reuse

The immutable interaction plan groups only bit-identical rest positions **and** complete bindings
(tree ID, child/parent IDs, weight bits). It shares transforms without merging the per-cell visual
normals/confidence or GPU vertices. Visual indexing and ordered triangle IDs remain unchanged for
GPU comparisons, barycentric rest-space editing and collision queries. Six trees still render all
146904 vertices / 101120 triangles, while CPU interaction uses **59875 exact points / 59006 transforms**.
Canonical pose lookup happens once per tree. Double-buffered pose scratch and physics-unit positions
are reused; collision retains immutable triangle topology and updates only vertices/BVH. Exactly
unchanged physics points need no refit (no quantization or motion approximation).

A scalar-reference regression compares every visual point **bit-for-bit** across poses and verifies
that differing owner/parent/weight bindings do not weld. The topology-stable physics API preserves
triangle IDs/collider handles and rejects wrong-size/nonfinite updates before changing queries.
Validation: raster tests 12, all physics-crate tests/integration tests, check/format, Release build,
full Garden Tree/Conifer GPU smoke including exact rejected-placement rollback, and hidden launch.

Uncontended identical workload `target/tree-scaling-compact-02` (two runs, per-count GPU validation):

| Trees | Before CPU stage, run 1 / 2 ms | After CPU stage, run 1 / 2 ms |
| --- | ---: | ---: |
| 4 | 19.713 / 19.935 | 16.859 / 17.045 |
| 5 | 25.461 / 25.697 | 19.161 / 19.126 |
| 6 | 31.670 / 31.355 | 27.189 / 27.210 |

Settled run-1 six-tree medians, parsed from matching frame IDs:

| Tree update CPU component | Before us | After us |
| --- | ---: | ---: |
| Skinning | 4604.44 | 1206.945 |
| Collision | 1971.03 | 1474.995 |
| Query publication | 123.115 | 38.715 |
| Total (includes hierarchy submit/wait/readback) | 6992.515 | 2927.22 |

Zero errors/cliffs/contention and complete CPU/GPU/BVH/lighting/query checks. GPU execution has not
been optimized here: six-tree `frame.render` is about 14.4 ms and `tree_surface_update.pass` 5.0 ms.
The earlier `compact-01` second run has a faster 23.254 ms frame but also a materially different GPU
scope (10.302 / 2.012 ms); it is not evidence of an extra CPU improvement. `gpu_present_ms` is not
interpreted as pure GPU execution. The GPU variation is the next measurement target.

## Measured GPU memory-placement variation

Opt-in allocation-property logging independently confirms the cause in
`target/tree-scaling-memory-01` (all per-count GPU checks pass): four/five-tree static skin inputs
use `DEVICE_LOCAL | HOST_VISIBLE | HOST_COHERENT`, but the sixth tree's 12582912-byte `skin_rest`
falls back to **HOST_VISIBLE | HOST_COHERENT without DEVICE_LOCAL**. GPU-only outputs remain local.
The 27.191 ms frame / roughly 5 ms surface-update scope is therefore not CPU deformation cost.
Permanent shader inputs must not depend on the remaining host-visible BAR heap. This logging is
opt-in; normal frames/settings are unchanged.

## Device-local immutable GPU inputs

Immutable skin inputs, primitive/refit data, occupancy/surface/attachment lookups and indices now
use GPU-only device-local buffers. One fallible, bounded, off-side staging upload prepares them;
managed transfer dependencies replace per-frame fictitious host-write declarations. Only tiny
changing headers and pose palettes remain mapped. Temporary staging is included in the 512 MiB
old-plus-new logical peak preflight; upload errors cannot swap the resident publication.

`target/tree-scaling-local-01` passes every per-count GPU/BVH/lighting/collision check, with all seven
logged hot buffers reporting `DEVICE_LOCAL`. Six-tree frames are **21.360 / 21.423 ms** (about 47 FPS),
`frame.render` **8.421 / 8.142 ms**, and `tree_surface_update.pass` **0.231 / 0.211 ms** versus about
14.4 / 5.0 ms before. Zero errors/cliffs/contention. Full raster smoke, rejected-placement exact
rollback, 23 tree/cache/upload/budget tests, format/check, Release build and hidden launch pass.
Shader equations/rendering quality are unchanged. This fixes the measured heap fallback rather
than reducing tree detail or accepting host-visible allocation luck.

## Unaffected-tree compilation reuse

The resident mesh keeps bounded derived per-tree fragments. Immutable tree identity, placement,
exact world cone bits, read extent and actual sampler material bytes must all match; a revision,
count or hash alone cannot reuse geometry. Moisture/state bits are excluded only because the same
shared material decoder demonstrably excludes them from extraction/normals. Forest assembly keeps
complete extraction's normals/faces and stable lowest-ID binding ownership. The optional cache has
a conservative 256 MiB bound; over-budget fragments still assemble but are not retained. Removed
trees and failed candidate publications leave no accumulating history or second canonical owner.

Regressions compare against complete uncached forest compilation for overlap, cut wood, normal-only
neighbor changes, state-bit-only changes, replacement identity and cache-budget fallback. Raster
suite 15 and vegetation suite 42 pass, along with full GPU smoke (age/remove/replace/Conifer/saved
restoration and exact failed-placement rollback), format/check, Release build and short launch.

`target/tree-scaling-fragments-02`: six-tree medians **21.647 / 21.612 ms**, zero errors/cliffs/contention,
per-count CPU/GPU/BVH/lighting/query checks. Planting the fifth tree reuses four fragments; the sixth
correctly reuses four and rebuilds two (neighboring material input changed). Run-1 full derived
preparation at five/six trees decreases **153.879 / 195.792 → 137.435 / 182.833 ms** versus the
preceding device-local stage. GPU readback, whole-forest acceleration/collision construction and
resource/descriptor preparation still cost time. **One-time planting hitches are reduced, not
eliminated**; no healthy-frame improvement is attributed to this compilation cache.

## Final boundary validation and scope

- `target/tree-scaling-final-garden-01`: nine size-16 Garden Trees, one uncontended run, all count-level
  GPU checks pass; six/nine medians **21.467 / 39.129 ms**, zero errors/cliffs.
- `target/tree-scaling-final-conifer-01`: seven-tree mixed forest (initial Garden Tree, then Conifers),
  seeds 923 onward, one uncontended run, all GPU checks pass; seventh median **16.871 ms**. This
  different species workload is correctness/boundary evidence, not an identical-scene speedup claim.
- The authoritative original six-tree workload was **111.226 / 110.239 ms**; final two-run same-scene
  result is **21.647 / 21.612 ms** (roughly 9 → 46 FPS). Full geometry and rendering settings remain.

These repairs cover ownership, demand capacity/descriptors, complete garden publication/rollback,
failed-input suppression, exact healthy CPU/collision reuse, static GPU memory placement and
unaffected compilation. Cost still scales with genuine tree/leaf/shadow work. Arbitrarily large
forests are not promised constant FPS; explicit resource budgets reject rather than silently omit
geometry. Real driver OOM/device-loss recovery and whole-world snapshot-load rollback were not
fault-injected; pure budget/allocation-bound guards and preparation failure semantics were tested.
Existing neighboring-tree redraw behavior (authored cones can recreate earlier cuts on a successful
neighbor change) is unchanged; failed-operation rollback now restores cuts/soil exactly. External
VRAM release still requires explicit invalidation/another tree operation, not automatic retry.
The full repository suite remains unclaimed as noted above; targeted suites are green.
