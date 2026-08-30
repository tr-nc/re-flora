# Garden Tree Publication

Tree placement is one garden change even though its observable result spans foliage rendering,
fruit physics, attached-fruit rendering, local-sun shadow history, canopy audio, leaf emitters, and
the canonical tree record. `GardenTrees` owns that invariant. A tree is canonical only after every
earlier publication action succeeds; a failed action compensates the already-applied actions while
the previous canonical record remains authoritative.

## Interface designs considered

### App forwarding facade

`App::place_tree` could forward a compiled tree to the existing `Tracer`, `TerrainPhysics`, audio,
and `TreeRuntime` calls. This minimizes the diff but fails the deletion test: deleting the facade
would reveal the same ordering and rollback knowledge in every caller. It also leaves removal as a
different orchestration path. This shallow design is rejected.

### Extensible command registry

A generic command trait and a parameter bag could let subsystems register arbitrary prepare,
commit, and rollback callbacks. That is flexible, but callers and tests would need to understand
the registry, callback ordering, downcasts, and a large bag of unrelated dependencies. Re: Flora
has one concrete tree publication protocol, not an ecosystem of third-party tree actions. The
extra generation/version machinery would be speculative, so this design is rejected by YAGNI.

### Canonical owner with a closed publication plan

`GardenTrees::{place, replace, remove}` is the selected interface. Placement compilation produces
one `PreparedTreePublication` whose fields are meaningful tree facts, not caller-selected service
arguments. `GardenTrees` executes a closed `TreePublicationAction` sequence through an internal
host seam. The production host applies actions to owned runtime modules; the recording host proves
ordering, identity, omission sensitivity, preparation failure, and compensation without GPU work.
Leaf clusters and the canonical record commit together as the final action.

This design has the smallest caller interface, keeps protocol changes local to one owner, and uses
a real seam because production and recording adapters both exist. It deliberately does not add a
tree generation abstraction: tree identity and canopy acoustic generation already have distinct,
adequate meanings.

## Publication contract

Placement and replacement publish foliage, fruit lifecycle, attached fruit, shadow invalidation,
and canopy audio in that order, then atomically make the prepared record and leaf clusters
canonical. Removal publishes the inverse physical effects in the same ownership order and removes
the canonical record last. Every fallible input is checked before the first action. If an execution
action still fails, the production host restores the previous canonical publication (or removes a
partially placed new tree); compensation failure is reported together with the original error.

The world-edit transaction that publishes trunk voxels remains an upstream prerequisite. It has a
separate terrain publication contract and is not hidden inside the tree-observer publication seam.
