# Climbing vine: search, attach, prune, regrow

The playable vine uses **rooted pruning**, not quasi-static falling. This replaces the
previous settling model at the user's request: a missing backing surface cuts off its
whole downstream branch, even when higher attachments still have wall behind them.
The retained lower stem keeps its geometry and history. Normal growth now makes **one
unbranched vine**. Only the young span beyond the latest attachment bends and sags; a new
attachment freezes that completed span. A retained cut is also a stable growth base.

## Try it

1. Launch this worktree (`cargo run --release` for release playback).
2. **R → Climbing Plants**: changing **Test terrain** (flat wall, hole, outward ledge,
   recessed wall, sloping wall, or ground-to-wall) immediately enables/rebuilds the test
   patch and restarts the vine. There is no confirmation or extra reset click.
   **Create vine wall and focus** starts the initially selected scene; **Restart wall and
   vine** repeats it directly. **Vine seed** and **Clockwise tip search** changes also restart
   immediately. **New random seed** picks another seed and restarts; the same seed, winding,
   settings and simulation inputs reproduce the same initial state and exploration.
   This edits real terrain: do not use the demo patch for terrain you want to preserve.
   It is placed beside the startup tree, around voxel X=383/Z=300, with the wall base on
   current soil/sand/rock and a two-voxel embedded footing. It ignores wood/foliage as ground
   and waits instead of guessing when the source is unavailable. Resets reuse the site,
   so they cannot progressively stack walls on their own previous tops.
3. Watch the tip sweep diagonally upward, find an exposed surface and attach, then
   continue searching. The lighter-green young span bends under gravity and rotating
   exploration; older dark-green geometry stays fixed. **Unattached shoot flexibility**
   tunes this live response (default 1, range 0–2; 0 returns toward its intrinsic shape). Small existing holes can be bridged; a ceiling is followed
   tangentially to its edge, and growth resumes upward once clear. Let it grow. **3 / Dig**, LMB removes wall; **Shift + wheel** adjusts brush size.
   When the edited terrain becomes current/ready, the first unsupported step and its
   descendants disappear. There is no need to disconnect the root or wait for gravity.
4. The retained cut becomes a regrowth bud. The cut's first step waits while its wall
   is missing or blocked; repairing the gap allows growth from that exact cut.
5. **Pause vine growth** stops extension/search progression, not young-shoot settling or
   terrain-triggered pruning. A waiting cut stays completely still. Turn it off
   to regrow. **Prune highest attachment** / **Prune back to root** let you trim without
   editing terrain; intact wall permits immediate regrowth.
6. **Focus vine** recenters the remaining skeleton. Restart is immediate.
   **Disconnect root** explicitly stops regrowth until reset; it is not a fall command.
7. Optional markers: yellow attachment, blue next search probe, orange regrowth bud,
   red unsupported root seed.
   **Blocked-tip test → Refill terrain through tip** embeds real limestone in a tip;
   the blocked stem is pruned. Remove that inserted material to unblock regrowth.

Enable, terrain selection, seed, rotation direction, flexibility, pause, growth speed,
attachment spacing and marker settings use the declarative Debug **Save** path. **Plant history is session-only**, not part of terrain snapshots.
Loading/replacing the world clears the vine and cache without automatically creating a wall.

## Model and safety

- At most 512 **live** nodes, with one initial root/tip and no automatic branching.
  The pruning graph still supports explicit trees for its branch-isolation guardrails.
  Live storage compacts after pruning; monotonic node IDs are never reused. Parent,
  anchor and tip indices are remapped together. Leaf appearance uses stable IDs.
- Root-to-tip checks include every recorded surface contact and originally continuous
  backing line, not just sparse attachment markers. Removing previously present backing
  between sampled nodes counts. Exploratory arcs over **pre-existing** empty holes do not
  invent backing dependencies and do not prune themselves. Missing recorded material,
  material replacement or a buried stem prunes the first invalid edge.
- A cut retains its parent and creates one frontier bud, not a bud at every deleted node.
  Buds retry the first removed step exactly. Blocked buds do not consume RNG even while
  other branches grow. Repeated cuts reclaim capacity rather than exhausting historical IDs.
- Losing the entire backing wall retains only a latent root seed, which cannot extend
  until the root's surface is suitable again. Restoring the wall does not resurrect old IDs.
- Validation is transactional against immutable Contree exports. Unknown/stale data never
  means empty terrain. Published edit bounds and dependency polling trigger validation;
  cache reuse checks full bounds, presence, revision **and** readiness, including empty chunks.
- Established nodes and retained stumps never move. The current young span uses a
  20 Hz overdamped, gravity-biased elastic pose, with rotating distal search influence.
  Forward kinematics preserves every segment's rest length. Bounded line search checks
  the **whole swept segment interior** against radius-expanded voxels, not just endpoints.
  Contacts/backing are refreshed at the new pose; stale/unavailable queries roll back
  geometry, attachment and RNG changes together. This is not whole-plant falling,
  inertial mechanics or detached-remnant simulation.
