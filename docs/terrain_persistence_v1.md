# Terrain Persistence and Reload v1

Status: implemented; schema 1 remains current (re-audited 2026-08-28)

Tracking issue: [#65](https://github.com/tr-nc/re-flora/issues/65)

Decisions finalized: 2026-08-05

## Problem Statement

Before this implementation, Re: Flora terrain edits lived only in the running process. The current
path saves the authoritative voxel world and rebuilds its derived consumers after reload so a player
or developer can continue digging, filling, smoothing, or changing materials deterministically.

Persisting a derived representation would be incorrect. Surface output, Contree data, scene
indirection, colliders, acceleration structures, water terrain caches, and DDGI state are
rebuildable consumers of the authoritative editable voxel source. Saving any of them would couple
the file format to transient implementation details and could restore stale or incomplete state.

Saving also cannot race terrain publication. Visible terrain publication is synchronous, but the
Contree CPU cache follows through GPU readback and background CPU decoding. The file payload must
always come from one coherent authoritative GPU atlas state, and an unfinished CPU export must
never be treated as empty or complete.

## Solution

Add a terrain-only persistence vertical slice that streams the complete authoritative packed voxel
atlas to a versioned binary file one chunk at a time. The persisted boundary is every encoded voxel
byte, including all voxel types such as wood and all authoritative state bits in the schema. Logical
tree records, leaves, fruit, entities, and all other non-voxel state remain outside the file.

Version 1 uses an uncompressed raw chunk codec. The format includes a magic value, version,
dimensions, voxel schema metadata, canonical chunk identity, explicit decoded and stored lengths,
and checksums. It reserves a codec identifier for later compression without making compression a
v1 dependency.

Saving displays `Saving`, pauses mutable simulation, finishes visible terrain publication, flushes
the Contree CPU cache to the current revisions, then synchronously reads and writes all eight chunks
without allowing another frame to mutate the atlas. It writes a same-directory temporary file,
flushes it, and atomically replaces the selected path. Both CLI and Debug GUI saves overwrite an
existing destination without a confirmation prompt.

Loading performs two streaming passes. The first validates the entire file without mutating the
current world. The second uploads the already validated chunks into the authoritative GPU atlas.
A startup load bypasses procedural terrain initialization, the synthetic startup material stamp,
the startup tuning-tree stamp, and any other boot-time voxel mutation. It then rebuilds normal
derived systems from the loaded atlas.

A runtime load is terrain-only: it retains existing trees, entities, inventory, water particles,
and time state even though they may become spatially inconsistent with the replacement terrain.
The Debug GUI must state this limitation. Mutable simulation is paused throughout the load. The
operation reaches `Ready` only after visible terrain, the Contree CPU cache, and exact terrain
collision match the new atlas. Water simulation remains paused until the new water terrain cache is
ready. DDGI may continue rebuilding in the background through its normal invalidation and fallback
path.

Expose deterministic CLI operations:

- `--terrain-load <path>` loads a snapshot during startup;
- `--terrain-save <path>` saves once at the first coherent ready point;
- both options may be combined for a deterministic load-and-resave roundtrip;
- auto-exit cannot terminate the process before a requested CLI save completes.

Expose a minimal Terrain Snapshot section in the Debug GUI with an editable path, Save and Load
buttons, and explicit `Ready`, `Saving`, `Loading`, or `Error` status.

## User Stories

1. As a player, I want my terrain edits to survive restart, so that digging and filling are durable.
2. As a player, I want loaded terrain to remain editable, so that persistence does not produce a
   read-only render artifact.
3. As a player, I want every encoded voxel type and state bit restored exactly, so that the loaded
   atlas is the same authoritative source that was saved.
4. As a player, I want wood voxels to roundtrip even though logical tree state does not, so that the
   file does not silently filter authoritative voxel values.
5. As a player, I want startup loading to skip procedural terrain and startup stamps, so that the
   snapshot is not mutated during boot.
6. As a player, I want a clear Ready status, so that I know terrain, CPU queries, and exact collision
   agree.
7. As a player, I want a clear Saving status during the short blocking transaction, so that the
   pause is understandable.
8. As a player, I want a clear Loading status while simulation is paused and terrain-derived systems
   rebuild.
9. As a player, I want an actionable Error status, so that corruption, incompatibility, and I/O
   failures are not silent.
10. As a player, I want a corrupt file rejected before my current terrain changes, so that a failed
    load does not leave a half-old, half-new world.
11. As a player, I want a failed replacement save to preserve the previous file, so that an
    interrupted write is recoverable.
12. As a Debug GUI user, I want Save to overwrite the selected path directly, so that repeated
    iteration does not require a confirmation dialog.
13. As a Debug GUI user, I want runtime loading identified as terrain-only, so that I understand
    retained objects may float or become buried.
14. As a developer, I want the payload read from the authoritative GPU voxel atlas, so that it is
    not reconstructed from a sparse surface cache.
15. As a developer, I want Contree CPU readiness used only as a coherence gate, so that NotReady
    never becomes partial saved data.
16. As a developer, I want Save to flush transient CPU-cache lag automatically, so that a normal
    short-lived NotReady window is not presented as a user error.
17. As a developer, I want all terrain mutation paused during Save, so that all chunks represent one
    point in time.
18. As a developer, I want the entire file validated before GPU mutation, so that a bad late chunk
    cannot partially replace the world.
19. As a developer, I want each chunk to carry coordinates, codec, lengths, and checksum, so that
    records can be validated independently.
20. As a developer, I want header metadata checksummed, so that corrupted dimensions or schema
    identifiers are caught before allocation.
21. As a developer, I want duplicate, missing, out-of-range, truncated, and trailing records rejected,
    so that malformed files cannot create ambiguous worlds.
22. As a developer, I want all dimension products and byte lengths checked and bounded, so that a
    file cannot request unbounded memory.
23. As a developer, I want memory bounded to approximately one chunk plus staging, so that the
    implementation does not duplicate the full 128 MiB world in CPU memory.
24. As a developer, I want incompatible format versions, voxel schemas, and dimensions rejected, so
    that old data is never reinterpreted under a new layout.
25. As a developer, I want startup load failure to terminate with a non-success result, so that a
    procedural fallback cannot be mistaken for the requested snapshot.
26. As a developer, I want a runtime failure before GPU mutation to retain the current world, so that
    validation and filesystem failures are non-destructive.
27. As a developer, I want a runtime failure after GPU mutation begins to fail closed, so that a
    partially rebuilt world never resumes simulation.
28. As a developer, I want Surface, Contree, scene indirection, collision, water caches, and DDGI
    regenerated normally, so that no derived representation becomes a file-format commitment.
29. As a developer, I want stale pre-load asynchronous work rejected by revision, so that old
    collider, water, or DDGI results cannot overwrite the loaded world.
30. As a developer, I want entity physics paused until exact collision is rebuilt, so that objects do
    not interact with old collision under new visible terrain.
31. As a developer, I want water simulation paused until its terrain cache catches up, so that water
    particles do not cross replacement terrain using stale collision data.
32. As a developer, I want deterministic CLI load and save paths, so that persistence failures can be
    reproduced without GUI interaction.
33. As a developer, I want load-and-resave to produce byte-identical raw v1 files, so that roundtrip
    correctness can be checked mechanically.
34. As a developer, I want an automated edit-save-reload acceptance path, so that continued
    editability is proven rather than assumed.
35. As a maintainer, I want v1 to remain raw-only, so that format correctness is established before
    compression adds another failure mode.
36. As a maintainer, I want measured release-mode performance limits, so that queue stalls or rebuild
    regressions are visible.
37. As a maintainer, I want format, GPU integration, GUI/runtime behavior, and final validation
    delivered in focused commits, so that each boundary is reviewable.
38. As a user, I want the v1 scope stated explicitly, so that I do not assume logical trees, entities,
    inventory, water state, time, or other game state are persisted.

## Implementation Decisions

- The persisted payload is the complete one-byte-per-voxel encoding in the editable GPU atlas. No
  voxel type is filtered. Wood voxels are included; logical tree state is not.
- Current world metadata is a `2 x 2 x 2` chunk grid with `256 x 256 x 256` voxels per chunk. Each
  raw chunk is exactly 16 MiB and the full raw payload is 128 MiB plus negligible headers.
- Contree CPU data is never serialized. Before Save, the implementation completes visible terrain
  publication and synchronously flushes CPU-cache jobs to their current source revisions. Failure to
  reach readiness is an Error and writes no replacement file.
- Save is a short synchronous transaction. The app presents `Saving`, pauses terrain mutation,
  entity physics, water simulation, and game-time advancement, then reads, checksums, and writes all
  chunks before resuming.
- Version 1 is little-endian and contains magic, format version, header length, chunk-grid dimensions,
  voxels-per-chunk dimensions, bytes per voxel, voxel schema identifier, chunk count, record-header
  length, flags/reserved fields, and a header checksum.
- Every chunk record contains its three-dimensional coordinate, codec identifier, decoded byte
  length, stored byte length, payload checksum, and record-header checksum.
- Version 1 supports only the raw codec. Unsupported codec identifiers are rejected. Compression
  requires later measurements from real snapshot chunks and a separately specified codec.
- CRC-32 is sufficient for accidental corruption detection. It is not authentication.
- Writers use one canonical coordinate order. Readers validate uniqueness, range, exact coverage,
  lengths, and end-of-file rather than trusting order.
- Readers enforce conservative bounds on axes, chunk count, per-chunk bytes, and total logical file
  bytes before allocating.
- Loading always performs a complete validation pass followed by a separate upload pass. No GPU
  mutation occurs until magic, version, schema, dimensions, record headers, payload checksums, exact
  chunk coverage, and end-of-file all validate.
- Save writes a temporary file beside the destination, flushes and synchronizes it, then atomically
  replaces the destination where the platform supports it. Existing destinations are overwritten
  without confirmation in both CLI and Debug GUI flows.
- Startup terrain selection is explicit: procedural generation or snapshot upload. Snapshot upload
  never calls procedural chunk initialization and suppresses every startup voxel stamp, including
  the synthetic material block and tuning tree.
- CLI options that combine snapshot loading with a deterministic test scene that stamps terrain are
  rejected rather than silently choosing an order.
- After upload, every world chunk follows the normal Surface to Contree to scene-indirection rebuild
  path. The Contree CPU cache is then flushed to the loaded revisions.
- The full terrain domain receives a new published revision. Shadow history is reset. Exact terrain
  collision is rebuilt completely before `Ready`.
- Water terrain source and cache work is invalidated and rebuilt normally. Water particle state is
  retained but its simulation remains paused until the new terrain cache is ready.
- DDGI resources and history are not persisted. The full terrain domain is invalidated and DDGI may
  rebuild asynchronously through the existing fallback path without blocking persistence `Ready`.
- Runtime load preserves all non-terrain state. This can leave trees, entities, pipes, water, and
  other state spatially inconsistent; v1 documents rather than repairs that limitation.
- During Loading, terrain editing, entity physics, water simulation, and game time are paused. At
  `Ready`, terrain editing, entity physics, and time may resume. Water resumes independently when its
  new terrain cache is ready.
- A load error before mutation preserves the current world and reports Error. A GPU upload or
  derived-rebuild error after mutation begins freezes simulation in fatal Error and requires exit or
  restart; v1 does not retain a 128 MiB rollback snapshot.
- `--terrain-load` fails fast with a non-success process result for missing, corrupt, unsupported, or
  incompatible files. It never falls back to procedural terrain.
- `--terrain-save` saves once after coherent readiness. When combined with `--terrain-load`, loading
  and required derived rebuilding complete before saving. Auto-exit cannot preempt the save.
- The Debug GUI exposes an editable path and direct Save/Load actions. Its status includes useful
  phase detail while remaining one of Ready, Saving, Loading, or Error.
- Stable logs report operation, path, format version, dimensions, chunk count, logical bytes,
  checksums or checksum summary, phase transitions, and outcome. A partial operation is never logged
  as success.
- Release-mode performance acceptance on the current validation machine is:
  - complete raw world Save at or below 1 second;
  - validation, atlas upload, and visible terrain rebuild at or below 0.5 seconds;
  - exact terrain collision Ready at or below 5 seconds;
  - water-cache and DDGI convergence measured and logged but not gated by those persistence limits.
- If the first two budgets fail, investigate repeated queue-idle waits and unnecessary staging before
  considering compression.
- Implementation proceeds in focused validated commits: format and pure tests; atlas and startup CLI
  integration; Debug GUI/runtime load and invalidation; deterministic end-to-end acceptance and final
  validation fixes.

## Testing Decisions

- The primary seam is the highest-level streaming snapshot contract: metadata plus authoritative
  chunk payloads enter the writer, an atomic file is produced, the complete file validates, and the
  same chunks stream back. Most correctness tests remain deterministic and Vulkan-free.
- Tests assert externally observable format behavior rather than private helper structure.
- Determinism tests write identical metadata and chunk payloads twice and require byte-identical
  files.
- Roundtrip tests use distinct multi-chunk fixtures and require identical metadata, chunk identity,
  full encoded bytes, and checksum results.
- Atomic replacement tests require a successful second save to replace the old snapshot and an
  unfinished or failed save to leave the old destination unchanged.
- Header error tests cover bad magic, unsupported version, bad header length, incompatible schema,
  dimension mismatch, invalid bytes per voxel, impossible chunk count, checksum failure, arithmetic
  overflow, and configured bound violations.
- Chunk error tests cover duplicate, missing, and out-of-range coordinates; unsupported codec;
  decoded/stored length mismatch; record-header and payload checksum failure; truncation; and
  trailing bytes.
- CLI parser tests cover load, save, combined load/save, missing values, incompatible terrain-stamping
  test scenes, and default absence.
- Loading-strategy tests require snapshot startup to select atlas upload and suppress procedural
  terrain, synthetic materials, and startup tree stamping.
- Save-coherence tests require visible publication completion and CPU-cache flush before the first
  atlas readback. Flush failure must preserve the previous destination.
- Pause-state tests require terrain editing, entity physics, water simulation, and game time to stop
  during runtime load. They require water to remain paused after terrain Ready until the replacement
  terrain cache is ready.
- Derived-publication tests observe the highest existing seam: every chunk is rebuilt, the CPU cache
  reaches the loaded revisions, full-domain terrain revision changes, exact collision completes,
  water work is invalidated, and DDGI receives full-domain invalidation.
- Pre-mutation load failures must leave the current terrain revision and atlas unchanged. A simulated
  post-mutation failure must enter fatal Error and never resume simulation.
- A release hidden roundtrip saves a coherent startup world, loads it, resaves it, and requires
  byte-identical raw files or identical stable checksums.
- Continued editability is a required automated acceptance path: load a baseline, mutate one fixed
  small region through the normal terrain edit/publication path, wait for CPU cache and collision,
  save the edited snapshot, verify only expected chunk checksums changed, and load the edited snapshot
  successfully.
- Performance acceptance records Save, validation, upload, visible rebuild, CPU-cache flush, exact
  collision, water-cache, and DDGI phases separately in a release hidden run.
- Repository validation remains `cargo fmt --check`, `cargo check`, `cargo test`, a release hidden
  muted run, and same-worktree latest-log inspection. Runtime GUI config diffs are suspicious unless
  intentionally required.

## Out of Scope

- Logical tree records, growth state, leaves, fruit ownership, and procedural-tree state.
- Entities and placeables, including sprinklers, pipes, butterflies, particles, and rigid bodies.
- Inventory, selected tools, player state, camera state, and GUI preferences.
- Water particle and simulation persistence. Retained runtime water is not restored from the file.
- Time of day, weather, wind, audio, and DDGI radiance history.
- Persisting Surface output, flora instances, Contree nodes/leaves, scene indirection, collision shapes,
  acceleration structures, terrain SDF grids, water caches, or DDGI resources.
- Autosave, save slots, cloud synchronization, delta saves, journaling, or multiplayer sync.
- Application-layer compression in v1.
- Migration, resampling, or conversion across format versions, voxel schemas, or world dimensions.
- Runtime rollback after GPU mutation begins.
- Security-grade authentication or encryption.
- A native file picker or polished save-management UX.
- Repairing retained non-terrain state after a terrain-only runtime load.

## Further Notes

- Raw v1 is intentionally acceptable at roughly 16 MiB per chunk and 128 MiB per world. Atomic
  replacement can temporarily require both old and new files, roughly 256 MiB of logical free space.
- The validation machine stores the checkout on Btrfs with transparent `zstd:1`; physical disk usage
  may be lower than logical raw size, but the format must not depend on filesystem compression.
- A local incompressible-data probe measured roughly 90 ms to write and synchronize 128 MiB, 30 ms
  for a direct 128 MiB read, and 10 ms for a checksum scan. GPU queue synchronization and derived
  rebuilding are therefore the more important measurements.
- The existing visual terrain decision remains unchanged: visible publication stays synchronous,
  while water and DDGI follow-up remains revisioned. Persistence adds a stronger exact-collision
  Ready boundary for runtime replacement safety.
- The Contree CPU surface shell and the filled authoritative GPU atlas have intentionally different
  semantics. Persistence must never substitute the sparse CPU cache for the voxel payload.
- Future compression should be evaluated only with real exported chunks. A proposed codec must show
  useful portable size reduction and acceptable release-mode compression/decompression time while
  retaining decoded-length and checksum validation.
