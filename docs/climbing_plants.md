# Climbing vine: search, attach, prune, regrow

The playable vine uses **rooted pruning**, not quasi-static falling. This replaces the
previous settling model at the user's request: a missing backing surface cuts off its
whole downstream branch, even when higher attachments still have wall behind them.
Other branches and the retained lower stem keep their geometry and history.

## Try it

1. Launch this worktree (`cargo run --release` for release playback).
2. **R → Climbing Plants**: choose **Test terrain** (flat wall, hole, outward ledge,
   recessed wall, sloping wall, or ground-to-wall) and the root tip's rotation direction,
   then **Create vine wall and focus**. Changing these options takes effect on the next
   reset, not by rebuilding a living vine. Branches use opposing search directions.
   Creation clears and authors real terrain at `(224,190,280)..(288,302,366)`.
   Do not reset over terrain you want to preserve.
3. Watch the tip sweep diagonally upward, find an exposed surface and attach, then
   continue searching. Small existing holes can be bridged; a ceiling is followed
   tangentially to its edge, and growth resumes upward once clear. Let it grow. **3 / Dig**, LMB removes wall; **Shift + wheel** adjusts brush size.
   When the edited terrain becomes current/ready, the first unsupported step and its
   descendants disappear. There is no need to disconnect the root or wait for gravity.
4. The retained cut becomes a regrowth bud. The cut's first step waits while its wall
   is missing or blocked; repairing the gap allows growth from that exact cut.
5. **Pause vine growth** stops extension, not terrain-triggered pruning. Turn it off
   to regrow. **Prune highest attachment** / **Prune back to root** let you trim without
   editing terrain; intact wall permits immediate regrowth.
6. **Focus vine** recenters the remaining skeleton. Reset requires confirmation.
   **Disconnect root** explicitly stops regrowth until reset; it is not a fall command.
7. Optional markers: yellow attachment, blue next search probe, orange regrowth bud,
   red unsupported root seed.
   **Blocked-tip test → Refill terrain through tip** embeds real limestone in a tip;
   the blocked stem is pruned. Remove that inserted material to unblock regrowth.

Enable, terrain selection, root rotation direction, pause, growth speed, attachment
spacing and marker settings use the declarative Debug **Save** path. **Plant history is session-only**, not part of terrain snapshots.
Loading/replacing the world clears the vine and cache without automatically creating a wall.

## Model and safety

- At most 512 **live** nodes and four tips. Live storage compacts after pruning; node IDs
  are monotonic and are not reused. Parent, anchor and tip storage indices are remapped
  together. Leaf placement/color derives from stable IDs so unaffected branches do not pop.
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
- Retained nodes never move. New growth uses conservative whole-segment collision tests.
  There is no gravity solver, inertial bending or hanging detached remnant in this mode.
- Growth combines an upward/local-tangent direction, rotating radial exploration (24
  angular phases per turn), and a small support-seeking bias. A nominal step is 2 voxels;
  a local contact correction is limited to 2.5 voxels and must have a clear entire segment.
  Surface normals, rather than one global wall plane, orient subsequent search and leaves.
- Only exposed faces of the root's material are eligible attachments. Free extension is
  bounded by `clamp(3 × spacing, 12, 64)` voxels from the last attachment; forks inherit
  that distance rather than renewing it. Blocked ordinary tips rotate their probe without
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
pruned=true upper_attached_removed=true stable_survivors=true before=100 after=7
waiting=true root=false frames=30 nodes=7
regrown=true from_cut=true fresh_ids=true
waiting=true root=true frames=30 nodes=1
verified prune=true wait=true regrow=true root_recovery=true finite=true
phase=complete failures=0
```

2026-09-23 local evidence: `target/climbing-validation/pruning/` contains the failing
pre-change pruning test, check/test/smoke logs and successful release review. The
previous test failed with `unsupported upper stem is still suspended`; it now passes.
Current exploration evidence is in `target/climbing-validation/exploration/`. In addition
to the pruning review, run each actual terrain fixture (use longer auto-exit if needed):

```sh
for scene in flat hole outward inward slope ground; do
  RE_FLORA_CLIMBING_REVIEW=$scene cargo run --release -- --hidden --mute --perf --auto-exit 12
done
```

Each must print `fixture=<name> verified=true collision_clear=true attached_height=...`
and a successful shutdown. The fixture review stops after 180 ready growth attempts,
requires an attachment above voxel Y=262, and checks all stem segments and attachment
materials against the current terrain export. These checks are not satisfied merely by
an airborne tip reaching above the obstacle. `RE_FLORA_CLIMBING_REVIEW=1` retains the
separate prune/wait/repair/root-recovery sequence.

Unit tests also cover both search directions on every fixture, ground-to-wall growth,
finite unsupported extension **across forks**, unrelated branch/tip retention, missing root support, buried stems,
material replacement, pending/stale snapshots, gaps between anchors/nodes, repeated
prune/regrow cycles, GUI button input and settings persistence.

`[CLIMBING][PERF]` now reports export, pruning/revalidation, growth, render preparation and
voxel-query counts. It excludes fixture edit/camera actions and is not a GPU/FPS measure.
**This scenario and algorithm differ from the old settling benchmark**; do not directly
pool or compare their workloads. Historical measurements/research are preserved in
[research/climbing_vine_tuning.md](research/climbing_vine_tuning.md).
