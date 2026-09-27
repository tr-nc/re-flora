# Unsupported growth above a wall: historical diagnosis

> Superseded by [the self-weight implementation](climbing_vine_self_weight.md).
> The two diagnostics below now run and pass in the normal suite. The remainder
> records the earlier failed attempts, not the current implementation status.

Date: 2026-09-24. Baseline: `5b990636` (continuous mechanics made permanent).
The user approved the continuous stem, requested removal of its predecessor/A/B UI,
and asked to investigate the upright shoot above a wall and collisions while draping.

## Delivered versus pending

- **Delivered:** one model, no experiment checkbox or environment-variable mode fork.
  Old saved checkbox values are removed on load/save; other user values are retained.
- **Delivered safety fix:** retained cuts now preserve the severed *established attachment*
  cell and rootlet footprint, separately from the node's current provisional contact.
- **Not delivered:** load-aware drooping, downward extension, landing on the wall lip, or
  growth down the far side. Failed mechanical candidates were reverted; the user's approved
  pose/growth parameters are unchanged. There is no hidden second solver.

## Reproductions

Two explicitly ignored **known-failing diagnostics** live in `rod/tests.rs`. They are not
passing regression coverage and must be enabled with the eventual mechanics fix:

```sh
cargo test stem_above_wall -- --ignored --nocapture
cargo test a_downward_tip -- --ignored --nocapture
```

The first is a minimal cantilever: the existing flat fixture, seed 3500, counterclockwise,
base six voxels below the top, spacing 10, flexibility 2, 360 growth attempts and two 50 ms
motion ticks per attempt (36 simulation seconds). It checks all segments, lengths, attachment
bounds and support cells throughout. A growing apex can point up even on a hanging shoot,
so the failure signal measures downward **body segments** and the tip's drop from its peak,
not a mandatory downward-facing apex.

Baseline output:

```text
wall_top=300 peak=346.99747 end=Vec3(263.7807, 345.85672, 277.8155) downward=false
unsupported stem remains an upright antenna instead of bending down
```

The deterministic debug test takes about 2.3 seconds here (timing is feedback-loop speed,
not app performance evidence). The second test constructs a clear downward tangent at a
wall and calls the actual growth path. It fails with `a safe downward extension was rejected`.

## Findings and rejected candidates

Observed code properties:

1. Gravity contributes only `0.018 * dt * flexibility` downward to the pose proposal.
   With 50 ms ticks this is 0.0009 voxel at flexibility 1, before relaxation and constraints.
   The 28-voxel growth zone separately receives a persistent upward-oriented guide/torque.
2. The 64-voxel unsupported arc cap limits extension; it is not a self-weight or carrying-
   capacity model. It can leave a long inclined column, not a draped stem.
3. Both free extension and contact-corrected births explicitly reject negative Y travel.
   Merely bending the old body does not enable downward growth along its tangent.
4. Local bend iterations alone have poor response to long-wavelength cantilever bending.
   In the attempted moment-based model, a global bending proposal produced substantial
   drooping where the local-only version barely changed the upright result. This is evidence
   about these candidates, not a proof that a specific numerical solver is required.

Tests, one-variable probes and rejected attempts are retained locally under
`target/climbing-validation/draping/`:

- `upright-red.log`, `downward-red.log`: baseline failures.
- `gravity-only-diagnostic.log`: raising only gravity to 18 moved the tip down about 35
  voxels and produced downward body segments. This was diagnostic, **not a viable fix**:
  subsequent tests found nearly 180-degree joints and loss of the reference outward climb.
- `load-tests.log`: distributed distal mass/lever-arm moments, finite rigidity and existing
  local bending preserved reference climbing but did not resolve the upright cantilever.
- `global-bend-tests.log`, `supported-bend-tests.log`, `multiscale-tests.log`: bounded global
  bending proposals could drape the minimal stem below the wall top, but introduced roughly
  43–46-degree joints in the previously improved seed-3500 scene and lost some reference
  slope/ledge reach. One variant also froze late hole motion. **Not accepted or shipped.**
