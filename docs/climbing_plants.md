# Climbing vine: search, attach, prune, regrow

The demo grows one persistent, unbranched vine. Missing support prunes the downstream
subtree, even if higher attachments still have wall behind them. A rooted surviving
stump can explore and regrow with fresh IDs without repairing the missing wall;
there are no falling detached remnants.

The continuous-stem model permits older stem to deform. Following visual approval,
it is now the only model: the old solver and experiment checkbox have been removed.
Older saved checkbox values are discarded without changing other user settings.

## Try it

1. Run this worktree with `cargo run --release`, then **R → Climbing Plants**.
   The demo wall and vine initialize automatically on a new session; there is no
   enable checkbox. Loading a saved world still clears the session-only vine and
   does not silently add a new test wall.
2. The vine uses distributed bending through attachments, independently timed exploration,
   gradual contact establishment, compliant established stem and short attachment roots.
3. **Test terrain** restarts immediately. **Climbing pole** authors a 20×20-voxel
   limestone post with a grounded footing; it tests contact/attachment rather than
   a separate twining algorithm. The playable demo uses a fixed code seed (3500)
   and searches counterclockwise in its local frame; direction is a typed code-level
   phenotype (`SearchDirection`), not a saved Debug checkbox. Reviews may explicitly
   choose another seed and clockwise search. **Restart wall and vine** repeats the demo;
   **Create vine wall and focus** starts an uncreated patch. **Focus vine** recenters it.
4. **Shoot exploration / flexibility** controls the existing bend/gravity response.
   **Search turn width** scales the rotating sweep (0–6× flexibility); **Search rotation
   rate** scales its phase speed (0.5–3×). At the default 10 growth attempts/sec a
   64-voxel free shoot lasts about 3.2 seconds, shorter than the old ~5.5-second turn;
   setting rate to 2× lets it complete a turn before exhausting that reach.
   **Unsupported search reach** controls how much unanchored stem may grow (16–96 voxels).
   These saved sliders change live without resetting geometry. Lowering reach below
   the existing free length holds new extension, but does not delete existing stem.
   **Growth attempts/sec** changes elongation independently of the oscillator.
   **Adhesion spacing** targets distance along the stem, not wall distance or search amplitude.
   Lip transitions may attach closer; the apex retains bending room.
5. The vine keeps attempting growth at the selected rate (at least 1/sec).
   Older saved zero rates are migrated to 1/sec. A surviving
   cut holds its base but can extend a new shoot; a root without support stays latent
   until its own support is repaired.
6. **3 / Dig**, LMB removes wall; **Shift + wheel** changes brush size. Repair the missing
   support if desired; a rooted stump can also search for a new route. **Prune highest
   attachment** / **Prune back to root** trim without
   editing terrain. **Disconnect root** stops growth until reset.
7. Optional markers: yellow attachment, blue next extension probe, orange retained bud,
   red unsupported root seed. **Blocked-tip test → Refill terrain through tip** inserts real
   limestone; removing it permits exploration to resume.

**Resets modify real terrain.** The patch is beside the startup tree around X=383/Z=300,
with a two-voxel footing embedded in current natural ground. Site discovery ignores
foliage, wood and authored limestone, and waits for available/current terrain. Reusing
that site prevents successive resets from stacking walls. Do not build anything you want
preserved inside this test patch.

All settings use the declarative Debug Save pipeline. Older GUI files
receive missing declarations without losing saved fields. **Plant history is session-only**;
world replacement clears it and does not silently author another wall.

## Continuous-body model

`src/climbing_plants/rod.rs` owns the mechanics; `rod/weight.rs` handles distributed
self-weight and long-span rotational relaxation; `rod/collision.rs` owns contact reactions. `growth.rs` remains authoritative for growth/contact candidates.

- Material is not regenerated: node IDs, rest lengths and parent relationships persist.
  A coupled three-node bend stencil spans attachments. Young material gradually remembers
  its shape; maturation slows near the apex instead of immediately hardening a stalled tip.
  Older material retains finite elastic resistance. This is an overdamped
  position-constraint model, not calibrated plant physiology, inertia, or an XPBD solver.
- A finite growing zone receives a smooth tangent/preferred-bend field. Its transported
  frame does not reset to each voxel face normal. The wall-clinging phenotype remembers
  its wall axis through sideways hole/stair contacts. Confirmed opposite-facing adhesion
  reverses the support bias without resetting phase; it is not an arbitrary pole twiner.
- The oscillator advances on a 20 Hz simulation clock independently of births. New material
  follows the existing tip tangent with a small tropic correction, rather than printing the
  rotating direction into each new segment. Growth and motion are interleaved on that same
  fixed tick so rendering cadence does not reorder them.
- Candidate contacts at least six rest-arc voxels behind the apex must persist for 0.35 s
  before becoming established. Rootlet reach extends two voxels beyond stem contact clearance;
  it does not enlarge the stem collider. Lip transitions permit two-voxel attachment intervals;
  otherwise the target interval is capped at 58 to preserve apex room within the air budget.
  Rootlets bridge roughness without forcing the main centerline onto every attachment point.
  Established footprints remain fixed; the connected stem has bounded positional compliance.
  Rootlet centerlines are checked against terrain. Decorative rootlet thickness/leaf boxes
  are not independently swept collision bodies.
