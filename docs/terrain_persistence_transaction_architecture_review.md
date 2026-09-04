# Terrain Persistence Transaction Architecture Review

Date: 2026-08-30

Reviewed HEAD: `4ce1321fea3aa588982ba997c490e2001c0b3e0e`

Candidate strength entering review: Worth exploring

Decision: Reject at the depth gate

## Scope and constraints

This review asks whether runtime terrain persistence should move from `App` methods into a new
transaction interface. An acceptable change must materially shrink the interface, hide transaction
and failure policy, and reuse the existing Visible Terrain Publication and terrain-connectivity
owner. It must preserve save/load behavior, flora, non-terrain state, and recovery semantics.

Generic traits, hypothetical adapters, empty facades, file-only moves, borrow workarounds, and a
second publication or connectivity owner are out of scope.

## Current HEAD audit

The candidate premise is no longer accurate at this HEAD. Runtime callers do not arrange pause,
flush, file I/O, atlas upload, publication, connectivity, physics, water, or resume themselves.
Those operations are already local to `src/app/core/terrain_persistence.rs`, introduced as a
concentrated module by `165b6563` (`refactor: deepen terrain persistence transaction`). The GUI
caller issues only `perform_runtime_terrain_load()` or `perform_runtime_terrain_save()`, and the
frame loop makes one semantic water-readiness observation.

The current module hides these invariants:

- Save quiesces water, waits for the GPU, flushes the Contree CPU cache, reads the complete atlas,
  atomically replaces the snapshot, and restores the running simulation gate on success or failure.
- Load validates and opens the complete snapshot before quiescence and before authoritative terrain
  mutation. A failure before the first atlas write is recoverable; a failure after mutation freezes
  world and water updates and requires restart.
- A successful load allows world updates but keeps water paused and prevents another persistence
  operation until the rebuilt water terrain reaches Ready.
- Snapshot replacement uses `BuildEdit::RebuildChunksWithoutFlora`, so loading terrain does not
  regenerate flora or replace trees, entities, water state, or time.
- `VisibleTerrainPublication::snapshot_replacement` owns the semantic order: physical publication,
  edit observers, loaded-world connectivity reconciliation, collider import, and water-terrain
  enqueue.
- The publication host calls the existing `reconcile_loaded_terrain_publication()` owner. Terrain
  persistence does not duplicate connectivity state or policy.

Startup is intentionally different from the synchronous runtime transaction. Startup load feeds
validated chunks incrementally through `LoadingState`. Startup save runs after physical/collider
settlement but before startup observers, and propagates an error to the fatal startup caller. A
single interface would need to expose these variant-dependent ordering and error rules.

## Dependency classification

- `PlainBuilder`, water, Vulkan, Contree, terrain physics, player tools, Visible Terrain
  Publication, and terrain connectivity are in-process dependencies with concrete owners.
- Snapshot files are local-substitutable. The concrete reader/writer already have tempfile-backed
  deterministic, corruption, bounds, and atomic-replacement tests; a filesystem port adds no real
  adapter.
- Visible Terrain Publication already has a real seam with the `App` adapter and the test
  `RecordingHost` adapter. A second persistence host would have only one production adapter and
  would be a hypothetical seam.

## Interface designs considered

### 1. Minimal event interface

```rust
enum TerrainPersistenceEvent<'a> {
    StartupSave(&'a Path),
    RuntimeSave,
    RuntimeLoad,
    WaterTerrainAdvanced,
}

impl App {
    fn terrain_persistence(
        &mut self,
        event: TerrainPersistenceEvent<'_>,
    ) -> Result<TerrainPersistenceOutcome>;

    fn terrain_persistence_view(&mut self) -> TerrainPersistenceView<'_>;
}
```

This has a small type-level surface, but the event method must expose startup-versus-runtime error
behavior and the view must still expose path editing, readiness, status, world gating, and water
gating. It compresses names without reducing what callers must know.

Deletion test: deleting the event method restores the four existing semantic methods in the same
module; transaction complexity does not spread across callers. The proposed module is an empty
facade.

### 2. Flexible command transaction

```rust
enum TerrainPersistenceCommand<'a> {
    StartupSaveTo(&'a Path),
    Runtime(RuntimeTerrainPersistence),
}

enum RuntimeTerrainPersistence {
    SaveSelected,
    ReplaceFromSelected,
}

impl App {
    fn transact_terrain_snapshot(
        &mut self,
        command: TerrainPersistenceCommand<'_>,
    ) -> Result<TerrainPersistenceDispatch>;

    fn observe_terrain_snapshot_settlement(&mut self);
}
```

This unifies invocation shapes but enlarges the interface contract: startup errors propagate,
runtime errors become observable status, pre-mutation load failures resume, post-mutation failures
freeze, and successful runtime load remains incomplete until water terrain is Ready. There is no
second storage source, execution mode, or concurrent transaction to justify this flexibility.

Deletion test: deleting the command type leaves the existing transaction implementation local to
the same file. YAGNI and the depth gate both fail.

### 3. Common-caller command

```rust
enum RuntimeTerrainSnapshot {
    SaveSelected,
    LoadSelected,
}

impl App {
    fn execute_terrain_snapshot(&mut self, command: RuntimeTerrainSnapshot);
    fn observe_terrain_persistence_water(&mut self);
}
```

This makes the GUI branch marginally shorter, but adds an enum to replace two already-semantic
entry points. Moving egui controls behind the same interface would put the seam in presentation
code while startup and frame policy still remain outside.

Deletion test: deleting the command restores two one-line calls. No transaction knowledge moves
back to the caller, so the command provides no meaningful leverage or locality.

## Depth-gate decision

All three designs fail the deletion test: current transaction complexity is already concentrated,
and deleting any proposed interface does not distribute it across multiple callers. They also fail
YAGNI because there is one concrete application owner, one snapshot format/filesystem path, and no
new operation family.

Making `TerrainPersistenceRuntime` directly own the concrete orchestration would require one of:

- a borrowed resource bundle mirroring a large portion of `App`;
- callbacks exposing pause/read/write/publish/connectivity/collider stages;
- a new host trait with one production adapter;
- temporarily taking/replacing runtime state to evade Rust borrowing; or
- moving shared GPU, water, physics, and connectivity ownership into persistence.

The first two create a larger, shallower internal interface. The third is a prohibited hypothetical
seam. The fourth is a workaround. The fifth is a broad ownership migration without a second use
case and would conflict with existing owners.

The current receiver being `App` is an implementation and Rust ownership detail, not evidence that
callers know the transaction. The existing terrain-persistence module already provides locality and
leverage, while Visible Terrain Publication and terrain connectivity retain their established
ownership. No runtime code change is justified.

## Regression anchors

Any future review must retain these tests and observable contracts:

- `app::core::terrain_persistence::tests::successful_load_waits_for_water_without_freezing_world_updates`
- `app::core::terrain_persistence::tests::load_failure_before_mutation_resumes_but_failure_after_mutation_freezes`
- `app::core::terrain_persistence::tests::runtime_snapshot_replacement_does_not_regenerate_flora`
- `app::core::visible_terrain::tests::snapshot_replacement_has_one_ordered_semantic_completion`
- `app::core::visible_terrain::tests::loaded_connectivity_child_completes_before_collider_import`
- the deterministic, corruption, bounds, and atomic-replacement tests in `src/terrain_persistence.rs`

Reopen this decision only if a second real persistence caller or storage adapter appears, startup
and runtime acquire the same failure contract, or ownership changes make a materially smaller
concrete interface possible without duplicating publication or connectivity policy.
