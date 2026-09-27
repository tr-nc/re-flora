# Circumnutation and terrain-aware vine exploration

Date: 2026-09-23. Implemented on `grepping-plants`, following the rooted-pruning change
`2bc5c2e2`. This note separates botanical evidence, the game's abstraction, and measured
validation; it does not reuse the old falling-solver benchmark as a baseline.

**Historical implementation snapshot:** the implementation, fixtures, validation and CPU
numbers below describe `3cdbc513`, before grounded placement and single flexible-shoot
support. Botanical sources remain applicable; current behavior and new measurements are
in [climbing_vine_live_shoot.md](climbing_vine_live_shoot.md). In particular, the old
four-tip/immobile-stem description and fixed world coordinates are no longer current.

## Primary-source findings

1. **Rotating exploration is a useful model, but contact and attachment are separate.**
   *Autonomously shaping natural climbing plants: a bio-hybrid approach* (2018), §3.1.2,
   describes growing tips moving on helical trajectories around their mean growth direction,
   and bean shoots combining upward growth, light response and circumnutation. The paper's
   experiments concern twining beans, not an adhesive ivy wall-covering algorithm.
   [Full paper, §3.1.2 and control experiments](https://pmc.ncbi.nlm.nih.gov/articles/PMC6227980/).
2. **Neither clockwise nor counterclockwise is a universal rule.** *Decision-Making
   Underlying Support-Searching in Pea Plants* (2023), §2.1, reports clockwise,
   counterclockwise and mixed patterns. In its eight-plant single-support condition the
   counts were respectively two, two and four. Its support-choice experiment also shows
   why reaching a support should not automatically imply that it is suitable. These
   results concern pea tendrils; they are not universal numerical parameters for vines.
   [Full paper, §2.1 and introduction](https://pmc.ncbi.nlm.nih.gov/articles/PMC10143786/).
3. **Persistent local growth interacting with complex supports is a demonstrated graphics
   approach.** Hädrich et al., *Interactive Modeling and Authoring of Climbing Plants*
   (Eurographics 2017), represents plants as connected particles carrying biological and
   physical state and demonstrates editing in changing environments. Our implementation
   borrows the persistent-history/local-interaction idea, not its physics solver or its
   performance claims. [Authors' project page](https://storage.googleapis.com/pirk.io/projects/climbing_plants/index.html),
   [paper DOI](https://doi.org/10.1111/cgf.13106).

**Design inference:** use a bounded rotating search and explicit surface-contact tests.
Keep the user-approved subtree pruning and retained-bud regrowth. Do not reintroduce
falling physics or imply that circular tip searching is the attachment mechanism of every
wall-clinging species. See also [the earlier species/mechanism research](procedural_climbing_plants.md).

## Original implemented abstraction

- A tip advances through 24 angular phases around an upward/local-tangent axis. Nominal
  extension is 2 voxels; contact correction is at most 2.5. Rotation direction is seeded;
  the saved root-direction checkbox selects the initial direction, and branches reverse
  handedness and offset phase. These numbers and branch rules are artistic choices.
- The rotating component has a modest bias toward support. Without that bias, projecting
  collisions against a wall displaced successive search circles outward: the flat-wall
  regression test reached only Y=210.45 in **attachments**, despite airborne tips extending
  higher. This was an algorithm defect, not successful climbing.
- Contact uses distance to an **actual exposed voxel face**, not just its infinite plane.
  The latter incorrectly attracted tips sideways back onto ledge edges. Under ceilings,
  the tip sweeps laterally while progressing tangentially toward the edge, then resumes
  upward search. Stem segments are never accepted through solid terrain.
- Every contacted node records its face/normal. An edge with originally continuous backing
  records that backing too. A free arc across an existing hole does not acquire imaginary
  backing. Removing recorded support prunes the downstream subtree, including otherwise
  attached descendants; a cut retries its stored step only after repair.
- Free extension is capped at `clamp(3 × spacing, 12, 64)` voxels from the last attachment.
  A fork inherits that distance: creating a branch cannot renew the air-growth budget.
  Ordinary blocked tips advance their probe phase without RNG consumption; unavailable
  terrain rolls back the whole growth quantum, including phase changes. Cut buds stay put.
- Normals orient subsequent searching, stem/leaf frames and attachment markers. A ground
  seed can leave its floor and locate a nearby wall. The plant still has a 512-live-node,
  four-tip cap; retained history never moves or gets regenerated.

## Original shared terrain fixtures

`src/climbing_plants/fixtures.rs` owns both the analytic voxel predicate and cuboid recipes
used by the real terrain-authoring transaction. A parity test checks the two descriptions.
The Debug choice selects the next created/reset scene:

| Scene | Geometry challenge |
| --- | --- |
| `flat` | Original six-voxel-thick wall |
| `hole` | Through-hole, 14 voxels wide × 18 high |
| `outward` | Upper wall projects eight voxels at Y=242 |
| `inward` | Upper face recedes eight voxels at Y=242 |
| `slope` | Voxel-stepped inclined wall: one outward voxel per two upward voxels |
| `ground` | Horizontal seed surface in front of the wall |

Creation clears/authors `(224,190,280)..(288,302,366)` in the actual world; it is not a
render-only mock. Scene/direction settings apply on reset and use normal Debug Save.
Old GUI files acquire defaults without losing their existing pause setting. A regression
  test first exposed a `Missing parameter: climbing_fixture` panic; the existing
  section-schema migration now supplies missing climbing controls from `config/gui.toml`.
  Loading a world clears the vine/cache and does not silently reseed it.

## Original validation

Local artifacts: `target/climbing-validation/exploration/` (not committed). Reproduce with
the commands in [the feature guide](../climbing_plants.md). Checked-in spacing is 16 voxels;
review mode fixes the seed and advances one growth attempt per ready frame.

- `cargo fmt --check`, `cargo check`, full `cargo test` (1086 + 4 passed, 2 ignored),
  release hidden/muted smoke.
- Analytic tests: all six shapes with both root directions; support above Y=262, intact
  self-revalidation, parent/ID invariants; grounded starts, mirrored search, bounded air
  extension including forks; existing pruning/repair/unknown-terrain regressions.
- Release real-terrain runs: `RE_FLORA_CLIMBING_REVIEW=<scene>`, 180 ready attempts, checking
  every stem segment and every attachment material against the current Contree export.
  All six emitted `verified=true collision_clear=true`, and all shut down with zero failures.
- Separate `RE_FLORA_CLIMBING_REVIEW=1`: 100 → 7 nodes after removing lower support; upper
  attached descendants removed, survivors unchanged, 30-frame waiting, regrowth with fresh
  IDs, whole-wall loss to one seed, and root recovery all passed.
- Screenshots from actual hidden native rendering: `hole-final.png`, `outward-final.png`,
  `inward-final.png`, `slope-final.png`, `ground-final.png`, captured with `--screenshot
  player-default ... --screenshot-delay 9`. These are candidate visuals, not user approval.

Final real-terrain results (`<scene>-final.log`):

| Scene | Live nodes | Attachments | Highest attachment Y |
| --- | ---: | ---: | ---: |
| flat | 274 | 16 | 297.184 |
| hole | 262 | 14 | 296.105 |
| outward | 288 | 18 | 295.562 |
| inward | 224 | 11 | 296.597 |
| slope | 225 | 21 | 279.170 |
| ground | 276 | 16 | 291.514 |

### Release CPU observations, not performance acceptance

From the same six final runs, 180 samples per scene with `quanta > 0`; nearest-rank p95.
`total_us` covers export/revalidation/growth/render preparation, not terrain authoring,
camera actions, GPU work or whole-frame time. Idle samples after review completion are excluded.

| Scene | Growth p95 µs | Total median µs | Total p95 µs | Total max µs |
| --- | ---: | ---: | ---: | ---: |
| flat | 11 | 25 | 2719 | 12946 |
| hole | 12 | 25 | 3681 | 17840 |
| outward | 11 | 27 | 2618 | 12237 |
| inward | 10 | 20 | 2768 | 12942 |
| slope | 19 | 21.5 | 1840 | 15395 |
| ground | 12 | 26 | 3925 | 17259 |

**Known issue:** expanding collision exports still creates CPU spikes. Low typical growth
cost does not erase the 12–18 ms total maxima. This is a changed algorithm/workload, not an
A/B optimization result; no previous FPS/regression threshold is claimed to have passed.
Visual review comes first, with performance optimization/acceptance a separate stage.

## Original boundaries

No elasticity, sagging or mechanically realistic detached remnants. No guarantee of
finding a route through arbitrary mazes or across large gaps; a bounded search can stop.
Only the root's material is eligible support. The collision checks validate stem segments;
coarse decorative leaf boxes are not individually collision-solved. Plant history remains
session-only. These fixtures demonstrate current behavior, not universal botanical realism.