- Local exposed-voxel constraints act on both ends of each segment/expanded-voxel intersection
  interval, not just stem endpoints or interval midpoints. Active contacts are refreshed while
  solving. Angular load proposals remove inward contact velocity while retaining sliding.
  Faces are selected from the previously safe side, including on thin exported voxel shells.
  Final whole-segment and swept convex-hull checks still decide acceptance.
- Length error is bounded to 0.002 voxel, displacement to 0.6 voxel per motion quantum,
  and established attachment displacement to 0.3 voxel. If no bounded, clear pose is found,
  geometry is held rather than publishing an unconverged or penetrating solution.
- A nominal growth step is 2 voxels; contact correction is capped at 2.5. New contact growth
  cannot introduce a bend above 25 degrees relative to the incoming segment. This is **not**
  a hard bound on later deformed joint angles: obstacle corners can still produce tighter bends.
- Self-weight bends the unsupported tail without stretching it; tangent-following births
  may point down. Ordinary terrain contact supports, blocks or permits sliding without
  becoming adhesion or triggering pruning. A reachable lip/far-side surface can subsequently
  establish rootlets; reattachment is not guaranteed.
- Unsupported extension defaults to 64 voxels of rest arc and is adjustable live from
  16 to 96. Deformation cannot replenish it; a new attachment resets the free arc.
  This is an artistic bounded search, not global terrain pathfinding or a guarantee
  that every seed reaches the top.

The former young-span solver and phase-per-growth strategy have been deleted. Shared
whole-segment sweep safety now belongs to `sweep.rs`. Historical descriptions and
measurements remain in [research/climbing_vine_live_shoot.md](research/climbing_vine_live_shoot.md);
they do not describe an available runtime mode.

## Shared history and safety

- A gameplay cap of 512 voxels of **current surviving main-stem rest arc** limits total
  growth; pruning frees that length budget. This is not a botanical measurement or the
  separate adjustable unsupported search budget. At most 512 live nodes remain as an
  independent storage bound. Pruning compacts storage without reusing IDs; indices are
  remapped together. Explicit-tree pruning guardrails remain, although normal growth
  never branches.
- Root-to-tip revalidation checks actual recorded contacts/backing, established anchor
  material, clear stem geometry, and attachment rootlet centerlines.
  Pre-existing gaps do not invent backing dependencies. Missing/changed material or a buried
  stem removes the first invalid step and all descendants.
- A cut preserves the root-connected upstream stem and its recorded dependencies, and
  starts a new exploratory tip at the stump. Only the surviving attachment history
  remains dark; the free span above the last anchor stays flexible and light green.
  The rod's retained base is held through that anchor, not through the stump. It does
  not restore a removed anchor or pass through missing terrain. The free-search budget
  is recomputed from the latest surviving anchor to the stump. Whole-root-support loss leaves a latent
  root seed; repairing that root permits regrowth. New nodes receive fresh IDs.
- Terrain snapshots are transactional: unavailable or stale queries do not commit pose,
  material memory, phase, contacts, RNG, pruning, or growth. Collision cache reuse checks
  bounds, dependencies and readiness. Loading a world clears history/cache.

## Validation

```sh
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5

# Seven real fixtures and the separate pruning/root-recovery scenario.
for scene in flat hole outward inward slope ground pole 1 overhang; do
  RE_FLORA_CLIMBING_REVIEW=$scene \
    cargo run --release -- --hidden --mute --perf --auto-exit 12
done
cargo run --release -- --tail-latest-log 200
```

Reviews fix seed 42, clockwise, spacing 16, flexibility 1, one growth attempt and two
50 ms motion ticks per sample. Each fixture needs a supported attachment at least 70 voxels
above ground after 180 attempts, finite geometry, clear stems and authoritative support.
The additional `overhang` review uses seed 3500, counterclockwise, spacing 10, flexibility 2,
360 samples and a side-facing camera; it checks that the above-wall shoot actually drops.
Reviews verify bounded stretch/attachment displacement and actual established-stem motion. Pruning review requires regrowth during 30 frames with the wall gap still open,
removal of upper attached descendants, fresh IDs, and separate root-support recovery.
Require completion markers and inspect logs for errors—not just a zero process exit.

Latest implementation evidence, quantitative A/B results and unresolved visual/performance
limits: [research/climbing_vine_continuous_stem.md](research/climbing_vine_continuous_stem.md).
Biological motivation and limitations: [research/climbing_vine_support_search.md](research/climbing_vine_support_search.md).
The user approved the continuous-body direction; performance acceptance remains separate.
[Self-weight implementation and validation](research/climbing_vine_self_weight.md) records
passing droop/downward-growth/lip/back-side regressions, native screenshots and current costs.
The shoot still initially extends well above the wall before drooping; this is not a wall-height
stop. Visual tuning and performance acceptance remain open.
[Earlier overhang diagnosis](research/climbing_vine_overhang.md) preserves the original failures
and rejected candidates; its formerly ignored diagnostics are now normal passing tests.
