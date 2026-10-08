# Static no-wind tree collision with precise visual picking

**Stage history:** These are historical full-rest-surface measurements. The later
[GPU picking change](gpu_tree_picking.md) replaces the per-frame CPU visual copy described here;
normal picking is asynchronous and GPU-backed, with CPU deformation reserved for validation.
The subsequent [shared trunk-only policy](tree_trunk_collision_policy.md) narrows physical wood
to root axes for every species without changing visual/picking geometry. The old measurements
remain evidence for removing per-frame refits, not measurements of the newer collider workload.

## Accepted behavior

The user explicitly accepted approximating tree physical collision by its **no-wind rest geometry**:
players mainly contact the trunk/root, where wind displacement is small. Wind no longer changes
physical tree vertices, BVH bounds or contact workspaces. Upper branches can visibly move away
from their physical collision, affecting player/fruit contact there; this is intentional, not an
exact animated-collision claim. Fruit stepping remains at 120 Hz and fruit bodies are not removed.

Rendering, lighting, shadows, foliage motion and attachment poses remain animated. Mouse/CPU ray
queries and barycentric rest-space wood editing still follow the actual posed surface, so a moved
branch remains selectable and editable. No new GUI setting, quality reduction, shader change or
saved default change is introduced.

## Implementation

- `RasterTreeMesh::rest_collision_geometry` projects the existing compact immutable CPU plan to
  rest points and its ordered mapped triangles. No current pose is accepted by collision preparation.
- `TerrainPhysics::prepare_tree_collision_surface` builds this geometry off-side, even if retained
  trees are currently bent. `PreparedTreeSurface` commits it atomically with the intended forest.
  Placement, replacement, age rebuild, edited wood, removal and restoration update collision;
  ordinary wind frames and wind on/off toggles do not.
- The former per-frame physical publication/refit is deleted. Committed-terrain preparation failure
  explicitly clears the mesh/exclusions to recover authoritative voxel collision. Rejected tree
  transactions still keep the old publication intact.
- CPU skinning is retained for **precise visual queries**, not physical collision. It computes a
  componentwise maximum rest-to-posed displacement in its existing point loop. Read-only candidate
  traversal expands immutable rest BVH bounds by that amount and rounds outward; exact posed
  triangle intersection and original barycentric mapping remain the narrow phase. Triangle IDs,
  collider geometry, broad-phase bounds and contact workspaces are unchanged by that traversal.
- Physical collision is not simply frozen at its first windy frame, and the implementation does
  not mutate vertices while suppressing Rapier's `SHAPE` notification. Its geometry truly stays rest.

## Controlled Release results

Worktree `new-tree`, i5-12600KF / RTX 3060 Ti, size **20**, same fixed 12-tree fixture, hidden native
Release window, muted output, FIFO, final forest held 12 s with 0.5 s settling excluded. Each count
passes GPU geometry/lighting/query validation. No other game at the contention gate, zero errors
or cliffs. Run order: old twice, new twice, old twice. Reports and raw logs remain under `target/`.

All cases have the same original GUI SHA-256 (`450a82ed59ed7ab2…`), restored after each run. Twelve
trunks remain **514088 vertices / 340444 triangles**, **203336 exact CPU points / 200049 transforms**,
**170378 leaves**, **176 dropped fruit bodies**, and **122355728 bytes** of logical GPU publication.
Hot input buffers remain device-local. CPU/GPU statistics use matching settled frame IDs.

| Case/report | Run 1 median | Run 2 median | Frame sample p95, run 1 / 2 |
|---|---:|---:|---:|
| Old, before (`tree-scaling-rest-collision-before-01`) | 26.192 ms | 24.752 ms | 60.850 / 31.890 ms |
| Static rest collision (`tree-scaling-rest-collision-after-01`) | **18.351 ms** | **18.577 ms** | **21.215 / 21.119 ms** |
| Old, reverse (`tree-scaling-rest-collision-reverse-01`) | 23.769 ms | 23.908 ms | 26.834 / 27.004 ms |

New results are **53.8–54.5 FPS**, versus reverse old results **41.8–42.1 FPS**. Against those calmer
reverse runs, median frame time falls ~22–23%, FPS rises ~29%. The first old replay has substantial
long-tail variability; it is disclosed, not silently dropped or used to inflate a precise gain.
This is not a constant-FPS guarantee for arbitrary forests/cameras.

| Matched settled scope | Old reverse run 1 / 2 | New run 1 / 2 |
|---|---:|---:|
| CPU physical tree update/refit | 4.994 / 5.084 ms | **<0.001 / <0.001 ms** |
| CPU fruit physics step | 8.574 / 8.763 ms | **0.626 / 0.525 ms** |
| CPU exact visual skinning | 4.052 / 4.083 ms | 4.151 / 4.156 ms |
| CPU tree total, including pose submit/wait/readback | 9.596 / 9.722 ms | 4.632 / 4.625 ms |
| CPU frame acquisition wait | 1.332 / 1.103 ms | 8.826 / 9.118 ms |
| Actual GPU whole frame | 11.591 / 11.569 ms | 11.383 / 11.587 ms |

The GPU workload is not removed. The large fruit-step gain is consistent with keeping immutable
contacts rather than invalidating a moving whole-forest shape. No separate narrow-phase/solver
breakdown was performed, so do not attribute every saved microsecond to one Rapier sub-stage.
Acquisition waiting grows because CPU fruit work previously overlapped the preceding GPU frame;
do not sum the removed ~5 ms refit and ~8 ms physics into a promised ~13 ms frame-time gain.
The remaining ~4.2 ms CPU skinning is genuine exact picking/validation work, not hidden collision
refitting. On-demand visual queries/scheduling remain possible follow-up optimizations.

## Validation

- Regression first went red on the original collision preparation: it published the windy surface
  rather than rest positions (`/tmp/tree-rest-regression-red-02.out`). Initial fixture compilation
  failure was repaired first and is not counted as this semantic red result.
- **33 app-physics**, **15 raster**, **42 vegetation**, **9 response** tests pass.
- Physics-crate unit and integration suites pass, including immutable topology, padded query
  read-only behavior, finite/negative margin guards, contact/capsule/voxel and fixed-scene checks.
- New app test verifies rest collision across 16 poses, capsule blocking and invariant player-ray
  hits; all sampled posed triangle rays remain candidates. Rejected preparation keeps old geometry;
  prepared replacement changes it only on commit; removal and repeated fallback clear it.
- Complete hidden GPU smoke passes wind off/on, strong posed-query agreement (**17 matched rays**),
  posed wood removal (**2 wood cells**), age rebuild, removal/replacement, Conifer restoration,
  lighting/normal-confidence validation, exact transaction rollback and bit-exact GPU leaf motion.
- Opt-in `[TREE][REST_COLLISION] verified=true wind_independent=true` confirms the new rest contract
  independently of visual motion in smoke and the twelve-tree replays.
- Formatting/check, Release rebuild and short hidden launch pass; run log inspected. No generated
  or authored config files changed. No visible game was automatically launched.

The full repository suite's preexisting/unverified unrelated GUI failures remain unclaimed; the
changes above do not establish a green whole-repository suite. Manual upper-branch collision
appearance was not reviewed. Approximation is approved, but its visual/physical mismatch remains
an intentional limitation.

```sh
cargo build --release
uv run --no-project scripts/diagnose_tree_scaling.py \
  target/tree-scaling-rest-collision-next --trees 12 --final-seconds 12 --validate-gpu
```
