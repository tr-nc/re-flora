# Garden Tree Publication

Tree placement is one garden change even though its observable result spans trunk voxels and their
visible-terrain publication, foliage rendering, fruit physics, attached-fruit rendering, local-sun
shadow history, canopy audio, leaf emitters, and the canonical tree record. `GardenTrees` owns that
invariant. A tree is canonical only after every earlier publication action succeeds; a failed
action compensates the already-applied actions while the previous canonical record remains
authoritative.

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

`GardenTrees::{place, replace, replace_batch, remove}` is the selected interface. Placement
compilation produces one `PreparedTreePublication` whose fields are meaningful tree facts, not
caller-selected service arguments. `GardenTrees` executes a closed `TreePublicationAction`
sequence through one internal executor. That executor alone maps actions to typed publication
primitives. The production and recording hosts implement those same primitives, so an omitted or
exchanged production mapping changes the recorded behavior without duplicating the action match.
Leaf clusters and the canonical record commit together only after trunk and observer publication
succeed.

This design has the smallest caller interface, keeps protocol changes local to one owner, and uses
a real seam because production and recording adapters both exist. It deliberately does not add a
tree generation abstraction: tree identity and canopy acoustic generation already have distinct,
adequate meanings.

## Publication contract

Placement and replacement checkpoint and publish trunk voxels first, then publish foliage, fruit
lifecycle, attached fruit, shadow invalidation, and canopy audio in that order. They make the
prepared record and leaf clusters canonical only after the whole physical transaction commits.
Removal publishes the inverse physical effects through the same owner and removes the canonical
record last. Inputs that can be preflighted are checked before the first action. The concrete trunk
executor treats both atlas mutation and visible-terrain publication as fallible: a failure at any
point runs the inverse checkpoint publication, aborts unfinished physical terrain work, and reports
the original and restore errors together if both fail. Later observer failure restores the same
trunk checkpoint plus observer, fruit-body, and audio publication (or removes a partially placed
new tree).

Cherry-wood voxels do not encode a tree identity. `GardenTrees` therefore keeps a chunk-local index
of canonical trunk owners. A transaction queries only chunks touched by its old and new bounds,
clears the target plus intersecting retained neighbors, then deterministically redraws the desired
target and neighbor union. Restore uses the inverse union. This covers remove, shrink, batch, and a
replacement whose new footprint alone overlaps another canonical tree without scanning every tree.

Age rebuild uses one `replace_batch` transaction. It clears all old trunks before stamping any new
trunk so overlapping trees cannot erase one another, delays every canonical commit until all
observers succeed, and restores the entire physical batch if any tree fails. Canopy Voice
realization is strict within the same transaction: one failed generation spawn removes every Voice
created by that synchronization, and checkpoint restoration propagates physical realization
failures instead of accepting a lifecycle-only restoration.

When tree description and global age change in the same GUI frame, the tuned description is a
one-shot staged input rather than a canonical mutation. The age rebuild consumes it and commits it
with the successful prepared record. Failure leaves the old physical and canonical description in
place, and a later rebuild cannot accidentally reuse the discarded unpublished description.

## Derived raster surface validity and publication

A terrain edit does not necessarily change the derived tree surface. The raster path separates
three responsibilities rather than teaching individual editing tools how to skip tree work:

- `RasterTreeMesh` derives `TreeSurfaceDependencies` alongside surface extraction. Dependencies
  conservatively cover each authored world-space wood cone plus the surface sampler's shared
  neighborhood radius, clipped to the sampled atlas region. They include potential wood even
  where the atlas is currently empty. The bulk readback AABB is not the semantic dependency.
- `TreeSurfaceCache` owns revision validity. A terrain publication outside those dependencies
  advances validity without rebuilding. An intersecting edit stays dirty until a complete
  rebuild succeeds; canonical-tree changes always require revalidation. Dependency intersection
  accepts inclusive voxel bounds, including single-cell and boundary-touching edits.
- `Tracer::publish_static_raster_trees` owns publication of the derived surface and attachments.
  It retains resident resources and pose history only when the complete observable surface is
  exactly equal: vertex bytes (including normals/confidence), indices, solid occupancy, skin
  bindings, cell lookup, and attachments. It does not rely on counts or a hash. Publication is
  marked invalid before any fallible resource change and valid only after every upload succeeds,
  so a failed partial upload cannot be accepted as an unchanged result on retry.

Terrain changes can affect tree shading without removing any wood. Neither dependency tracking
nor publication equality may use a removed-wood count as a shortcut. Shared sampling radius,
conservative potential-wood support, and exact output comparison keep these rules local and
applicable to every edit path that publishes visible terrain. The current per-cone AABBs can be
replaced by a more precise dependency index without changing editing tools or the cache interface.

Validation and Release measurements are recorded in
[`research/tree_surface_publication_optimization.md`](research/tree_surface_publication_optimization.md).
