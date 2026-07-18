# Voxel collision architecture

Status: implementation decision, based on isolated release-mode experiments on 2026-07-18.

## Objective

Add a general collision and rigid-body foundation for falling fruit and later gameplay objects without turning the water collider into a global physics representation.

The collision layer must support:

- exact terrain-aligned voxel collision;
- dynamic rigid bodies with orientation, friction, restitution, CCD, and sleeping;
- ray casts, shape casts, and overlap queries;
- incremental terrain edits;
- stable contact across independently updated collision bricks;
- a fixed-step simulation whose coordinates remain numerically well-scaled.

## Decision

Use Rapier 3D with Parry's native `Voxels` shape behind a project-owned `CollisionWorld` API.

Use physics-space voxel units: one terrain voxel is one physics unit. Convert positions to render world units at the rendering boundary. Gravity, velocities, and forces must be tuned or converted consistently instead of reusing render-world values directly.

Keep the existing 32-cells-per-world-unit SDF as a water-specific representation. It is useful for large particle workloads and smooth distance sampling, but it is not accurate enough to be the general rigid-body collider.

## Release benchmark results

The common terrain workload was one logical 32-cubed brick containing a two-voxel-thick floor, 100 dynamic radius-two bodies, 600 steps at 120 Hz, CCD enabled, and identical friction and damping where the compared implementation supported them.

### Terrain representation

| Backend | Average step | Build or update | Contact result | Important limitation |
| --- | ---: | ---: | --- | --- |
| Custom voxel narrow phase | 43.6-45.9 us | 0.0032-0.0034 ms build | No measured penetration or seam stall | No broad phase, general solver, body-body contact, CCD, or sleeping |
| Rapier native voxels | 69-74 us | 0.15-0.25 ms build; 0.6-1.3 us local edit | Stable rolling, CCD, and sleeping | Synthetic flat brick, not a complex real terrain brick |
| Rapier triangle mesh with internal-edge fix | 103 us median | 1.45 ms build; 1.51 ms full-brick edit rebuild | Stable after internal-edge preprocessing | Rebuilds mesh and BVH after an edit |

The custom implementation is only a narrow-phase lower bound. Its small advantage over the complete Rapier result does not justify owning a solver, broad phase, CCD, sleeping, contact persistence, and body-body collision.

The default triangle mesh produced a vertical kick of 0.147312 voxel/s while crossing flat internal triangle edges. `FIX_INTERNAL_EDGES` removed the kick but did not remove the mesh generation and BVH rebuild costs.

### Brick boundaries and edits

Two adjacent native voxel colliders without shared neighborhood state allowed a body to cross the boundary, but produced a vertical kick of 0.094443 voxel/s. Calling `Voxels::combine_voxel_states` removed the kick.

After an edit to a voxel on a brick face, calling `set_voxel` only on the edited brick leaves the adjacent brick's face state stale. The edit must also be sent through `Voxels::propagate_voxel_change` for every affected face-neighbor.

These are correctness requirements:

1. Combine voxel states whenever a brick is inserted next to an existing brick.
2. Diff brick updates and apply local `set_voxel` operations.
3. Propagate changed face voxels to the corresponding face-neighbor.
4. Wake dynamic bodies whose AABBs intersect the edited region.
5. Reject stale source revisions.

Face-neighbors are sufficient for Parry's face-state bits. An edit on a brick edge or corner can affect two or three face-neighbors and must be propagated to each of them.

### Dynamic-body scaling

| Bodies | 600-step total | Representative average step | State after 600 steps |
| ---: | ---: | ---: | --- |
| 10 | 8.64-9.02 ms | 14.4 us | All sleeping |
| 100 | 41.69-47.02 ms | 69.5 us | All sleeping |
| 1000 | 293.03-303.38 ms | 488 us | All active in a tall pile |

The 1000-body workload was deliberately unresolved after 600 steps. It produced 550 contact pairs deeper than 0.01 voxel, with a maximum depth of 0.02124 voxel. Dense fruit piles therefore need a separate release benchmark before solver iterations or pile behavior are finalized.

### Real Contree brick measurement

The first app-integrated source probe imported the 32-cubed brick at voxel minimum
`(256, 96, 256)`, collision-brick ID `(8, 3, 8)`. This brick contains the base of
the startup tuning tree and depends only on terrain source chunk `(1, 0, 1)`.

In a hidden release run on the same tested machine, source revision 2 produced 1,215
solid voxels. Exact Contree export took 0.975 ms, occupancy packing took 0.013 ms,
Rapier voxel collider insertion took 0.202 ms, and the complete import took 1.190 ms.
An isolated worker run measured 0.937 ms, 0.014 ms, 0.171 ms, and 1.122 ms
respectively, so the result was repeatable at the scale needed for scheduling.

This measurement supports budgeted CPU-cache extraction as the initial production
path. It does not justify rebuilding an unbounded number of bricks in one frame:
terrain synchronization still needs a revisioned queue and a per-frame work budget.
The GPU readback fallback is not needed unless later complex-scene measurements show
that decoded-cache traversal dominates that budget.

## Fruit collider experiment

The current raster apple contains 32 unit cubes selected from a four-voxel-diameter sphere. Four rigid collider descriptions were tested against the native voxel floor.

