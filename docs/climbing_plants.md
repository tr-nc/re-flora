# Climbing vine: search, attach, prune, regrow

The demo grows one persistent, unbranched vine. Missing support prunes the downstream
subtree, even if higher attachments still have wall behind them. A retained cut waits
for repair and regrows with fresh IDs; there are no falling detached remnants.

The continuous-stem model permits older stem to deform. Following visual approval,
it is now the only model: the old solver and experiment checkbox have been removed.
Older saved checkbox values are discarded without changing other user settings.

## Try it

1. Run this worktree with `cargo run --release`, then **R → Climbing Plants**.
2. The vine uses distributed bending through attachments, independently timed exploration,
   gradual contact establishment, compliant established stem and short attachment roots.
3. **Test terrain**, **Vine seed**, and **Clockwise tip search** also restart immediately.
   **Restart wall and vine** repeats the current seed; **New random seed** changes the saved
   seed. **Create vine wall and focus** starts an uncreated patch. **Focus vine** recenters it.
4. **Shoot exploration / flexibility** changes the response live. In continuous mode,
   reducing it suppresses the exploration amplitude and gravity load, not the elasticity
   or collision constraints. **Growth attempts/sec** changes elongation, not the continuous
   oscillator's period. **Adhesion spacing** targets distance along the stem; it is neither
   wall distance nor exploration radius. Continuous mode has an independent bounded air budget.
5. **Pause vine growth** holds extension and exploration phase, but allows settling and
   terrain-triggered pruning. A waiting cut is completely held. A repaired cut deliberately
   retains a fixed surviving base so its stored absolute restart step stays valid.
6. **3 / Dig**, LMB removes wall; **Shift + wheel** changes brush size. Repair the missing
   support to regrow. **Prune highest attachment** / **Prune back to root** trim without
   editing terrain. **Disconnect root** stops growth until reset.
7. Optional markers: yellow attachment, blue next extension probe, orange retained bud,
   red unsupported root seed. **Blocked-tip test → Refill terrain through tip** inserts real
   limestone; removing it permits repair-gated regrowth.

**Resets modify real terrain.** The patch is beside the startup tree around X=383/Z=300,
with a two-voxel footing embedded in current natural ground. Site discovery ignores
foliage, wood and authored limestone, and waits for available/current terrain. Reusing
that site prevents successive resets from stacking walls. Do not build anything you want
preserved inside this test patch.

All settings use the declarative Debug Save pipeline. Older GUI files
receive missing declarations without losing saved fields. **Plant history is session-only**;
world replacement clears it and does not silently author another wall.

## Continuous-body model

`src/climbing_plants/rod.rs` owns the mechanics; `rod/collision.rs` owns its
local obstacle constraints. `growth.rs` remains authoritative for growth/contact candidates.

- Material is not regenerated: node IDs, rest lengths and parent relationships persist.
  A coupled three-node bend stencil spans attachments. Young material gradually remembers
  its shape; older material retains finite elastic resistance. This is an overdamped
  position-constraint model, not calibrated plant physiology, inertia, or an XPBD solver.
- A finite growing zone receives a smooth tangent/preferred-bend field. Its transported
  frame does not reset to each voxel face normal. The wall-clinging phenotype remembers
  its original exposed side through hole/stair contacts; it is not an arbitrary pole twiner.
- The oscillator advances on a 20 Hz simulation clock independently of births. New material
  follows the existing tip tangent with a small tropic correction, rather than printing the
  rotating direction into each new segment. Growth and motion are interleaved on that same
  fixed tick so rendering cadence does not reorder them.
- Candidate contacts behind the apex must persist before becoming established. Short rootlets
  bridge surface roughness without forcing the main centerline onto every attachment point.
  Established footprints remain fixed; the connected stem has bounded positional compliance.
  Rootlet centerlines are checked against terrain. Decorative rootlet thickness/leaf boxes
  are not independently swept collision bodies.
- Local exposed-voxel constraints act on both ends of each segment/expanded-voxel intersection
  interval, not just stem endpoints or interval midpoints. Active contacts are refreshed while
  solving. Final whole-segment and swept convex-hull checks still decide acceptance.
- Length error is bounded to 0.002 voxel, displacement to 0.25 voxel per motion quantum,
  and established attachment displacement to 0.3 voxel. If no bounded, clear pose is found,
  geometry is held rather than publishing an unconverged or penetrating solution.
- A nominal growth step is 2 voxels; contact correction is capped at 2.5. New contact growth
  cannot introduce a bend above 25 degrees relative to the incoming segment. This is **not**
  a hard bound on later deformed joint angles: obstacle corners can still produce tighter bends.
- Unsupported extension is capped at 64 voxels of rest arc in continuous mode. Deformation
  cannot replenish it. This is an artistic bounded search, not global terrain pathfinding or
  a guarantee that every seed reaches the top.

The former young-span solver and phase-per-growth strategy have been deleted. Shared
whole-segment sweep safety now belongs to `sweep.rs`. Historical descriptions and
measurements remain in [research/climbing_vine_live_shoot.md](research/climbing_vine_live_shoot.md);
they do not describe an available runtime mode.

## Shared history and safety

- At most 512 live nodes. Pruning compacts storage without reusing IDs; indices are remapped
  together. Explicit-tree pruning guardrails remain, although normal growth never branches.
- Root-to-tip revalidation checks actual recorded contacts/backing, established anchor
  material, clear stem geometry, and (in continuous mode) attachment rootlet centerlines.
  Pre-existing gaps do not invent backing dependencies. Missing/changed material or a buried
  stem removes the first invalid step and all descendants.
- Buds retry the exact first severed step. A waiting cut commits neither geometry nor RNG
  progression; the surviving base stays fixed after repair. Whole-wall loss leaves a latent
  root seed. Repair creates new nodes, not resurrected identities.
- Terrain snapshots are transactional: unavailable or stale queries do not commit pose,
  material memory, phase, contacts, RNG, pruning, or growth. Collision cache reuse checks
  bounds, dependencies and readiness. Loading a world clears history/cache.

## Validation

```sh
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5

# Six real fixtures and the separate pruning/root-recovery scenario.
for scene in flat hole outward inward slope ground 1; do
  RE_FLORA_CLIMBING_REVIEW=$scene \
    cargo run --release -- --hidden --mute --perf --auto-exit 12
done
cargo run --release -- --tail-latest-log 200
```

Reviews fix seed 42, clockwise, spacing 16, flexibility 1, one growth attempt and two
50 ms motion ticks per sample. Each fixture needs a supported attachment at least 70 voxels
above ground after 180 attempts, finite geometry, clear stems and authoritative support.
Reviews verify bounded stretch/attachment displacement and actual established-stem motion. Pruning tests require 30 unchanged
waiting frames, removal of upper attached descendants, repair with fresh IDs, and root recovery.
Require completion markers and inspect logs for errors—not just a zero process exit.

Latest implementation evidence, quantitative A/B results and unresolved visual/performance
limits: [research/climbing_vine_continuous_stem.md](research/climbing_vine_continuous_stem.md).
Biological motivation and limitations: [research/climbing_vine_support_search.md](research/climbing_vine_support_search.md).
The user approved the continuous-body direction; performance acceptance remains separate.
The next visual issue is excessive upright extension above walls, not addressed by removing
this model's predecessor.
