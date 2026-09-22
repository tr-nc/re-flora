# Climbing vine: prune, wait, regrow

The playable vine uses **rooted pruning**, not quasi-static falling. This replaces the
previous settling model at the user's request: a missing backing surface cuts off its
whole downstream branch, even when higher attachments still have wall behind them.
Other branches and the retained lower stem keep their geometry and history.

## Try it

1. Launch this worktree (`cargo run --release` for release playback).
2. **R → Climbing Plants → Create vine wall and focus**. The fixture edits real terrain
   at `(224,192,300)..(288,300,306)`. Do not reset over terrain you want to preserve.
3. Let it grow. **3 / Dig**, LMB removes wall; **Shift + wheel** adjusts brush size.
   When the edited terrain becomes current/ready, the first unsupported step and its
   descendants disappear. There is no need to disconnect the root or wait for gravity.
4. The retained cut becomes a regrowth bud. The cut's first step waits while its wall
   is missing or blocked; repairing the gap allows growth from that exact cut.
5. **Pause vine growth** stops extension, not terrain-triggered pruning. Turn it off
   to regrow. **Prune highest attachment** / **Prune back to root** let you trim without
   editing terrain; intact wall permits immediate regrowth.
6. **Focus vine** recenters the remaining skeleton. Reset requires confirmation.
   **Disconnect root** explicitly stops regrowth until reset; it is not a fall command.
7. Optional markers: yellow attachment, orange regrowth bud, red unsupported root seed.
   **Blocked-tip test → Refill terrain through tip** embeds real limestone in a tip;
   the blocked stem is pruned. Remove that inserted material to unblock regrowth.

Enable, pause, growth speed, attachment spacing and marker settings use the declarative
Debug **Save** path. **Plant history is session-only**, not part of terrain snapshots.
Loading/replacing the world clears the vine and cache without automatically creating a wall.

## Model and safety

- At most 512 **live** nodes and four tips. Live storage compacts after pruning; node IDs
  are monotonic and are not reused. Parent, anchor and tip storage indices are remapped
  together. Leaf placement/color derives from stable IDs so unaffected branches do not pop.
- Root-to-tip checks include the continuous backing line of every stem edge, not just
  sparse attachment markers. Holes between attachments and between sampled nodes count.
  Missing material, material replacement or a buried stem prunes the first invalid edge.
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
- Growth currently follows the original flat-wall rule; rotating exploration and complex
  support fixtures are the next integration step. Do not infer arbitrary-surface climbing
  from the pruning tests.

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
Unit tests also cover unrelated branch/tip retention, missing root support, buried stems,
material replacement, pending/stale snapshots, gaps between anchors/nodes, repeated
prune/regrow cycles, GUI button input and settings persistence.

`[CLIMBING][PERF]` now reports export, pruning/revalidation, growth, render preparation and
voxel-query counts. It excludes fixture edit/camera actions and is not a GPU/FPS measure.
**This scenario and algorithm differ from the old settling benchmark**; do not directly
pool or compare their workloads. Historical measurements/research are preserved in
[research/climbing_vine_tuning.md](research/climbing_vine_tuning.md).