| Shape | Average step for 100 | p95 | Motion character | Sleeping after 600 steps |
| --- | ---: | ---: | --- | ---: |
| Radius-two sphere | 66.8-67.4 us | 138.5-140.2 us | Smooth rolling; arbitrary final orientation | 100 |
| Four-cubed box | 178.6-182.1 us | 236.2-242.7 us | Slides about one voxel and does not roll | 100 |
| Convex hull of raster-apple voxel corners | 218.1-221.6 us | 346.0-348.5 us | Faceted tumbling; settles close to an axis-aligned face | 64 |
| Exact compound of 32 boxes | 851.7-871.7 us | 1377-1417 us | Similar to convex hull, with much higher contact cost | 0 |

All runs had zero escapes and no contact deeper than 0.01 voxel. A single convex fruit slept at step 402. The exact compound slept at step 438 in isolation, so its poor 100-body result came from multi-contact pile convergence rather than a permanently unstable terrain contact.

For the intended art direction, use the convex hull for apples. It visibly tumbles but naturally settles within about 1.8 degrees of a grid-aligned orientation in the horizontal probe. Do not lock orientation while moving and do not use the exact 32-box compound. A sphere remains a possible later physics LOD if a measured high-fruit-count workload requires it.

## Runtime ownership

`CollisionWorld` owns Rapier and exposes project types rather than leaking Rapier handles throughout the app. The initial API should cover:

- insert, update, and remove a revisioned static voxel brick;
- spawn and remove dynamic rigid bodies;
- fixed-step simulation;
- read dynamic transforms for rendering;
- ray cast, shape cast, and overlap query entry points;
- collision groups and material properties;
- explicit wake-up around terrain edits.

Contree's decoded CPU source is the canonical owner of terrain presence and source
revisions. Both collision paths use those revisions for invalidation and stale-work
rejection, but they deliberately derive different occupancy representations:

```text
revisioned Contree CPU terrain source
    |-- sparse surface-shell voxel block
    |       -> exact 32-cubed Rapier voxel bricks
    |       -> CollisionWorld / rigid bodies
    |
    `-- chunk dirty notification + source dependency revision
            -> async GPU atlas filled-solid sample
            -> immutable 32-cubed solid grid
            -> water SDF + normal grid + ghost density
            -> particle collision
```

The semantic split is required. The CPU Contree cache represents the sparse visible
surface shell and is appropriate for exact terrain-aligned Rapier contact. It does
not preserve the filled interior needed to compute a correctly signed distance field:
in a direct experiment it produced only 109-296 occupied samples in affected terrain
chunks, rather than the 12,844-17,322 filled samples from the GPU atlas, and reduced
the deepest negative SDF from roughly `-0.5` to `-0.016`. Water therefore continues
to sample filled-solid occupancy from the GPU atlas.

This is one revision/invalidation system, not one interchangeable voxel payload. A
water sample captures the Contree source dependency before GPU submission and is
discarded if that dependency changes before readback completes. The immutable sampled
grid then travels with the queued SDF build, so water no longer owns a parallel
`CpuSolidVoxelStore`, duplicate chunk revision counter, or second long-lived occupancy
cache.

## Fruit state and rendering

Attached apples are currently render-only instances. A `FruitRegistry` should retain stable fruit IDs, tree ownership, attached transforms, and optional dynamic-body handles.

The GUI action changes fruit state from attached to dynamic. It should not contain physics logic itself.

Dynamic fruit rendering needs orientation as well as position. The existing static apple instance path is translation-oriented, so dynamic apples need an instance format containing a quaternion or equivalent rigid transform. The raster apple remains a rigid voxel sculpture: its local cubes do not deform, while the complete object may rotate in world space.

## Implementation sequence

1. Add the pinned Rapier dependency and retain the native-voxel release benchmark.
2. Add an isolated collision crate or module with revisioned static voxel bricks, state combination, edit propagation, fixed stepping, and unit tests.
3. Add and benchmark an exact 32-cubed source adapter for one real terrain brick.
4. Integrate budgeted, revisioned terrain-brick synchronization and wake-up after edits.
5. Add `FruitRegistry`, the GUI transition, convex fruit bodies, and dynamic rendering transforms.
6. Validate 10, 100, and high-count fruit drops in the release app before tuning solver iterations, CCD policy, or physics LOD.
7. Add pipe and other prop colliders through the same `CollisionWorld` API after terrain and fruit are stable.

Each step should remain independently validated and committed. Do not remove the coarse water SDF as part of this work.

The implementation currently completes steps 1 through 4: exact Contree block export
feeds a budgeted, revisioned multi-brick queue; terrain edit paths dirty the affected
bricks; and published changes combine boundary state and wake nearby sleeping bodies.
The capped 120 Hz dynamic-body path is also exposed through the GUI as a lit, rotating
convex-fruit collision probe. Step 5 remains incomplete: attached fruit still needs
stable registry state and a real attached-to-dynamic drop transition. High-count fruit
validation from step 6 also remains future work.

## Experiment provenance

The isolated worker commits used to produce these results were:

- `b59439c9` and `6c2fffce`: Rapier native voxels, CCD, sleeping, scaling, brick seams, and boundary edits;
- `5aef2a63`: triangle-mesh comparison and internal-edge A/B test;
- `d36044f9`: custom narrow-phase lower bound;
- `73e2276e`: fruit shape and resting-orientation comparison.
