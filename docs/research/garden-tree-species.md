# Garden trees: apple, pine and weeping willow

## Scope and references

This iteration delivers three plantable, recognizable tree architectures, not new names on the
same recursive silhouette. Existing Garden Tree and experimental Conifer descriptions remain
loadable with their original generators. New plants use distinct persistent identities.

Primary references read:

- [NC State Extension: Malus domestica](https://plants.ces.ncsu.edu/plants/malus-domestica/):
  deciduous edible-fruit tree, potentially as wide as tall; cultivars/rootstocks vary greatly.
  Design: rounded, spreading crown, visible scaffold wood, irregular overlapping leaf masses,
  red apples hanging below the foliage. This is a picturesque garden apple, not a claim to model
  a particular cultivar or orchard training system.
- [Penn State: open-center training](https://extension.psu.edu/pruning-and-training-home-fruit-trees-to-an-open-center):
  the open-center advice is primarily for stone fruits, not a universal apple shape. Do not turn
  the apple into an empty wineglass based on that reference.
- [NC State Extension: Pinus sylvestris](https://plants.ces.ncsu.edu/plants/pinus-sylvestris/):
  young conical/columnar habit becomes an open, rounded, irregular crown; short needles in pairs.
  Design: irregular branch stations, unequal limb counts and lengths, open lower trunk, curved
  ascending tips and clustered sprays. Mature Scots-pine-inspired silhouette rather than a
  regular fir/Christmas-tree cone. Existing voxel leaves remain a stylization, not literal needles.
- [NC State Extension: Salix babylonica](https://plants.ces.ncsu.edu/plants/salix-babylonica/):
  broad tree, pendulous branches/stems, linear/lanceolate leaves, commonly beside water.
  Design: rising scaffold crown with long, separately attached drooping shoots; narrow sprays
  following those shoots, varied lengths and gaps instead of a solid green dome.

No reference images or external assets are imported. Geometry is authored procedurally in Rust.
The UMN apple page returned HTTP 403 and is not used as evidence.

## Implementation contract

- Distinct persistent species profiles own architecture and leaf-spray/fruit defaults. The shared
  axis compiler owns connected parent-before-child topology, continuous taper, stable axis IDs,
  causal reveal ages and the bounded segment count.
- Seed varies an individual; age scales/reveals its same topology. Every foliage attachment has
  a visible branch, and every visible child has a visible parent. Existing wind/pose, leaf audio,
  terrain ownership, publication, editing and snapshot paths are reused.
- New broadleaf and pine architectures do not modify the old saved Garden Tree/Conifer generator.
- Apples use the real existing apple lifecycle/rendering. Pine and Willow must not produce apples,
  even if a copied or edited description contains a nonzero apple probability.
- No fake extra fruit species, texture pipeline, new material or permanent visual A/B is needed.
- Visual evaluation uses native Release captures, with deterministic seeds and real placement.
  Unit tests establish connectivity/determinism/persistence, not beauty or performance.

## Evaluation

Implementation and native validation results are recorded in `../evidence/garden-tree-species.md`.
