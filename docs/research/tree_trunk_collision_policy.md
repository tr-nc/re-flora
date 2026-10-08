# Shared static trunk-only collision for every tree

## Diagnosis

Starting revision: `b2f9e66a4f03c4f0868eeeb4dd37ff9ac4d6ab31` in `new-tree`.
Both Garden Tree and Conifer already used immutable **no-wind rest collision**, not per-frame
Rapier updates. The mismatch was **which geometry** became physical. The physical path projected
the complete visual CPU deformation plan, including lateral wood. There was no shared trunk-only
policy; Conifer's thinner authored limbs merely produced less voxel collision geometry. It was
not a valid species-specific guarantee that limbs never collided.

A deterministic regression drives the real `TerrainPhysics::prepare_tree_collision_surface` seam:
one resident root-axis voxel plus one well-separated lateral voxel. Both must remain visible and
pickable (24 triangles), but only the trunk may collide (12 triangles). Before the fix:

```text
cargo test every_tree_architecture_has_trunk_only_rest_collision -- --nocapture
GardenTree axis_model=false: lateral visual wood leaked into physical collision
left: 24
right: 12
exit=101
```

Artifact: `target/tree-trunk-collision-policy/red.log`. The existing static-pose/no-refit test
already passed before the change, ruling out a new species-dependent per-frame-update defect.

## Source-level invariant

- `Tree::build` derives visible physical cones from structural roles **and full ancestry**.
  Every ancestor must belong to a root `MainAxis`. A side axis's locally named `MainAxis`
  continuation is still lateral. No species switch, branch-level heuristic, skin-weight cutoff,
  per-kind flag or Debug opt-in controls this rule. Exact authored radii/subdivision are retained.
- `Tree::world_physical_trunks(origin)` exposes only rest root-axis wood, using the same world
  translation arithmetic as stamping/ownership. Pose/wind/kick cannot be passed to this interface.
  New generators must supply the already-required structural parent/role fields; they need not
  remember a collider optimization. Regression loops use `TreeSpecies::ALL` automatically.
- `RestTrunkCollision` classifies **resident wood voxel centers** against those cones and unions
  overlaps. It never recreates a cut from a pristine cone. It constructs a closed, shared-corner
  triangle surface from physical occupancy, not the rendered faces: lateral wood next to a trunk
  cannot suppress a needed physical cap. Coordinates/index overflow reject preparation safely.
- Full rendered geometry, skinning, GPU BVH, normals, lighting, attachments, wind and exact async
  picking remain unchanged. Physical policy cannot leak into visual picking by filtering that stream.
- Trunk occupancy participates in complete publication equality and per-tree fragment reuse,
  including optional-cache exhaustion and lowest-ID visual ownership. Cache accounting includes
  the new occupancy. The full compact visual triangle copy is no longer allocated in production
  solely for physical collision; its test oracle remains available.
- All resident tree wood is excluded from underlying terrain collision, **not just the trunk**.
  Otherwise side colliders return through imported voxel bricks. Side-only publications retain
  an exclusion revision even with zero physical triangles, so fallback/removal can clear them.
- Collision still prepares/commits transactionally only on geometry publication. Wind/kicks do not
  mutate it. Explicit committed-terrain failure keeps the existing authoritative-voxel fallback;
  this safe fallback is not a successful trunk-only publication.

No saved Settings, GUI controls, authored appearance, shaders or generated files changed.
Branches intentionally no longer block characters/fruit; animated trunk/rest mismatch remains
approved. This is physical simplification only, not a visual quality reduction.

## Validation

- Actual publication regression passes Garden Tree and both Conifer architectures; every
  registered species is tested with both architecture flag values.
- Tree model tests cover every registered species, ages 0/0.25/1, exact subdivision/radius identity,
  translation and lateral continuations. Empty age-zero geometry does not invent a phantom trunk.
- Tests cover closed/compact topology, shared faces/corner overflow, carving, side-only exclusions,
  later terrain imports, idempotent fallback clear, immutable collision across repeated wind poses,
  failed preparation, replacement and removal. Existing fragment material/cut/overlap/cache-limit
  tests also cover the new physical mask through publication equality.