- `rejected-load-attempt.patch` and `rejected-load.rs`: local diagnostic implementation,
  not an installable patch or a second supported mode.

No release performance conclusion or visual approval was obtained for these candidates.
Tests in an optimized test harness are not release-app benchmarks.

## Non-adhesive stem contact versus attachment

These must remain separate concepts:

- **Ordinary contact:** the entire stem has radius and collides with terrain. An upward
  wall-lip surface can carry weight; a vertical face blocks its inward normal component
  but must permit tangential sliding. Separation removes this reaction. It does not freeze
  a node, require glue, or automatically add an attachment.
- **Attachment:** an eligible surface and a persistent rootlet contact gradually establish
  local adhesion, which can hold against separation as well as support weight.

Existing safety already checks the main stem, not just attachment dots:

1. `rod/collision.rs` gathers exposed-voxel constraints along segment interiors, using both
   endpoints of each segment/expanded-voxel intersection interval.
2. Candidate motion is checked by `clear_segment` and `sweep::clear_sweep`; the latter covers
   the swept interior, not just the two end poses. Stem radius and the small collision skin
   stay authoritative.
3. A bounded line search can reduce the attempted movement; infeasible poses are held,
   never published through a wall. Unknown/stale terrain rolls back the transaction.

The next solver needs contact reactions to participate in **load distribution**, not just
reject a final penetrating pose. A collision-free held pose is safe but not proof that the
stem correctly rests or slides. Normal drooping into a wall should resolve through contact,
not trigger automatic pruning. Terrain edits that actually insert solid terrain into an
existing stem continue to use the intentional prune-and-repair policy. Decorative leaves
and rootlet thickness are not independently swept rigid bodies; stem safety must not be
misrepresented as collision solving every rendered primitive.

## Repair dependency defect found during larger motion

An established attachment can remain on cell A while the compliant stem's nearest provisional
contact changes to neighbouring cell B. Revalidation correctly pruned when A was removed,
but the old restart record retained only B. It could therefore resume immediately without
repairing A. This was reproduced independently of all gravity changes:

```sh
cargo test a_cut_requires_its_established -- --nocapture
# Before fix: bud bypassed its missing attachment via the neighbouring contact
```

`Restart` now stores the severed attachment cell and fixed rootlet footprint. Retry requires
that original material and a clear connector as well as the existing contact/backing/stem
checks. The test requires an unchanged waiting bud and fresh IDs after repair. This small
fix does not change ordinary growth or the user's approved appearance.

## Validation of the retained implementation

`cargo fmt --check`, `cargo check` and the normal suite pass: **1115 + 4 passed**.
There are four ignored tests: two pre-existing ones and the two explicitly known-failing
manual diagnostics above. Both new diagnostics were run manually again and still fail as
expected; the sagging problem is not fixed. Release hidden/muted smoke, the pruning/wait/
repair/root-recovery review and the recessed-wall review pass. The latter still reports
84 nodes, six anchors and highest attachment Y=237.003, matching the approved mechanics.
Run logs were inspected, including the built-in latest-log tail, without runtime ERROR,
panic or Vulkan validation markers. No visible game was launched or user settings changed.

## Next iteration acceptance

- Keep the approved wall-climbing curvature and both winding directions as guardrails;
  do not call a draping fix complete by deleting failed climbing/angle tests.
- Resolve long-span elastic response together with distributed self-weight and ordinary
  contact reactions. Do not simply multiply gravity, print a downward growth path, impose
  a wall-height stop, or turn the whole plant into a rope.
- Permit negative-Y births only as part of that validated revision, preserving tangent
  continuity, finite air budget, stable IDs, exact repair dependencies and swept clearance.
- Add separate scenarios for an unglued stem landing/sliding on a lip, contact then adhesion,
  a feasible far-side reattachment, and a gap with no reachable support. Success is possible
  reattachment, not a guarantee or attraction through unseen terrain.
- Inspect time-varying native Release output before visual acceptance; then measure release-
  app costs. This diagnosis does not certify a future solver's appearance or performance.
