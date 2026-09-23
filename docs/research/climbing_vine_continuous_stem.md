# Continuous, terrain-aware climbing stem: first visual candidate

Date: 2026-09-23. Follows `c1d8c897` and the user's approval of the
[support-search research](climbing_vine_support_search.md). The old requirement that
established geometry never move is intentionally relaxed, not silently broken.

## Delivered behavior

The saved Debug checkbox `climbing_continuous_stem` selects the model at runtime.
Checked (new declaration default) enables the experiment; unchecked retains the original
`shoot.rs` behavior. Changing it requests a same-seed fixture restart, using the same
selection-observation path as terrain/seed/winding changes. There is no CLI-only A/B or
unsaved App-only setting. The existing user values remain seed 3500, counterclockwise,
recessed wall, flexibility 2, speed 10, spacing 10, markers on.

The new model has:

- one persistent main shoot, distributed bending across attachments, and finite older-stem
  stiffness; no old-node regeneration or automatic branches;
- independently timed exploration, a transported frame and a smooth finite growing-zone
  tangent/preferred-curvature field, instead of printing the current rotary direction;
- material memory that gradually sets while bending resistance increases with age;
- persistent candidate contact followed by established compliant attachment behind the apex;
- short visible attachment-root pairs, independent of the main shoot topology;
- collision constraints on segment interiors as well as whole-segment/swept-volume rejection;
- the existing rooted pruning, waiting, repair and monotonic identity rules.

This is a bounded **overdamped position-constraint rod approximation**, not a calibrated
botanical simulation, inertial rod, or XPBD implementation. It is specifically a wall-clinging
phenotype; its original exposed side is retained through local hole/stair normals rather
than claiming arbitrary pole winding or global support navigation.

## Ownership and numerical choices

- `src/climbing_plants/rod.rs`: material/oscillator/pending-contact state and transactional
  mechanical update. `Rod` belongs to `Plant`, not the App. Three-node bend gradients cross
  attachment locations; local anchors do not terminate the stencil.
- `src/climbing_plants/rod/collision.rs`: active exposed-voxel constraints. Both endpoints
  of the segment/expanded-voxel intersection interval are constrained. Candidate constraints
  refresh during solving; final clear-segment and swept-hull tests remain authoritative.
- `growth.rs`: one contact-candidate and backing implementation shared by both models.
  Continuous births inherit their actual incoming tangent. Contact projection cannot add
  a new joint above 25 degrees; subsequent deformation can bend it further at obstacles.
- `Anchor::surface_position`: authoritative fixed rootlet footprint shared by rendering,
  support revalidation and pose acceptance. Rootlet centerlines cannot pass through an
  intervening solid; established support cells remain dependencies even if provisional
  body contact changes.
- The App orders continuous growth and motion on fixed 50 ms ticks. Rendering cadence
  no longer changes their interleaving. The baseline keeps its original update order.

Artistic constants, **not values validated by botanical papers**:

| Quantity | Choice |
| --- | --- |
| Growing-zone arc length | 28 voxels |
| Material maturation | 3 simulation seconds |
| Bend relaxation coefficient | Smoothly rises from 0.35 to 0.65 |
| Exploration period | Seed-dependent 4.8–6.2 simulation seconds; independent of growth rate |
| Contact dwell | 0.35 simulation seconds |
| Additional attachment-root reach | 0.8 voxel beyond radius + 0.18 growth clearance |
| Continuous-mode unsupported extension cap | 64 voxels of rest arc, independent of spacing |
| Per-quantum displacement bound | 0.25 voxel |
| Established attachment compliance bound | 0.3 voxel |
| Absolute segment-length error tolerance | 0.002 voxel |

The initial solve uses 16 bounded iterations. Each of at most ten halving line-search
trials restores length/contact constraints with at most 128 passes; unconverged, nonfinite,
penetrating or excessive-motion poses are not published. All phase, material, contact and
RNG updates roll back on unavailable/stale terrain, including late query/freshness failures.

**Retained-cut exception:** the surviving base remains fixed after a cut, including after
repair. Its stored restart step is in absolute world coordinates. Ordinary established
stem is compliant, but repair does not reinterpret that cut or move its support references.
This explicit exception preserves the player's existing cut/wait/repair contract.

`Node.fixed` still distinguishes established history for baseline mechanics and coloring;
in continuous mode it is not itself a motion constraint. The root and retained-cut lock
are the actual immutable bases. Decorative leaves/rootlet thickness are not independent
swept collision bodies. This work does not add self-collision, branching, or save plant history.

## Reproductions and corrections

Local intermediate evidence is under `target/climbing-validation/continuous-stem/`:

1. With seed 3500, counterclockwise, recessed wall, spacing 10 and flexibility 2, the original
   model reaches **54.311 degrees** maximum adjacent-segment angle in the deterministic
   180-attempt test. The final continuous model reaches **15.505 degrees**. Existing
   established-node motion is nonzero (maximum per sample about 0.020 voxel). The regression
   also requires exact same-seed replay, collision clearance, bounded length/drift, and an
   angle reduction versus original. This is a core-model comparison, not a measured FPS result.