- Growth combines an upward/local-tangent direction, rotating radial exploration (24
  angular phases per turn), and a small support-seeking bias. A nominal step is 2 voxels;
  a local contact correction is limited to 2.5 voxels and must have a clear entire segment.
  Surface normals, rather than one global wall plane, orient subsequent search and leaves.
- Only exposed faces of the root's material are eligible attachments. Free extension is
  bounded by `clamp(3 × spacing, 12, 64)` voxels from its growth base (the last attachment
  or retained cut). Deformation preserves arc length and cannot renew that budget. Blocked ordinary tips rotate their probe without
  consuming RNG. Retained cut buds instead retry the stored severed step unchanged.
- This is an artistic abstraction of **circumnutation**, not a claim that all wall-clinging
  species twine, nor a biomass/elasticity simulation. Ground-to-wall and five wall fixtures
  exercise the same terrain description in analytic tests and real editable voxels.
  See [research/climbing_vine_circumnutation.md](research/climbing_vine_circumnutation.md).

## Validation and CPU profiling

```sh
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
RE_FLORA_CLIMBING_REVIEW=1 cargo run --release -- --hidden --mute --perf --auto-exit 12
cargo run --release -- --tail-latest-log 200
```

The new deterministic review edits real terrain, asserts downstream removal despite valid
upper attachments, waits 30 ready frames without changing the stump, repairs the gap and
checks new IDs/growth from the cut. It then removes the entire backing wall and verifies
latent-root waiting and recovery after restoring the wall. Require:

```text
pruned=true upper_attached_removed=true stable_survivors=true before=64 after=22
waiting=true root=false frames=30 nodes=22
regrown=true from_cut=true fresh_ids=true
waiting=true root=true frames=30 nodes=1
verified prune=true wait=true regrow=true root_recovery=true finite=true
young_shoot_moved=true frozen_history_stable=true single_tip=true
phase=complete failures=0
```

2026-09-23 local evidence: `target/climbing-validation/pruning/` contains the failing
pre-change pruning test, check/test/smoke logs and successful release review. The
previous test failed with `unsupported upper stem is still suspended`; it now passes.
Original exploration evidence is in `target/climbing-validation/exploration/`. In addition
to the pruning review, run each actual terrain fixture (use longer auto-exit if needed):

```sh
for scene in flat hole outward inward slope ground; do
  RE_FLORA_CLIMBING_REVIEW=$scene cargo run --release -- --hidden --mute --perf --auto-exit 12
done
```

Each must print `fixture=<name> verified=true collision_clear=true attached_height=...`
and a successful shutdown. Review fixes seed 42, clockwise search, spacing 16 and flexibility 1;
there are two 50 ms pose steps per ready growth attempt. The fixture review stops after 180 ready growth attempts,
requires an attachment at least 70 voxels above the site's ground level (reference Y=262), and checks all stem segments and attachment
materials against the current terrain export. These checks are not satisfied merely by
an airborne tip reaching above the obstacle. `RE_FLORA_CLIMBING_REVIEW=1` retains the
separate prune/wait/repair/root-recovery sequence.

Grounded/immediate-control regression evidence: `target/climbing-validation/live-shoot/site-*.log`.
The initial UI tests failed on the required extra confirmation/missing selection reaction;
the placement test failed on the old constant base Y=192. Ground-height, vegetation rejection,
unknown/stale/no-ground/headroom checks, UI input, full tests and release prune/ground/slope
reviews pass. At the standard scene's new site the sampled ground is Y=129, not the old Y=192.

Current single-shoot evidence is `target/climbing-validation/live-shoot/shoot-*`:
1107 + 4 tests passed (2 ignored), Release smoke, all six real-terrain fixtures, and the
64 → 22 node prune/wait/repair/root-recovery sequence. Every review checked actual young-node
motion and frozen-history invariance. Native screenshots of inward/ground/slope scenes
show the grounded single vine; these are not user approval of its animation.

Unit tests cover seed-42 search in **both independently selected directions** on every
fixture, reproducibility/safety of additional seeds, ground-to-wall growth, finite air
extension, explicit-tree pruning isolation, swept-interior collisions, sagging/attachment
freezing, pause/cadence behavior, pending/stale snapshots, and saved settings/older defaults.
Different seeds are allowed to reach a side edge and stop; bounded exploration is not a
route planner or a guarantee that every seed reaches the top.

`[CLIMBING][PERF]` now reports export, pruning/revalidation, growth, young-shoot pose (`pose_us`), render preparation and
voxel-query counts. It excludes fixture edit/camera actions and is not a GPU/FPS measure.
**This scenario and algorithm differ from the old settling benchmark**; do not directly
pool or compare their workloads. Current evidence and limitations are recorded in
[research/climbing_vine_live_shoot.md](research/climbing_vine_live_shoot.md): collision-export
spikes still reach 25.5 ms, and performance acceptance is **not** claimed. Historical
measurements/research remain in [research/climbing_vine_tuning.md](research/climbing_vine_tuning.md).
