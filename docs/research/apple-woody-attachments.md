# Apples need a woody attachment, not a canopy offset

## Botanical reference

The Royal Horticultural Society's [identifying apple and pear fruit buds](https://www.rhs.org.uk/fruit/apples/identifying-fruit-buds)
distinguishes spur-, tip- and partial tip-bearing cultivars. Most apples are spur-bearing:
fruit buds occur on two-year-old wood and on short branched shoots (spurs) on older wood.
Tip-bearers fruit at the tips of the previous year's shoots. A surrounding leaf cluster is
not itself a physical attachment. This supports an explicit woody support connected to the
apple stalk; it does not justify random offsets from the leaf envelope.

Our stylization reuses existing side-branch wood and the existing model's stalk. It does
not claim to simulate cultivar-specific spur anatomy or introduce a new fruit asset.

## Reproduced defect

`cargo test --bin re-flora attached_apple_stems_reach_published_wood` failed before the fix:
seed 0, fruit 2702208344, upper stalk cap `(189.10574,144.04637,193)` had no supporting wood.
This runs `TreePlacementService::compile`, not a standalone synthetic placement algorithm.
It reproduces the reported floating-apple failure even without wind.

The old candidate positions added independent side/down offsets to quantized mature foliage
sites. These offsets were treated as final centers, with no check that the existing model's
short stalk could reach visible wood. Independent swing translated the entire model; its
upper stalk cap was not constrained. Subsequent skinning guessed the nearest branch at the
body center, not at a persistent woody support.

The free-offset implementation appears in `3faf8c5d` (`add controllable fruit placement and
swing`, 2026-07-14). This identifies the mechanism's history, not a verified first release
of the exact screenshot defect; later tree/model size changes also affect its visibility.

## Authoritative geometry and compatibility

- Candidate generation retains deterministic fruit IDs and cycle/strength metadata. Saved
  side/down placement values remain candidate preferences, not permission to detach fruit.
- Tree compilation resolves candidates to occupied side-wood cells with an exposed underside.
  It rejects unsupported candidates, reserves distinct instance keys, and keeps the mature
  size range clear of flat ground. It never fabricates a floating fallback.
- Each published fruit owns an exact, fractional rest-voxel stalk pivot. Rounded instance
  centers remain keys/broad-phase data, not attachment geometry.
- A fruit-specific lookup namespace prevents a fruit and leaf at the same coordinates from
  stealing each other's pose. Leaves keep their existing lookup and motion.
- Surface publication skins the *support*, with the same parent/child binding as wood.
  A rigid frame is translated to that skinned point; applying only the child bone to an
  interior branch point would reopen the gap during bending.
- Color (native mesh and cached pixel model) and shadows share `apple_attachment.slang`.
  Scale and growth alter the body below the fixed stalk cap. Wind rotates around that cap,
  rather than translating the entire apple. The physical pendulum arm is the actual
  stalk-to-center distance, not the old free translation-length field.
- Picking and event-only release use the matching CPU geometry and published response.
  Release preserves both orientation and velocity, including local swing angular velocity.
  When the response system is disabled/legacy, apples follow the branch without a second
  independently simulated local oscillator, keeping render and handoff consistent.
- No tree descriptor/save schema or imported model file changes. Restoring a garden rebuilds
  fruit attachment geometry under the new rule; existing fruit IDs stay tied to candidates.
  Tree scale, fruit size and species native tree sizes are not modified.

## Evidence and boundaries

Regression coverage checks occupied supporting wood, stalk contact across apple size/growth
scales and wind angles, the published asset's cap location, leaf/fruit lookup separation,
parent/child support skinning, and release continuity. Existing growth/snapshot/planting tests
exercise the ordinary publication path. Native hidden Release runs use the species-review
fixture and inspect GPU pose/surface/lighting validation and save/restore.

Before/after screenshots and drop traces are local `target/` artifacts, not imported assets.
The prior full-suite baseline has nine unrelated failures (two GUI-default tests and seven
raster-tree fixtures); report them separately rather than changing their expectations here.

This fixes generated/restored hanging fruit. It is not a new branch-break simulation: existing
terrain-carving behavior and removal of supporting wood mid-life require separate lifecycle
handling. Final appearance approval remains a user visual review; no performance gain is claimed.