2. Early candidate contacts disappeared under slight compliant motion. Attachment organs now
   have explicit short reach separate from the main stem's collision radius; the eligible
   search bounds expand with that reach. Main-stem collision clearance was not weakened.
3. Interpolating two identical fixed positions introduced floating-point changes to retained
   stumps. Fixed entries now copy their old positions exactly, rather than using a lerp.
4. A binary “overhead obstacle means move horizontally” rule launched shoots away from voxel
   slopes. The local guide now selects the most upward clear direction from a bounded fan
   and transports the previous frame smoothly into it.
5. Corner contacts initially froze the whole pose despite a clear free shoot. Endpoint-only
   body reactions missed segment interiors. An intermediate interval-**midpoint** reaction
   still missed penetration at an interval endpoint near a hole ceiling. A translated,
   clockwise hole test reproduced this as `one old contact froze the whole exploring body`
   (`hole-freeze-red.log`). Constraining both interval ends fixed it. Final real-terrain
   hole review observes motion in 179/180 samples, rather than the intermediate 50/180.
6. Provisional rootlet contacts are not infinite body half-spaces. Removing that conflation
   keeps obstacle geometry in the collision-constraint owner instead of projecting the main
   stem against a face it only reaches through an attachment root.

The intermediate hole run's 32.6 ms pose p95 belongs to the rejected midpoint solver, not
this final candidate. No temporary debug prints remain in source.

## Final validation

- `cargo fmt --check`, `cargo check`, full `cargo test`: **1118 + 4 passed, 2 ignored**.
  Normal full-suite duration was approximately 19 seconds; the tests are deterministic
  logic checks, not GPU/window benchmarks.
- Release hidden/muted smoke passed; per-worktree run log inspected using the built-in
  `--tail-latest-log` helper. Final logs contain no `ERROR`, `panicked` or `VUID` markers.
- All six continuous real-terrain reviews passed, with actual existing-stem motion,
  bounded length and anchor displacement, one tip, finite geometry and clear segments.
- Continuous pruning review: **64 → 6 nodes**, upper attached descendants removed, 30
  unchanged waiting frames, repair with new IDs, whole-wall loss to one latent seed,
  30-frame root wait and successful root recovery. Original-mode pruning also passes.
- GUI tests cover saved false/true A/B values, missing-field migration and immediate restart.
  Missing defaults are compared against compiled declarations rather than historical
  hardcoded seed/winding settings. No existing user value was overwritten.
- Native hidden screenshots: `original-inward.png`, `continuous-inward.png`, `user-3500.png`.
  The final user-settings capture was inspected for the smoother single stem. Earlier A/B
  captures were inspected too; these are not animation approval. No visible game was launched.

Final Release fixture review uses seed 42, clockwise, spacing 16, flexibility 1, 180 attempts
and two 50 ms motion quanta per sample. Ground Y=129; required attached height is Y=199.

| Fixture | Nodes | Anchors | Highest attachment Y | Maximum joint angle over run |
| --- | ---: | ---: | ---: | ---: |
| flat | 79 | 7 | 228.955 | 4.208° |
| hole | 83 | 6 | 232.912 | 41.783° |
| outward | 88 | 5 | 229.116 | 32.451° |
| inward | 84 | 6 | 237.003 | 10.314° |
| slope | 89 | 6 | 232.988 | 14.081° |
| ground | 79 | 7 | 222.807 | 7.535° |

**Visual acceptance remains open.** In particular, hole/ledge corners still produce tighter
bends than plain/recessed walls. The reference hole reaches about 42 degrees; do not claim
that all possible sharp bends have been eliminated. The new wall-clinging appearance is
also deliberately less wavy than the baseline. User feedback should decide the next tuning,
not a claim that lower joint-angle numbers alone prove natural motion.

### Release CPU observations, not performance acceptance

From `final-<fixture>.log`: 180 samples with `quanta=1`, nearest-rank p95. Pose includes two
motion quanta. Total includes export, revalidation, growth, pose, render preparation and
review overhead, but excludes fixture authoring, camera operations, GPU and whole-frame time.

| Fixture | Growth p95 µs | Pose p95 µs | Total p95 µs | Total maximum µs |
| --- | ---: | ---: | ---: | ---: |
| flat | 8 | 1799 | 1879 | 12272 |
| hole | 9 | 3034 | 4027 | 22833 |
| outward | 9 | 1869 | 2405 | 17857 |
| inward | 9 | 1830 | 2192 | 12116 |
| slope | 9 | 1552 | 2887 | 16280 |
| ground | 8 | 1550 | 1662 | 9435 |

Whole-body solving costs more than the original young-span model. Export/update spikes
still reach **22.8 ms** in these runs. These different growth histories are not a controlled
speedup benchmark. No FPS budget or performance acceptance is claimed; optimization follows
visual feedback. The bounded solver favors correctness over publishing an invalid pose.

Run recipes and player controls are in [the feature guide](../climbing_plants.md).
