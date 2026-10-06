# Wind shedding, safe Walking entry, and butterfly facing

## Scope and evidence

This records the three player reports together. Engine documentation establishes available techniques, not how every shipped game implements them. Plant studies establish mechanisms, not Re: Flora's numerical tuning. No final artistic, motion, performance, or cross-platform acceptance is claimed.

## Leaf retention

Normal abscission is regulated separation at an abscission zone, distinct from forcibly breaking a healthy attachment. Auxin/ethylene interactions are experimentally documented, but pathways vary across species and triggers; they are not a universal one-hormone switch. See [Beyer, 1975](https://academic.oup.com/plphys/article/55/2/322/6074755) and [Patharkar and Walker, 2016](https://academic.oup.com/plphys/article/172/1/510/6115762). These sources were available through indexed abstracts; full-text retrieval was limited.

Leaves also reduce projected area by changing orientation and shape. A [2017 tulip-tree wind-tunnel experiment](https://pmc.ncbi.nlm.nih.gov/articles/PMC5587486/) observed smaller projected areas at 5 m/s than at 1 m/s. Damage affected fluttering. That experiment does not establish universal detachment thresholds.

The former game rule compared instantaneous squared wind speed with attachments that weakened from maturity onward. A deterministic 128-socket regression at game wind strength 8 detached **128/128** sockets.

Implemented in `src/leaf_lifecycle.rs`:

- A healthy attachment plateau precedes age-related weakening.
- Established canopies start with a seeded age distribution, including a small senescent tail; replacement leaves start young.
- Effective wind loading uses a simple drag-reduction proxy, `v² / (1 + v/4)`, rather than fixed broadside loading.
- Ordinary wind preferentially releases weakened leaves; genuinely extreme loads can still detach the entire canopy. There is no per-gust quota or permanently protected fraction.
- Existing strength, weakening half-life, and regrowth controls remain the authoritative saved inputs. Half-life also sets the healthy-lifetime scale.

The healthy threshold range (8–16 game wind units), four-half-life healthy timescale, and nominal 2% initial senescent tail are **authored game tuning**, not botanical measurements. In the same 128-socket regression, strength 8 now releases **6/128** sockets. That is a fixture result, not a universal gameplay percentage. Sustained-load fatigue, species/season effects, and detailed leaf mechanics are not implemented.

## Safe Walking entry

A ray finds a point, not a space large enough for a person. The first downward intersection can be a cave roof, bridge, or canopy. Placement therefore needs a full-body collision query.

Relevant official techniques:

- [Unity ComputePenetration](https://docs.unity3d.com/ScriptReference/Physics.ComputePenetration.html): minimum translation to separate overlapping colliders; primitive/convex restrictions and backface caveats apply.
- [Unity overlap recovery](https://docs.unity3d.com/ScriptReference/CharacterController-enableOverlapRecovery.html): automatic static-geometry recovery, explicitly excluding heightfields.
- [Unreal FindTeleportSpot](https://dev.epicgames.com/documentation/en-us/unreal-engine/API/Runtime/Engine/UWorld/FindTeleportSpot): searches for nearby acceptable placement and reports failure if none is found.

Implemented using our existing Rapier character collision world:

1. Validate the complete player capsule once on entry, including cycle/snapshot entry and completed zoom transitions.
2. Preserve a clear position, including an airborne position; normal gravity can land the player.
3. If embedded, search small nearby shells, then bounded upward candidates (maximum 512 voxels / two world units). Every candidate must fit the full capsule.
4. If no candidate is safe, remain in free flight rather than committing an immobile Walking state.

This is bounded candidate search, not an exact minimum-translation solver. It adds no continuous per-frame overlap search. Last-safe-position fallback and recovery from terrain subsequently being built around an already walking player are not implemented.

## Butterfly facing

Ground velocity and air-relative velocity need not point the same way in wind; see [NASA relative velocity](https://www1.grc.nasa.gov/beginners-guide-to-aeronautics/relative-velocity/). We deliberately prioritize legible game motion over reproducing that mismatch.

Previously, native facing followed snapshot velocity, while wingbeat coupling could replace it with an independently smoothed air-relative orientation. A renderer regression confirmed that changing velocity could leave the coupled orientation unchanged.

Implemented:

- Published movement velocity is the sole primary facing authority; local model -Z follows that velocity.
- Remove separately maintained heading/pitch/full-orientation state from wingbeat coupling.
- Retain decorative bank and animation phase/blend. Roll does not change the forward axis.
- Derive wingbeat force directions from supplied velocity instead of retained heading.
- Zero/invalid velocity uses a deterministic finite default basis, not normalization of a zero vector. Retaining the last heading at rest is not implemented.

Tests cover cardinal/vertical/diagonal velocity, bank, changing velocity under full coupling, and existing flight/particle publication. Detailed visual acceptance of vertical-flight pitch and cached-view discretization remains a manual-review concern.

## Validation

The isolated full Rust suite passed: **1378 application + 4 library tests**, five ignored. The separate physics crate's unit/integration suites passed **51 tests**. Focused checks passed: Walking recovery and 25 camera / nine surface-motion regressions; 42 butterfly tests; nine leaf-lifecycle tests. Native hidden/muted runs verified safe Walking entry twice, actual butterfly draws on native and cached paths, and complete extreme-wind leaf handoff/regrowth over two generations. The initial natural-flight wingbeat run contained no subjects and is **not** evidence of flight behavior; the subsequent explicit four-butterfly fixture exercised rendering instead.

The player's uncommitted GUI and camera files are preserved. Changes are local commits only. Release builds and hidden smoke runs are required before handoff; screenshots and correctness checks do not establish a performance budget or final visual approval.