- `cargo fmt --check`, `cargo check`: pass. Full `cargo test`: **1465 + 4 passed, 6 ignored**.
  Targeted suites: app physics **26**, raster/compact/fragments **18**, tree model **16**.
  After the final test-fixture initializer cleanup, the all-species publication test was repeated.
  Explicit kick-audio runtime and captured native-DSP replay also pass; unchanged four-variant
  peaks **0.268–0.338**, no isolated clipping. The runtime test reports unavailable optional ALSA
  device names; it passes through the available backend, not a physical-hearing acceptance.
- Hidden native Release raster/vegetation smoke passes delayed kick bending, overlapping wind-on
  impulses, **16 normalized canopy samples**, seeded **9/17** fruit release, **24 leaves**, the
  **576-byte** fenced handoff/no query wait, exact async picking, leaf history, rest collision,
  lighting, age/removal/replacement/Conifer and exact publication rollback.
- Original one-tree smoke physical triangles **29636 -> 5328** (points **17660 -> 2666**).
  This count reduction is real-app Release evidence, not a promised FPS gain.

Final `cargo build --release` passes; the updated binary is in this worktree. Inspected final
hidden Release logs `target/re-flora-logs/re-flora-20261009-020619.975-56266.log` and
`target/re-flora-logs/re-flora-20261009-020628.932-56444.log`: no ERROR/panic/VUID,
shutdown `failures=0`. Final Release SHA-256:
`ae57d157013b0edc2a180259505c165f540e3580e3482bd634b3216cee8b6021`.

Validation artifacts live under `target/tree-trunk-collision-policy/`; no visible game is
automatically launched. Muted smoke does not establish listening quality or OS K behavior.

## Controlled Release replays

Saved size 20 and all saved controls remain identical. Four-tree Garden and mixed Conifer
fixtures, final hold 6 s, hidden/muted normal Vulkan surface, GPU validation at every count.
The Conifer replay retains the initial Garden tree and adds three Conifers; it is not a pure
Conifer forest. Old/new/old runs are serialized with the shared GPU lock; no other game at the
contention gate. The background browser remains open; no unrelated process was terminated.

| Workload | Visual triangles, old/new | Physical triangles, old -> new | Physical points, old -> new | Old/new/reverse median frame ms |
| --- | ---: | ---: | ---: | ---: |
| 4 Garden trees | 107150 / 107150 | 107150 -> 19844 | 64204 -> 9930 | 16.970 / 16.926 / 16.987 |
| Garden + 3 Conifers | 49880 / 49880 | 49880 -> 15548 | 29987 -> 7782 | 17.173 / 16.891 / 16.794 |

No rendering/picking primitives were omitted; all phases validate, no errors or frame cliffs.
These workloads are near presentation cadence. Variability/reverse results do **not** establish
an FPS speedup, sustained walking/kick acceptance or a dense-forest performance budget pass.
They establish the shared physical reduction without a material frame-time regression in these
small workloads. No CPU-preparation/GPU/acquisition times are added into a fabricated saving.

Reports: `before-garden-02`, `after-garden`, `reverse-garden`, `before-conifer`, `after-conifer`,
`reverse-conifer` under the artifact directory. The initial `before-garden` attempt was rejected
before app execution because the old harness `--tree-size` regex assumes size immediately follows
`[tree.desc]`; saved species/architecture fields now precede it. It is excluded. Accepted reruns
omit the override and use the already-saved size 20; no workaround is shipped into game code.

Original binary SHA-256:
`9347f7982d2f905b1aeb41d6eb3a95546803ca3e078e92e53529d48ba72ac057`.
Measured candidate SHA-256 (before later test-only edits):
`b7a418f46844` prefix; full hash is recorded by each candidate report.
Saved GUI SHA-256 throughout:
`9ff97821ac23c9b8a91ad5ff87e65c2e03189acd8b9183541a26c1f093e4725b`.
Camera file SHA-256:
`d45b3c3875c2896a7b5c83b19520f8401bf29fc7208063fde5bb95191bf7f29a`.

Optional `cargo clippy --all-targets -- -D warnings` remains blocked by pre-existing findings in
scene-resolution tests, tree-kick/GPU-pose code, GUI grouping, butterfly tests and old compact/cache
fixtures. The new fixture's initializer finding was fixed; unrelated cleanup is not bundled here.
