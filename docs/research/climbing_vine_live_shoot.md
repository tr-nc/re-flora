# One seeded vine with a flexible young shoot

**Historical / original A mode:** the current saved A/B control also offers a
[continuous-stem experiment](climbing_vine_continuous_stem.md). The user subsequently
allowed older stem to move. Frozen-history requirements, default values and measurements
below describe the original implementation, not requirements for the new model.

Date: 2026-09-23. Follow-up to grounded/immediate scene controls (`66a56fbe`).
This replaces the permanently rigid growing zone and automatic forks, not the
rooted-pruning/retained-bud gameplay. See [the feature guide](../climbing_plants.md).

## Evidence and modeling boundary

The [circumnutation note](climbing_vine_circumnutation.md) records the primary sources:
[the 2018 bio-hybrid study, §3.1.2](https://pmc.ncbi.nlm.nih.gov/articles/PMC6227980/),
[the 2023 pea support-searching study](https://pmc.ncbi.nlm.nih.gov/articles/PMC10143786/),
and [Hädrich et al.'s persistent interactive climbing-plant model](https://doi.org/10.1111/cgf.13106).
They support a distinction between exploration and contact, variable handedness, and
persistent local interactions. **They do not validate our numerical stiffness, timing,
adhesion distance, or this particular elastic approximation.** Those are artistic choices.

The user asked for the young span to bend/sag while older growth stays fixed, not for
falling remnants or a physically calibrated whole plant. The implementation is an
overdamped, cantilever-inspired pose model, without inertia, oscillatory momentum,
biomass mechanics or stretch. It is deliberately not described as XPBD or a reproduction
of the cited papers' solvers.

## State and authority

- A seed makes exactly one tip. Automatic fork creation is removed; the underlying
  rooted graph and explicit-tree pruning tests remain useful. The 512-live-node limit,
  monotonic identities and compaction rules are unchanged.
- `Node.fixed` marks established geometry. Only the path from the current growing tip
  back to its fixed base can change. A new attachment freezes the completed path.
  Pruning freezes the retained stump; a waiting bud never moves, and repair starts a new
  young shoot without reanimating surviving geometry.
- `src/climbing_plants/shoot.rs` owns deformation: 20 Hz pose quanta, an 8/s exponential
  response, length-dependent transverse gravity and a smaller distal rotating-search
  influence. The characteristic bend length is 40 voxels. Forward kinematics preserves
  every rest length; it does not rebuild the plant or reuse node identities.
- At most ten line-search trials and 0.25 voxel displacement per pose quantum bound
  motion. Contact half-spaces allow tangential movement while retaining clearance.
  SAT checks the convex hull of the old/new endpoint pairs against radius-expanded
  voxels, conservatively covering the **entire swept segment interior**. Final segments
  are checked too. Unknown/stale queries commit neither pose, attachment nor RNG changes.
- `growth.rs` owns face candidates and continuous backing descriptions for both growth
  and deformation. A moved young node refreshes its contact/backing at its **new** pose;
  it never keeps an imaginary constraint at its former location.
- Young extension remains bounded. Pose preserves arc length, and ordinary blocked
  searches rotate without adding length. This is not global pathfinding: a seed can
  wander to a wall edge and stop instead of reaching the top.

## Controls

The normal declarative/save pipeline owns `climbing_seed` (0–65535, default 42),
`climbing_clockwise` and `climbing_flexibility` (0–2, default 1). Older files receive
missing defaults without losing existing settings. Seed controls initial angular phase,
small lateral variation and later spacing RNG; winding is independently selectable.
The same seed and simulation inputs reproduce the same plant; this is not a replay
system for differing terrain edits, settings or update schedules.

Terrain, seed and winding edits immediately restart the grounded fixture. **New random
seed** changes the actual saved seed, not a hidden App-only copy. Restart alone preserves
that seed. Resets reuse the sampled site beside the startup tree rather than climbing
onto previously authored terrain. Flexibility edits affect the young span live; 0 returns
it toward its intrinsic shape rather than unfreezing old geometry. Pausing extension
still permits young-shoot settling, but pauses search progression. Young stem segments
render lighter green and the panel reports their count.

## Failing hypotheses and corrections

`target/climbing-validation/live-shoot/` contains the local evidence (not committed):

- `shoot-red.log`: the original behavior still made four tips, same-handed seeds had
  identical initial probes, and the existing unattached shoot never moved.
- Checking only final segments or moving endpoints is insufficient for a deforming
  rod: its swept interior can pass through a voxel. A dedicated regression has all four
  boundary paths clear but the swept interior blocked.
- Isotropically inflating the old segment by maximum movement was safe but over-restrictive
  near walls. The swept convex-hull test permits legitimate tangential/away motion.
- Sagging slightly separates a tip from a ceiling; loss of its contact flag does not
  mean it has passed the ledge. Testing only the overhead center voxel also turns upward
  too early. Navigation now checks upward clearance for the **whole stem radius**.
- A single randomized vine has no sibling that can take another route. Seed 43 with its
  legacy counterclockwise default reached a slope side edge (highest attachment about
  reference Y=260), not the old arbitrary Y=262 target. It is retained as a safety and
  reproducibility case, not advertised as successful top-reaching. The six climbing
  reference cases use seed 42 with **both independently selected windings**, all still
  requiring attachments above reference Y=262. Random seed tests do not assert universal
  route finding or relax collision/history invariants.

## Validation

- `cargo fmt --check`, `cargo check`, `cargo test`: **1107 + 4 passed, 2 ignored**.
- Release hidden/muted smoke; logs inspected through the worktree's built-in helpers.
- All six real editable-terrain reviews passed. Each used 180 ready growth attempts,
  seed 42, clockwise, spacing 16, flexibility 1, and two 50 ms pose quanta per attempt.
  The grounded site's soil surface is Y=129; required attachment height is Y=199
  (70 voxels above ground), not merely an airborne tip above the obstacle.
- Every review verified one tip, motion of already-existing young nodes, unchanged
  established nodes, clear stem segments and material-backed attachments. Maximum
  observed young-node displacement per review sample was 0.375–0.491 voxels.
- The separate pruning review passed **64 → 22 nodes**, removal of higher attached
  descendants, unchanged survivors, 30 ready waiting frames, repair with fresh IDs,
  whole-wall loss to one latent root, and root recovery. The old 100-node/fork fixture
  threshold was replaced by 64 nodes plus at least three attachments for one shoot.
- `shoot-{ground,inward,slope}.png`: actual native-rendered screenshots inspected for
  grounded placement and single-shoot geometry. These do not establish user approval
  of the moving appearance; no visible game was launched after the shutdown request.

| Scene | Live nodes | Attachments | Highest attachment Y |
| --- | ---: | ---: | ---: |
| flat | 92 | 4 | 219.052 |
| hole | 90 | 4 | 216.853 |
| outward | 91 | 5 | 212.957 |
| inward | 117 | 5 | 237.703 |
| slope | 104 | 7 | 215.000 |
| ground | 93 | 4 | 213.851 |

### Release CPU observations, not acceptance

Measured in these release app runs, 180 active samples per scene, nearest-rank p95.
`pose_us` contains both pose quanta. Total also includes export, revalidation, growth,
render preparation and review checks; it excludes terrain-authoring/camera actions and
GPU/whole-frame time. Idle samples after completion are excluded.

| Scene | Growth p95 µs | Pose p95 µs | Pose max µs | Total median µs | Total p95 µs | Total max µs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| flat | 6 | 187 | 205 | 59 | 891 | 11613 |
| hole | 6 | 147 | 155 | 63 | 187 | 5016 |
| outward | 6 | 131 | 145 | 72.5 | 158 | 5055 |
| inward | 7 | 56 | 69 | 54 | 3499 | 25530 |
| slope | 9 | 36 | 55 | 24 | 2388 | 11368 |
| ground | 7 | 163 | 176 | 73.5 | 570 | 4821 |

Collision-export expansion spikes remain, including **25.5 ms** total in the inward
scene. This is a different single-shoot, grounded workload and merged renderer, **not an
A/B performance improvement claim**. No performance/FPS acceptance has passed. Visual
approval and subsequent performance optimization remain separate work.

Only stem geometry is collision-solved; decorative leaf boxes can intersect terrain.
Plant history remains session-only. No detached pieces, arbitrary-maze navigation,
large-gap guarantee or universal botanical realism is promised.
