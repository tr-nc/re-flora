# Twelve-tree bottleneck analysis after immutable leaf publication

## Conclusion

The reproduced default-size twelve-tree garden is primarily **CPU interaction work plus a
serial CPU/GPU publication schedule**, not a leaf-input rebuild or a saturated leaf shader.
The useful next target is the exact moving-tree collision path and its scheduling. Dropped
fruit stepping is also expensive, but its isolated removal does not buy its whole measured
CPU duration back as frame time. CPU and GPU durations overlap; they must not be added blindly.

This pass adds only opt-in diagnostics and a research report. It does not optimize or simplify
rendering/physics, change saved defaults, merge/push, or create release packages.

## Reproduction and evidence quality

- Worktree: `new-tree`; production implementation starts at `a42274fc`.
- Diagnostics: `3c3cf33b` (12-tree prefix-preserving replay and frame-ID matching),
  `ae6f2555` (CPU attribution, actual GPU submission-frame IDs, longer final hold),
  `cb144a9e` (fruit-step attribution and temporary season ablation).
- Hardware: i5-12600KF, RTX 3060 Ti 8 GB. Hidden native Release window, muted global output,
  unchanged Vulkan surface/swapchain, auto-selected FIFO. Audio processing remains active.
- Current saved tree size is **20**, not the size-16 scene in the earlier optimization report.
  Garden Tree seed 122 initially; subsequent seeds 123–133, fixed placements. First nine
  placements preserve the earlier replay; the last three extend its footprint.
- Twelve trees: **514088 visual trunk vertices, 340444 triangles, 203336 exact CPU points,
  200049 complete transforms, 3134 joints, 170378 leaf instances, 176 dropped fruit bodies**.
- All consuming GPU tree buffers and the immutable response inputs log `DEVICE_LOCAL`.
  Publication is 122355728 logical bytes, below its existing 256 MiB limit. No stale descriptors,
  capacity rejection or repeated failed compilation is present.
- Each case ran twice sequentially, with no other `re-flora` instance at the contention gate.
  All tree counts completed GPU surface/lighting/collision checks, with zero errors/cliffs.
  GUI/camera files are restored by the runner; final git state contains no accidental config diff.
- This is an independently controlled fixture reproducing the user's ~40-FPS scale, not an exact
  replay of the user's unsaved placements/camera. Tree geometry and fruit trajectories still vary
  with seeds, placement, wind and wall-time progression. The benchmark suppresses ordinary ambient
  particle emitters; it therefore does not attribute the live game's decorative-particle cost.

Initial two-second-final-phase replay `target/tree-scaling-twelve-default-01` gives
**23.680 / 23.587 ms** (~42 FPS), directly reproducing the reported scale.
Longer final holds give 22.4–23.7 ms (~42–45 FPS); this small variation is not a new optimization.

## Steady-frame attribution

Authoritative detailed run: `target/tree-scaling-twelve-physics-01`, final forest held for 12 s,
0.5 s excluded for settling; **506 / 508** frame samples. CPU/GPU scope rows are selected by
matching settled frame IDs. GPU logs now label the submission that produced the query results,
not the later CPU frame that collected them. Normal non-benchmark logging cadence is unchanged.

| Scope | Run 1 | Run 2 | Meaning |
|---|---:|---:|---|
| Total frame median | 22.596 ms | 22.427 ms | ~44–45 FPS |
| Total frame sample p95 | 23.982 ms | 23.940 ms | Not a universal worst case |
| CPU fruit update | 8.518 ms | 8.284 ms | Mostly physics stepping |
| Nested fruit physics step | 8.477 ms | 8.241 ms | 120 Hz, normally 2–3 ticks/rendered frame |
| CPU exact trunk skinning | 4.117 ms | 4.059 ms | 203k points / 200k transforms |
| CPU collision publication/refit | 4.974 ms | 4.995 ms | Topology stable, positions and BVH updated |
| CPU rendering command recording | 2.132 ms | 2.130 ms | Prepass nested inside this, ~0.75 ms |
| CPU scene services | 1.190 ms | 1.193 ms | Acoustic snapshot publication + terrain collider queue, not yet split |
| GPU whole rendered frame | 10.089 ms | 10.090 ms | Real Vulkan timestamp interval |
| GPU DDGI probe trace | 1.950 ms | 1.941 ms | Nested GPU pass |
| GPU terrain/direct shadow trace | 1.549 ms | 1.558 ms | Nested GPU pass |
| GPU main tracer | 1.764 ms | 1.764 ms | Nested GPU pass |
| GPU leaf draw | 0.572 ms | 0.570 ms | Canopy still fully rendered |
| GPU leaf lighting cache | ~0.25 ms | ~0.25 ms | Not CPU identity remapping |
| GPU vegetation motion solver | 0.195 ms | 0.195 ms | GPU wind/flutter preserved |
| GPU tree surface update | 0.482 ms | 0.485 ms | No earlier host-only input cliff |

