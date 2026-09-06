# Terrain and vegetation snapshots

`Save terrain` and `Load terrain`, under **Terrain & Plants**, use a single atomic snapshot at the existing
selected path (default `saves/terrain_snapshot.rflterrain`). The CLI flags remain `--terrain-save`
and `--terrain-load`. This format replaces the terrain-only v1 format; no migration is provided.

## Persistent state

- The exact packed terrain atlas, including hand-edited tree wood.
- Both painted grass species: roots and current per-instance growth, read from their live GPU streams.
- Lavender, ember bloom and kochia: roots, species, growth, seed and stable response identity.
- Every tree: identity, position, mature authoring description and global age. Leaves and their
  anchors, tree ownership, fruit specifications and canopy audio are rebuilt from that description.
- The fruiting cycle and global flora growth override. Loading a completed crop does not replay
  its old fruit drops; moving the fruiting control backwards still re-arms the next crop.

Spawn animation timestamps are stored as ages and rebased to the new process clock. Wind response
history is discarded on load. Loose apples, butterflies, transient particles, camera, inventory,
water simulation and other placeable entities are not part of this vegetation snapshot. Mesh
appearance controls continue to come from the separately saved GUI configuration.

## Ownership and failure handling

The file owner in `src/terrain_persistence.rs` keeps terrain records and a bounded, checksummed
vegetation section in one temporary file, synchronizes it, then atomically replaces the destination.
Format version 2 adds a 12-byte vegetation header (little-endian byte length and CRC32), followed
by UTF-8 JSON. The section is limited to 64 MiB and the whole file to 1 GiB. Existing terrain
record checksums and bounds still apply.

The application validates the entire file, species identities, chunk coverage and reconstruction
bounds before runtime mutation. The surface builder owns GPU-to-persistent flora conversion; the
tree owner reconstructs tree records and observers **without clearing or stamping wood**. Loading
replaces vegetation instead of appending it. Growth work is rescheduled against the current clock.

Snapshot restoration discards stale terrain-edit bounds, but does **not** run detached-voxel
cleanup. A snapshot is authoritative, not a new edit: that previous loading behavior could delete
saved tree branches. Normal player-edit detachment remains unchanged. The former full-world
reconciliation remains an explicit diagnostic, outside the loading path.

Water and GPU writes are quiesced for snapshots. Failure before mutation leaves the running world
alone; a publication failure after mutation freezes world updates and requires restart, rather than
allowing an incompletely restored world to continue. Saving failures retain the previous file.

## Reproducible acceptance

Unit tests cover the format, checksum rejection, legacy rejection, flora identities/growth, and
deterministic tree leaf/fruit reconstruction. GPU and lifecycle acceptance is opt-in:

```sh
RUST_LOG=info RE_FLORA_GARDEN_SNAPSHOT_SMOKE=seed cargo run --release -- \
  --hidden --mute --auto-exit 0.5 --terrain-save target/garden-fixture.rflterrain
RUST_LOG=info RE_FLORA_GARDEN_SNAPSHOT_SMOKE=verify cargo run --release -- \
  --hidden --mute --auto-exit 0.5 --terrain-load target/garden-fixture.rflterrain \
  --terrain-save target/garden-fixture-roundtrip.rflterrain
```

The first run paints all five flora species and creates two trees at age 0.43. The second checks
startup restoration, then makes intervening tree edits and loads twice through the GUI's runtime
entry point. It compares every terrain byte, flora root/growth/identity and tree/leaf publication,
and waits for the actual water publication to settle. Success logs `[GARDEN_SMOKE] passed`.
These controls are absent unless the environment variable is explicitly supplied.

Validated on the implementation branch: 891 application tests and 4 build-helper tests passed;
one pre-existing PATT fixture-dependent test remained explicitly skipped, and one test ignored.
The 332 script tests passed. Release runs passed both the five-species/two-tree fixture above and
an empty-vegetation snapshot, including two intervening-edit/runtime-load cycles for each. The
saved GUI and camera configuration files were unchanged by these runs.
