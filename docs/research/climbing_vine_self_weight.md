# Continuous vine self-weight, contact and downward growth

Date: 2026-09-24. Builds on `f7ff63ec`; replaces the pending status in
[the earlier overhang diagnosis](climbing_vine_overhang.md).

## Delivered behavior

There is still one continuous-body implementation, with no experiment switch. Existing
stem nodes deform; births follow their actual tangent, including downward directions.
The unsupported tail bends under distributed self-weight, while older material retains
finite bending resistance. Contact can support or slide without becoming adhesion.
Suitable persistent rootlet contacts can subsequently attach to a roof or its far side.
Neither future attachment nor successful climbing for every seed is guaranteed.

`rod/weight.rs` supplies an overdamped rotational proposal: each visited joint rotates
its entire distal chain, preserving segment lengths. Distributed rest-length mass,
lever arms, elastic strain, the finite growing-zone guide and provisional rootlet
forces contribute torque. Four joints are visited per tick, with a persistent cursor.
Contact Jacobians remove inward angular motion rather than discarding legal sliding
components. Segment and swept-interior checks reject unsafe proposals. The existing
coupled local solver then restores lengths, contacts and compliant attachment bounds.
Published displacement is bounded to 0.6 voxel per 50 ms tick; attachment drift remains
0.3 and length error 0.002. Unknown/stale terrain rolls back the cloned state, including
the rotational cursor, maturation and contact progress.

This is an artistic overdamped model, not an inertial rod, XPBD solver or biological
calibration. Rigidity 12000 and its maturity multiplier are simulation constants, not
measured tissue moduli. Flexibility still scales exploration and gravitational loading.

### Attachment and growing-zone corrections

Simply adding weight pulled the young slope shoot away before its first attachment.
Rootlets now reach two voxels beyond stem-contact clearance, independently of the
unchanged 0.65 stem radius. Their visible connectors and fixed footprints remain real,
terrain-checked geometry; there is no attraction through intervening material.

Attachments reserve **six voxels of free apical rest arc**, not merely two nodes:
contact-corrected segments can be very short. A minimum-node-count rule could immobilize
the apex just below a hole ceiling. Contact must still persist for 0.35 s. Target spacing
is capped at 58 within the 64-voxel air budget. Transitions onto/off an upward-facing lip
allow two-voxel spacing, so the first accessible back-side contact is not vetoed by the
usual interval. These are explicit changes to spacing semantics, not extra branches.

Material ages more slowly near the tip, at `dt * clamp(distance_from_tip / 28, 0, 1)`;
age never decreases. A blocked apex therefore retains shape adaptation. The transported
frame keeps its wall axis through sideways stair/hole faces; an established opposite
face reverses the support bias without resetting the oscillator. A bounded local fan
looks for ceiling escape/headroom without global pathfinding.

### Real-terrain defect exposed by validation

The first native hole run failed despite the dense analytic fixture passing. Comparing
native and test positions isolated their first divergence at sample 26. Direct voxel
queries showed omitted wall-interior voxels in the native export. The resulting thin
shell exposes additional inward faces. An intermediate penetrating solver proposal
could select such an inside face and be pushed through the shell; final sweep rejection
prevented publication but left the shoot stuck.

`rod/collision.rs` now admits reaction faces from the **previously feasible side**.
It does not infer the approach side from the already-penetrating proposal. The translated
hole regression now models a surface shell inside the app's bounded two-tick snapshot,
and checks both late motion and upper-wall attachment. The original climbing and angle
assertions were not relaxed. The failed native run remains in `native-hole.log`; only
`final-hole.log` is acceptance evidence.

## Validation

Artifacts are local under `target/climbing-validation/weight/`.

- `cargo fmt --check`, `cargo check`, `cargo test`: **1120 + 4 passed, 2 ignored**.
  The two overhang diagnostics are no longer ignored; the remaining ignores predate this work.
- Downward-tangent birth, self-weight droop, finite unsupported reach, replay, retained IDs,
  terrain transaction rollback, exact cut waits, repair dependencies and root recovery pass.
- Dedicated non-adhesive lip test uses a non-root-eligible material: ordinary contact supports
  the stem and permits tangential movement without creating an attachment.
- A broad wall (`z=264..306`) permits roof and negative-Z back-side rootlets in one growth
  history. This demonstrates a feasible case, not guaranteed reattachment on every wall.
- Both winding directions retain the existing six-fixture climbing checks. Seed 3500 retains
  its unchanged <35-degree adjacent-angle guard and actual established-stem movement.
- Hidden muted Release smoke, all six real fixtures, pruning/wait/repair/root-recovery and
  the new `overhang` review pass. Final logs have no runtime ERROR, panic or VUID markers;
  all have shutdown `phase=complete failures=0`. Latest-log tail was inspected.
- Native overhang: peak tip Y **284.090**, final tip Y **219.682**, wall top Y=237;
  `drooped=true collision_clear=true`, 87 nodes and 11 attachments. Maximum adjacent angle
  over this native run was **24.517°**; the six native fixtures ranged from **6.857° to
  17.859°**. Motion was observed in 359/360 overhang samples and 179/180 per fixture.
- No generated files or saved GUI values changed. No visible/manual game was launched.

### Native appearance

Captured 24 native Release frames with:

```sh
RE_FLORA_CLIMBING_REVIEW=overhang target/release/re-flora --hidden --mute \
  --screenshot player-default target/climbing-validation/weight/overhang-view.png \
  --screenshot-delay 1.2 --screenshot-sequence 24 0.25 --auto-exit 10
```

Inspected frames 3, 7 and 20: an initially rising free shoot becomes a downward arch over
the wall, then settles lower. The side camera still occludes part of the far-side stem;
collision assertions, not that occlusion, establish terrain clearance. This accelerated
review advances two fixed simulation ticks per rendered sample, not ordinary real-time
playback. These frames do not constitute user visual approval.

**Remaining visual limitation:** the shoot can still rise roughly 47 voxels above this
wall before bending down. The change fixes indefinite upright posture, not an immediate
wall-top fold. No artificial wall-height stop or prescribed drooping path was introduced.
Decorative leaf boxes/rootlet thickness and stem self-collision are not independently solved.

### Release-app cost, not performance acceptance

Nearest-rank p95 and maximum from `final-<scene>.log`, considering only growing samples
(`quanta=1`). Six fixtures have 180 samples; overhang has 360. Microseconds:

| Scene | Pose p95 | Total p95 | Total maximum |
|---|---:|---:|---:|
| flat | 2570 | 2837 | 15228 |
| hole | 2263 | 3601 | 24388 |
| outward | 2409 | 3949 | 20722 |
| inward | 2478 | 3299 | 16978 |
| slope | 1913 | 2950 | 20886 |
| ground | 2227 | 3560 | 20492 |
| overhang | 3199 | 3323 | 24383 |

Pose includes two motion ticks. Total includes export, revalidation, growth, pose, render
preparation and review checks, but not GPU or whole-frame time. Trajectories differ from
the earlier implementation, so these are not controlled speedup measurements. Roughly
24 ms update spikes remain. Appearance feedback and performance acceptance are separate
next stages; neither test timing nor screenshots certify a performance budget.