Fruit setup, handle checks and rendering together take only ~0.03 ms. The slow fruit scope is
**the physics step**, not fruit instance construction or drawing. Each tick costs ~2.8–2.9 ms.
The world runs at 120 Hz (`DEFAULT_FIXED_STEP_SECONDS`), with an eight-tick catch-up limit.
Pose readback is ~0.056 ms and explicit pose-job waiting ~0.025 ms; these are not large bandwidth
costs. Canopy audio's main-thread update is ~0.015 ms, ecology/hover each ~0.001 ms. Audio worker
cost is outside this main-thread scope and is not claimed to be zero.

The ~10 ms previously called "untracked CPU" is mostly fruit stepping and scene services.
The new remaining unattributed category is ~0.22–0.27 ms, not another presumed leaf cost.
`render.path` / `gpu_present_ms` includes CPU skinning/publication, command recording and waits;
it is **not GPU execution time**. Tree `total_cpu` also includes its small submit/wait/readback
measurements; do not add it again to its component scopes.

Long-tail caution: an earlier detailed replay (`target/tree-scaling-twelve-attribution-01`)
had a run-2 p95 of **61.33 ms**, with late fruit-step spikes and other CPU slowdowns. Subsequent
physics-probed runs have ~24 ms p95, but do not erase that observation or establish its exact
cause. Background CPU scheduling / contact dynamics need separate investigation if it recurs.

## One-variable diagnostic ablations

These deliberately alter work/appearance to locate costs. They are **not accepted optimizations**,
not runtime visual A/B candidates, and no saved setting remains disabled.

| Case (same size-20 fixture) | Frame medians run 1 / 2 | GPU medians run 1 / 2 |
|---|---:|---:|
| Full baseline (`...twelve-physics-01`) | 22.596 / 22.427 ms | 10.089 / 10.090 ms |
| Fruit cycle 0 (`...twelve-no-fruit-01`) | 21.702 / 21.916 ms | 9.814 / 9.810 ms |
| Raster trunk wind off (`...twelve-no-wind-01`) | 17.075 / 17.062 ms | 7.122 / 7.139 ms |
| Shadows off (`...twelve-no-shadows-01`) | 21.182 / 21.771 ms | 7.742 / 7.862 ms |

Fruit cycle 0 removes dropped bodies while retaining canopy geometry/wind. Fruit CPU update drops
8.3–8.5 → **0.014 ms**, but frame acquisition waiting increases **~0.2 → ~8.0 ms**. Most of the
removed CPU work previously overlapped the preceding GPU frame. The subsequent ~9 ms CPU exact
skinning/refit still occurs after acquiring the frame. "Remove 8 ms physics, gain 8 ms FPS" is false.

Wind-off retains **176 fruit bodies** and the canopy's GPU flutter. It replaces deformed trunk
interaction with authoritative static voxels: skin/refit go to zero and fruit stepping falls
~8.3 → **0.68–0.80 ms**. GPU geometry/query work also changes (DDGI trace ~1.94 → 0.42 ms), so
its overall speedup cannot be assigned entirely to CPU physics. It identifies the moving trunk
interaction path, not the number of fruit alone, as a strong optimization area. FIFO now paces
frames near 17 ms; this is not a claim that GPU/CPU each require 17 ms.

Removing all shadows saves ~2.3 ms of GPU execution but only ~0.7–1.4 ms of median frame time.
This is consistent with CPU/scheduling dominance. Individual leaf shadows are a still smaller
subset; sacrificing them is not the first quality-preserving target.

## Root mechanisms confirmed in source

1. `App` advances fruit bodies using the previous published collision surface, submits the GPU
   joint solver, acquires the frame, finishes joint readback, then calls
   `publish_tree_surface_pose` **before** starting `render.record`.
2. That publication computes every exact CPU point and refits the physics surface each moving
   frame. The earlier compaction/reuse already works; what remains is genuine dense work.
3. `CollisionWorld` currently installs **one whole-forest deforming collider**. Its broad-phase
   AABB spans the forest; detailed triangles are then culled by its inner BVH.
4. `update_deforming_triangle_positions` calls Rapier `Collider::shape_mut()`. Rapier 0.34 marks
   `ColliderChanges::SHAPE`; `narrow_phase.rs` clears the affected contact pair's workspace when
   that flag is set. Unchanging triangle topology does not by itself preserve those caches.
5. `GpuTreePoseSolver` submits `tree.pose` on `get_general_queue()`, and CPU geometry depends on
   its completed poses. Along with acquired-frame resource fencing, this prevents assuming free
   overlap between all CPU geometry work and the preceding graphics submission.

