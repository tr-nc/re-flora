# Native contact prediction compatibility module

This is a source backport inside the existing Rapier `NarrowPhase` dispatcher. There is one
`CollisionWorld`, one set of voxel shapes and dynamic bodies, and one Rapier solver. Shapes,
mass properties, material, CCD casts, query-only character movement and sleeping are unchanged.

## Sources and changes

- `manifolds.rs`: Dimforge **Parry 0.29.0**, upstream tag commit
  `8436f7c21875f8225bc1af4c84190aeefa1ae672`, file
  `src/query/contact_manifolds/contact_manifolds_voxels_shape.rs`.
  Applies [upstream d299cdca](https://github.com/dimforge/parry/commit/d299cdca86767680285bb838f37c699b59cfa17f):
  loosen the convex shape candidate AABB by the supplied prediction distance. This includes
  contact skin, as supplied by Rapier. The native canonical voxel topology, neighbor states,
  manifold cache and contact filtering algorithm are retained.
- `reduction.rs`: Dimforge **Rapier 0.34.0**, upstream tag commit
  `a1ef31035613154dfb97a9e1d480c6a5eb9d0010`, function `reduce_manifold_naive` in
  `src/geometry/manifold_reduction.rs`. Its point selection algorithm is unchanged. It runs at
  the dispatcher boundary using Rapier's **effective** prediction distance, which already
  includes skin. The original geometric points, distances, features and cached impulses remain
  attached to selected contacts. Rapier receives at most four points and therefore does not
  re-run its skin-unaware reduction. Existing per-point solver filtering still runs normally.

Both source files are Apache-2.0, with the license reproduced in `LICENSE`.

Mechanical adaptations of Parry's file: external crate import paths; std `HashMap`; the private
workspace uses `TypedWorkspaceData::Custom`; retain only the compiled 3D/f32 branches; remove
unused serialization feature annotations and the identity `AsPrimitive` integer conversion.
No physical distances, voxel states, body poses or velocities are changed by these adaptations.
The reduction helper is made generic over contact data and copies selected points back in the
same order the original reducer sends to the solver, without allocating a second solver state.

The dispatcher overrides only native voxel/non-ball convex manifold generation. Ball contacts
retain Parry's already-correct specialized generator. Other manifold generators and all distance,
intersection, translational/nonlinear cast queries delegate to `DefaultQueryDispatcher`.
Four-point reduction uses the complete prediction band for every generated manifold, so fruit
against fruit obeys the same skin contract as fruit against terrain.

## Removal on upgrade

`re-flora-physics` pins Rapier 0.34.0; the repository lockfile pins its Parry to 0.29.0. Do not
silently copy newer entire library modules into this backport. When upgrading, verify both the
voxel candidate expansion **and** skin-aware manifold reduction upstream, remove the dispatcher
and these source copies, then run the direct prediction-band test, `tests/fruit_ground.rs`, the
game's actual apple-description test, all character/terrain regressions and the real-game trace
documented in `docs/fruit_ground_jitter.md`. A library version bump alone is not acceptance.

The separate fruit-only extra solver iterations address measured convergence on the apple's
triangular resting face under game gravity; they are not part of this contact-generation patch.