The shared collider / cache invalidation is a strong, code-supported mechanism for expensive
moving-tree contacts. Its precise share within the 2.8 ms physics tick (broad phase, manifold
reconstruction, solver, sleeping/waking) is **not yet profiled separately**. No proposed fix may
just clear the `SHAPE` flag to retain potentially stale contacts.

## Prioritized optimization potential

### 1. Exact spatial collision ownership and bulk refit (highest priority)

Split the monolithic forest collision hierarchy into tree/spatial partitions, with coherent local
geometry revisions and stable global triangle mapping. Preserve exact deformed geometry, hit
ordering, rest-space editing, capsules, fruit contacts, water queries and transactional publication.
Use bulk/SIMD/refit paths instead of repeated per-triangle BVH insert/update traversal. Profile
Rapier sub-stages and candidate/contact counts before deciding the partition granularity.

This targets the directly critical ~5 ms refit and can reduce the costly moving-tree contacts too.
Do not promise an FPS multiple: retained trees also move, overlap ownership must remain exact,
and local AABBs/caches must be proven correct. No proximity-only omission of queried geometry.

### 2. Dense CPU skinning parallelism/SIMD (~4.1 ms budget)

The plan still has ~200k full bindings/transforms; existing deduplication cannot merge them by
position or approximate weights. Benchmark parallel contiguous transforms/points, SIMD or exact
algebraic reuse of shared parent/child pose facts. Preserve operation semantics and normal/query
agreement. Do not reduce tree size, remove faces, quantize bindings, or silently lower stiffness.

### 3. Pipeline scheduling and independent pose publication

Separate acquired-frame GPU palette writes from CPU-only geometry preparation. Evaluate earlier
pose-job submission, actual compute/graphics concurrency, or a bounded independently owned pose
snapshot that allows useful pre-acquisition CPU work. Verify queue/resource ownership and current
pose agreement; simply moving a wait or hiding it in another counter is not an optimization.
The no-fruit ablation demonstrates why scheduling belongs in the plan, not an optional polish.
One-frame-old collision or interpolation changes require explicit product acceptance, not a hidden
"optimization". A full deformed-vertex GPU readback is not a free substitute for the small joint
readback; CPU consumers still need exact data and synchronization.

### 4. Fruit simulation sleep/contact locality; frequency is a tradeoff

Avoid waking/rebuilding unrelated resting-fruit contacts for a distant tree update. Keep newly
falling/contacting fruit and terrain edits correct. A 120 → 60 Hz fruit tick experiment could
reduce tick CPU cost, but changes integration/contact/CCD behavior and needs approval/testing.
Retiring sleeping fruit into a cosmetic/static representation changes semantics too. Leaf VFX
simplification approval does not authorize fruit simplification. Alone, fruit removal's observed
frame-time benefit is small until the serial geometry path is addressed.

### 5. GPU visibility/shadow/DDGI queries (second stage)

Once CPU interaction/scheduling improve, GPU ~10 ms becomes a meaningful ceiling. Largest
measured opportunities are DDGI/direct-shadow/main ray traversal and whole-forest secondary BVH
query locality, not the ~0.195 ms leaf motion solver. Investigate coherent hierarchical culling,
ray/BVH layout, query-specific acceleration and per-pass work budgets with light-field correctness.
Do not equate skipped DDGI updates or disabled shadows with a quality-preserving speedup.
Leaf draw/light cache remain candidates for heavier views, but are not the priority in this fixture.

Planting pauses are a separate problem: the last placement still takes ~0.7 s (derived preparation
~0.66 s), despite fragment reuse. Chunked/off-side asynchronous compilation may improve interaction
latency, but must retain prepare-before-commit/rollback and is not steady FPS evidence.

## Commands and validation

```sh
cargo build --release
uv run --no-project scripts/diagnose_tree_scaling.py \
  target/tree-scaling-twelve-next --trees 12 --final-seconds 12 --validate-gpu
# Diagnostic only, saved fields restored automatically:
# append --fruit-cycle 0 / --wind-off / --app-arg=--no-shadows, one at a time
```

Formatting/check, the 12-tree placement-prefix test, three frame-timing tests, 31 app-physics
tests, 11 parser tests and Ruff pass. Repeated hidden GPU-validated replays and short launches
with inspected logs pass. Pyright is unavailable; the previously failing unrelated full Rust
suite is not claimed green. No shader or generated-file changes are part of this analysis.

Final validation also passes the complete GPU vegetation-response bit-exact comparison and full
raster-tree smoke (age/replacement/removal/Conifer/restoration), including exact publication
rollback. Release was rebuilt after the analysis and the short hidden run log was inspected.
All four long-hold cases have the same executable SHA-256 and original GUI SHA-256; the current
GUI file matches that original hash. Diagnostics/report are committed; production settings and
rendering quality remain unchanged.
