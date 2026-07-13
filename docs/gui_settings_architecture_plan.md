# Debug GUI Settings Architecture Plan

## Purpose

Make persistence an architectural property of every editable setting shown in the Debug Panel, rather than an optional second integration step that developers must remember.

The primary invariant is:

> An editable control presented as a Debug Panel setting edits the authoritative persisted settings state directly, and the top-level Save action saves that complete state.

This document is an implementation plan only. It does not prescribe a visual redesign of the panel and does not include implementation changes.

## Success Criteria

The redesign is complete when:

- Every editable control presented as a Debug Panel setting has an explicit persistence policy.
- Controls presented as normal settings are persistent by default; silently non-persistent settings are not allowed.
- The Debug Panel has one settings owner with one load path, one UI path, and one save path.
- Adding a setting cannot require unrelated manual wiring in `App`, the panel renderer, and the save button.
- The UI edits the authoritative desired settings state, not a runtime copy that must later be copied back.
- CLI overrides and continuously changing runtime state do not accidentally overwrite persisted desired values.
- Existing `config/gui.toml` files continue to load, including files created before the Tree section existed.
- A full non-default settings round trip is covered by deterministic tests.
- Saving uses an atomic file replacement and reports failures in the panel, not only in logs.

## Scope

Included:

- The editable settings shown under the Debug Panel's top-level Save button.
- Generic `GuiAdjustables` parameters.
- Custom Flora layout over generic parameters.
- Dynamic wind-source settings.
- Tree generation settings and `Render Leaves`.
- Persistence policy for live values such as time of day.
- Interaction between persisted settings, CLI overrides, and effective runtime state.
- Config schema/value ownership and compatibility strategy.
- Tests and staged migration boundaries.

Not included:

- Camera snapshots. They are named external resources with their own explicit save workflow and file.
- Read-only diagnostics such as updating flora chunk counts and frame timing.
- Gameplay tool selection and other ordinary UI state.
- A visual redesign of the Debug Panel.
- Implementing the plan in this branch.

## Current Architecture

### Relevant files

- `config/gui.toml`
  - Stores generic section metadata, ranges, labels, and values.
  - Now also stores the typed `[tree]` object.
  - Is read by both runtime code and `build.rs`.
- `build.rs`
  - Parses the generic `[[section]]` declarations.
  - Generates `src/app/generated/gui_adjustables_gen.rs`.
- `src/app/gui_config_model.rs`
  - Defines `GuiConfigFile`, generic sections/parameters, and `TreeGuiConfig`.
- `src/app/gui_config_loader.rs`
  - Loads, validates, and rewrites `config/gui.toml` at a fixed project path.
- `src/app/gui_config.rs`
  - Materializes `GuiAdjustables`.
  - Renders generic parameters, custom Flora controls, and wind sources.
  - Copies runtime values back into a newly loaded config during Save.
- `src/app/core/mod.rs`
  - Owns `gui_config`, `gui_adjustables`, `wind_sources`, `debug_tree_desc`, and `render_flags` separately.
  - Composes the Debug Panel and invokes Save.
  - Injects the Tree controls through an `after_section` callback.
- `src/tree_gen/tree.rs` and `src/branching_gui.rs`
  - Render the typed Tree controls.
- `src/cli.rs`
  - Defines `RenderFlags`, including the runtime `enable_leaves` flag.

### Current editable-setting paths

| Setting family | UI state owner | Render path | Save path |
| --- | --- | --- | --- |
| Generic parameters | `GuiAdjustables` | Generic config renderer | ID/type loop copies values into reloaded config |
| Flora custom layout | `GuiAdjustables` | Special renderer for the `Flora` section | Same generic ID/type loop |
| Wind sources | `Vec<WindSourceGuiValues>` | Special renderer for the `Wind` section | Special parameter removal/regeneration |
| Tree description | `App::debug_tree_desc` | Callback injected after `Debug` | Explicit argument passed to Save |
| Render Leaves | `RenderFlags::enable_leaves` | Tree callback | Explicit argument passed to Save |
| Audio ray tracing | `GuiAdjustables` | Extra checkbox appended in `App` | Generic ID/type loop |
| Time of day | `GuiAdjustables` | Generic renderer | Explicitly skipped by `SAVE_DENYLIST` |

Camera snapshots have a separate save button and file and should remain outside this contract. Flora growth text and timing data are diagnostics, not editable settings.

## Root Cause

The current system has several owners but no aggregate settings boundary.

A custom setting requires a developer to remember all of these steps:

1. Define its data shape.
2. Deserialize it at startup.
3. Add an `App` field or reuse an unrelated runtime field.
4. Render its control.
5. Pass it to the Save entry point.
6. Copy it into `GuiConfigFile`.
7. Apply it back to runtime systems.
8. Add round-trip coverage.

The compiler cannot detect a missing step. The UI renderer can access arbitrary `App` state, while the save method only knows about the values listed in its arguments. The previous Tree bug was therefore a predictable result of the architecture, not an isolated oversight.

The current Save signature is a visible symptom: every custom setting family makes the argument list grow. Reloading the config from disk and manually copying selected runtime values into it also means persistence coverage is defined by imperative code rather than by the settings model.

## Design Principles

### 1. One owner for desired settings

The application should own one aggregate `DebugSettings` value. All values shown as editable settings belong to it.

Conceptually:

```rust
struct DebugSettings {
    schema: GuiSchema,
    values: DebugSettingsValues,
    saved_revision: u64,
    current_revision: u64,
}

struct DebugSettingsValues {
    generated: GuiAdjustables,
    wind: WindSettings,
    tree: TreeSettings,
}
```

Exact names are flexible. The ownership boundary is not.

### 2. UI edits persisted desired state directly

A setting control must bind to a field inside `DebugSettingsValues`. Save serializes that same state.

Avoid:

```text
UI -> RenderFlags.enable_leaves
Save -> copy RenderFlags.enable_leaves into TreeGuiConfig
```

Prefer:

```text
UI -> settings.values.tree.render_leaves
Save -> serialize settings.values
Runtime -> derive effective render-leaves from settings + CLI constraints
```

This removes synchronization as a failure mode.

### 3. Desired state and effective runtime state are different concepts

Persisted settings express what the user wants. Effective runtime state may additionally depend on:

- CLI overrides.
- Hardware/capability limits.
- Temporary diagnostic modes.
- Derived invariants between multiple settings.
- Live simulation state.

The dependency direction must be one-way:

```text
persisted desired settings + startup/CLI constraints -> effective runtime state
```

Runtime state must not be copied back into desired settings merely because it is convenient for rendering.

### 4. Persistent is the default policy

Every editable control in the settings area must declare one of:

- `Persistent`: included in the top-level Save operation.
- `SessionOnly`: intentionally temporary and visibly labelled as such.
- `ExternalResource`: has an explicit independent save workflow, such as camera snapshots.

No hidden denylist is allowed. A normal-looking slider silently excluded by ID violates the panel contract.

### 5. Read-only runtime values are not settings

Continuously changing values should be displayed as status, or split into desired and observed fields.

For example:

```text
Initial/Manual Time of Day  editable and persistent
Current Time of Day         read-only live status
Auto Day/Night Cycle        editable and persistent
```

This is preferable to showing the live clock as a normal slider and silently skipping it during Save.

### 6. Complex typed groups are first-class

Not every setting should be flattened into generic `GuiParam` records. Tree and wind have custom structure and UI behavior. They should remain typed groups, but they must live under the same aggregate settings owner and use the same load/draw/save lifecycle.

## Target Responsibilities

### `DebugSettings`

Owns:

- The immutable or runtime-loaded generic UI schema.
- All desired values for generic and typed settings groups.
- Dirty/saved revision state.
- Config path or persistence backend abstraction.

Provides:

```rust
impl DebugSettings {
    fn load(source: &impl SettingsStore) -> Result<Self>;
    fn draw(&mut self, ui: &mut egui::Ui) -> SettingsUiResponse;
    fn save(&mut self, store: &impl SettingsStore) -> Result<()>;
    fn values(&self) -> &DebugSettingsValues;
}
```

`draw` is the only entry point that renders editable Debug Panel settings. It can internally delegate to typed sections.

### `DebugSettingsValues`

Owns the authoritative desired values. It should be serializable as a complete unit, or have complete generated serialization code with equivalent compile-time coverage.

It should not own:

- GPU resources.
- Particle handles.
- Simulation objects.
- CLI options.
- Read-only statistics.

### Typed settings groups

Examples:

```rust
struct TreeSettings {
    render_leaves: bool,
    desc: TreeDesc,
}

struct WindSettings {
    sources: Vec<WindSourceSettings>,
    // Wind-wide settings can remain generated values initially.
}
```

A lightweight static interface is sufficient:

```rust
trait SettingsSection {
    fn draw(&mut self, ui: &mut egui::Ui) -> bool;
    fn validate(&self) -> Result<()>;
}
```

Dynamic trait-object registration is not required. Static composition is simpler, more strongly typed, and easier to navigate. The important property is that every custom section is a field of the aggregate serialized values.

### `SettingsStore`

Persistence should be separated from the project-root path so unit tests can use temporary files.

Conceptually:

```rust
trait SettingsStore {
    fn load_text(&self) -> io::Result<String>;
    fn save_text_atomically(&self, text: &str) -> io::Result<()>;
}
```

Production can use the current path initially. Tests can inject a temporary path without mutating `config/gui.toml`.

### Application/runtime consumers

Runtime systems consume desired settings or derived snapshots. They do not own a second mutable copy of the same setting.

Examples:

```text
effective_render_leaves =
    render_flags.enable_flora && settings.tree.render_leaves
```

```text
active_wind_sources = derive(settings.wind.sources)
```

A system that needs expensive reconfiguration can track relevant setting revisions or respond to a typed `SettingsChanges` result from the UI.

## Generic Parameters Versus Typed Groups

The existing data-driven generic parameter system remains useful. It allows labels, sections, ranges, and values to be tuned without hand-writing each egui control.

The recommended hybrid is:

- Generic scalar settings remain generated from schema declarations.
- Complex/list settings remain typed Rust structures with custom UI.
- Both are fields of `DebugSettingsValues`.
- Both participate in one Save operation.
- The panel renderer cannot reach unrelated mutable `App` fields.

The generic generator should eventually generate complete value conversion or serialization code. Hand-written type matching and ID lookup should not define persistence coverage.

Possible generated APIs:

```rust
impl GuiAdjustables {
    fn from_persisted(schema: &GuiSchema, values: &GenericValues) -> Result<Self>;
    fn to_persisted(&self) -> GenericValues;
}
```

or direct serde support for generated values. Either is acceptable if adding a generated field automatically adds both load and save behavior.

## Config File Direction

### Short-term compatibility target

Keep accepting the current `config/gui.toml` shape while introducing the aggregate runtime owner. This minimizes migration risk and avoids coupling a structural refactor to a format migration.

The load boundary should normalize:

- Legacy files without `[tree]`.
- Current files with `[tree]`.
- Existing generic `[[section]]` parameter records.
- Existing generated wind-source parameter records.

Downstream code should only see the normalized `DebugSettingsValues` representation.

### Recommended long-term split

The current file combines two responsibilities:

1. Schema: IDs, kinds, labels, sections, ranges, choice options.
2. Persisted values: the user's current/default tuning.

It is also both a runtime-writable file and a `build.rs` input. This creates unnecessary coupling and makes ordinary Save operations look like source/build-input changes.

A cleaner final shape is:

```text
config/gui_schema.toml    immutable definitions used by build.rs
config/gui_defaults.toml  repository defaults
a user/project values file  saved overrides
```

Whether the writable values file belongs in the repository or an OS user-data directory is a product decision:

- Internal developer-tuning panel: a project-local tracked defaults file may be intentional.
- Player settings: write to a user-data directory and leave repository defaults immutable.

This decision should be made before implementing the final file split, but it does not block the ownership refactor.

### Save semantics

Save should:

1. Validate the complete desired settings object.
2. Serialize all persistent groups.
3. Write to a temporary sibling file.
4. Flush/close it as appropriate.
5. Atomically rename it over the destination.
6. Update the saved revision only after success.
7. Surface success or failure in the panel.

Do not partially update the existing file in place. A serialization or write failure must leave the previous valid config intact.

## UI Composition Contract

The Debug Panel should be divided conceptually into:

1. **Persistent Settings**
   - Rendered exclusively by `DebugSettings::draw`.
   - Covered by the top-level Save button.
2. **External Resources**
   - Camera snapshot library and similar resources.
   - Have their own explicit actions and status.
3. **Runtime Diagnostics**
   - Read-only chunk counts, timing, and system status.
4. **Session Controls**, only if unavoidable
   - Clearly marked `Session only` next to the control.

`App` may compose these blocks, but it must not append an ordinary editable setting after `DebugSettings::draw`. This prevents controls such as the current extra Audio Ray Tracing checkbox from quietly bypassing the centralized rendering contract, even if that specific field currently happens to save through `GuiAdjustables`.

## Special Cases

### Render Leaves and CLI `--no-flora`

`TreeSettings::render_leaves` should remain the desired persisted value.

`--no-flora` is an effective runtime constraint. Starting with `--no-flora` must not overwrite or save `render_leaves = false`. Otherwise a temporary CLI diagnostic choice becomes a persistent preference.

Use:

```text
effective.enable_leaves =
    cli_allows_flora && settings.tree.render_leaves
```

Do not mutate the desired setting to enforce the constraint.

### Time of day

The current `SAVE_DENYLIST` is a hidden persistence exception and should be removed as a mechanism.

Choose one explicit model:

- Persist the editable time value, accepting that Save captures the current displayed time; or preferably
- Separate a persistent initial/manual time from a read-only current live time.

The second model better represents automatic cycling and makes Save behavior predictable.

### Water CLI overrides

Startup currently applies persisted water values, applies CLI overrides, and then synchronizes the effective water config back into GUI adjustables. This can make a temporary CLI override appear as a desired GUI value and later be persisted by Save.

The redesign must keep these separate:

```text
settings water values -> desired
CLI water profile/options -> runtime override
water simulation config -> effective derived config
```

If the UI should display effective values, it must label them as overridden and provide an explicit action to adopt them. It must not silently replace desired values.

### Wind sources

Wind sources are a dynamic list and currently masquerade partly as generated generic parameters. The migration must preserve:

- Source order.
- Names.
- Active/muted state.
- All noise and gain fields.
- Add/delete behavior.
- Wind source count consistency.

Long term, serialize them as a typed list. Avoid maintaining both `wind_source_count` and vector length as independent authorities; vector length should be authoritative and count should be derived.

### Tree regeneration

Editing Tree settings can trigger expensive regeneration. Persistence and runtime application are separate concerns:

- A field changing marks settings dirty.
- Relevant changes request tree regeneration.
- Save writes settings but does not itself need to regenerate.
- Regeneration does not imply Save.

A typed `SettingsChanges` result can report groups such as `TREE_GEOMETRY`, `TREE_RENDER`, `WIND`, or `WATER` without coupling persistence to side effects.

### Camera snapshots

Camera snapshots should stay outside `DebugSettingsValues` because they are a collection of named resources with independent create/update/delete semantics. Their separate save UI must remain visually distinct from the top-level settings Save.

### Unknown/new fields

Decide the compatibility policy explicitly:

- Reject unknown fields to catch typos and version mismatch; or
- Preserve unknown fields across a save for forward compatibility.

Silently discarding unknown fields is the worst option. For a developer-owned config, strict rejection with a clear error is likely simplest. For user configs across versions, preservation or migrations may be preferable.

## Phased Implementation Plan

Each phase should be a separate validated commit. Avoid combining config-format migration with the initial ownership refactor.

### Phase 0: Record and test the current persistence contract

Objective:

- Establish a behavior baseline before moving ownership.

Work:

- Inventory every editable control in the Debug Panel.
- Classify each as persistent, external resource, session-only, or read-only.
- Add tests around current generic, Tree, and wind round trips where possible.
- Document the intentional decision for time of day.

Acceptance:

- The inventory has no unclassified editable controls.
- Tests reproduce the expected current config shape.
- No runtime behavior changes.

Likely files:

- `src/app/gui_config_model.rs`
- `src/app/gui_config.rs`
- New test helpers only

### Phase 1: Introduce the aggregate settings owner

Objective:

- Create one in-memory owner without changing config format or UI appearance.

Work:

- Add `DebugSettings` containing schema/config metadata, generic adjustables, typed Tree settings, and wind settings.
- Add a single load constructor that normalizes legacy/current config.
- Replace the separate `App` ownership fields with the aggregate.
- Keep existing rendering and save internals temporarily behind the aggregate methods.

Acceptance:

- `App` has one Debug Settings field rather than separate config/value/tree/wind fields.
- Startup behavior and current config output remain unchanged.
- No generated file is edited by hand.

Likely files:

- `src/app/gui_config.rs` or a new focused `src/app/debug_settings.rs`
- `src/app/core/mod.rs`
- `src/app/mod.rs`

### Phase 2: Close the UI boundary

Objective:

- Make it structurally difficult to render an unowned editable setting.

Work:

- Move all persistent-setting UI composition under `DebugSettings::draw`.
- Remove the `after_section` callback used to inject Tree controls.
- Move the extra Audio Ray Tracing checkbox into the owned settings renderer.
- Leave camera snapshots and read-only diagnostics in explicitly separate blocks.
- Return a typed change summary for runtime side effects.

Acceptance:

- `App` does not pass arbitrary mutable setting fields into the renderer.
- No ordinary editable setting is appended outside the settings owner.
- Panel layout and labels remain behaviorally equivalent unless intentionally clarified.

### Phase 3: Make persistence complete by construction

Objective:

- Remove hand-maintained custom arguments and implicit omissions from Save.

Work:

- Replace `save_to_config_with_wind_sources(...)` with `debug_settings.save(...)`.
- Serialize/capture the complete `DebugSettingsValues` state.
- Generate generic load/save conversion together so every generated field gets both paths.
- Serialize Tree and wind as typed groups.
- Remove the save denylist.
- Validate all groups before writing.

Acceptance:

- Save accepts no per-setting arguments.
- Adding a generated scalar automatically adds load and save handling.
- Adding a custom section requires placing it in `DebugSettingsValues`; omission is visible in type construction and round-trip tests.
- No normal setting can be rendered but absent from the serialized model.

### Phase 4: Separate desired settings from effective runtime state

Objective:

- Prevent temporary runtime constraints from contaminating persistence.

Work:

- Derive leaf rendering from Tree settings and CLI flora constraints.
- Stop using mutable `RenderFlags` as the owner of `Render Leaves`.
- Separate water desired values from CLI-derived effective water config.
- Resolve the time-of-day desired/live-state model.
- Audit other startup synchronization functions for reverse copies into desired values.

Acceptance:

- Running with CLI overrides and clicking Save does not persist those overrides unless the user explicitly adopts them.
- Automatic simulation changes do not dirty settings.
- UI clearly distinguishes desired values from overridden/read-only effective values.

### Phase 5: Harden persistence and feedback

Objective:

- Make Save robust and observable.

Work:

- Inject a settings store/path.
- Use temporary-file plus atomic-rename writes.
- Track dirty and saved revisions.
- Display save success/failure and unsaved state in the panel.
- Optionally warn on close/reload with unsaved changes.

Acceptance:

- Failed saves preserve the previous valid file.
- Tests do not touch the repository config.
- A save failure is visible to the user.

### Phase 6: Split schema/defaults/writable values

Objective:

- Remove the coupling between build-time schema and runtime-writable state.

Work:

- Move IDs, kinds, labels, ranges, and choices to an immutable schema source.
- Move defaults and writable values to their chosen locations.
- Add migration from the combined v1 file.
- Update `build.rs` to depend only on immutable schema.

Acceptance:

- Clicking Save does not modify the code-generation schema.
- Existing v1 files load and migrate safely.
- Build-generated descriptors are stable when only values change.

This phase is optional until the product decision about developer defaults versus per-user settings is made.

### Phase 7: Remove compatibility scaffolding

Objective:

- Finish the migration after at least one stable compatibility period.

Work:

- Remove obsolete save helpers and callback APIs.
- Remove old Tree/wind flattening only after migration coverage exists.
- Update README and contributor guidance.

Acceptance:

- There is one documented way to add a simple setting and one documented way to add a complex settings group.
- No old manual synchronization path remains.

## Testing Strategy

### Pure unit tests

- Full `DebugSettingsValues` round trip with every field/group changed from default.
- Legacy config without `[tree]` loads with defaults.
- Current Tree and wind data round trip without loss.
- Generic generated field inventory matches persisted generic field inventory.
- Choice/range/type validation produces clear errors.
- Interdependent values normalize or reject deterministically.
- Unknown-field policy behaves as documented.
- CLI constraints derive effective state without mutating desired state.
- Time-of-day cycling does not alter its persisted initial/manual value.
- Water CLI overrides do not alter desired water settings.

### Persistence tests

Using a temporary store/path:

1. Load fixture.
2. Mutate every settings group.
3. Save.
4. Load again.
5. Compare complete desired values.
6. Confirm schema metadata is unchanged.

Also simulate serialization/write/rename failure and verify that the previous file remains valid.

### UI contract tests

Direct egui interaction tests are optional. Prefer structural guarantees:

- Editable settings are rendered only through `DebugSettings::draw`.
- Typed sections are statically composed fields of `DebugSettingsValues`.
- Generated control descriptors and generated value conversion share one source declaration.

A small UI smoke test may still verify that each expected section is present.

### Runtime validation

For phases that touch application wiring:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Manual round-trip acceptance:

1. Launch normally.
2. Change at least one generic scalar, color/bool, wind source, Tree field, and Render Leaves.
3. Save.
4. Exit and restart.
5. Verify all values return exactly.
6. Repeat under `--no-flora` and a water CLI override; confirm those temporary constraints are not persisted.

## Easy-to-Miss Failure Points

### Ownership and synchronization

- Keeping both `TreeSettings::render_leaves` and a separately mutable `RenderFlags::enable_leaves` creates two authorities.
- Copying effective runtime values back into settings at the end of startup can persist CLI overrides.
- A custom control rendered from an arbitrary `App` field can bypass persistence again.
- A renderer callback is an escape hatch unless its state is owned by `DebugSettingsValues`.

### Generic code generation

- Generated files must be regenerated from their source; never hand-edit `src/app/generated/gui_adjustables_gen.rs`.
- Generation must cover load and save together. Generating only fields/load code leaves persistence manual.
- IDs need global uniqueness, stable naming, and explicit migration if renamed.
- Rust identifiers and config IDs may diverge; conversion must remain generated and validated.
- Choice option reordering can change meaning while preserving the numeric value. Prefer stable symbolic values if choices become user-facing.

### Serialization and compatibility

- `#[serde(default)]` helps additive compatibility but can hide misspelled field names if unknown fields are silently accepted.
- Renaming or moving fields needs aliases or an explicit schema migration.
- Saving an older in-memory schema over a file modified externally can lose new fields.
- Reloading schema from disk during Save can mismatch the schema used to build/render the current in-memory values.
- Dynamic lists must not have a separately persisted count that can disagree with the list length.
- Float normalization must apply consistently to generic and typed groups if stable diffs matter.

### Runtime behavior

- Tree setting changes and Save are independent: do not accidentally make Save regenerate trees twice.
- Disabling leaf rendering should affect future leaf emission/rendering as designed, not delete unrelated existing particles.
- Auto day/night cycling must not mark settings dirty every frame.
- Enforcing min/max relationships in the UI can mutate a second field; dirty tracking must include both.
- Expensive setting application should be revision/change-group driven, not rerun every frame merely because settings are centrally owned.

### Save reliability

- Direct `std::fs::write` can leave a truncated config if interrupted.
- A successful serialization followed by failed rename must not report success.
- Save status only in logs is insufficient for an interactive button.
- The saved revision must advance only after the atomic replacement succeeds.
- Tests must use an injected temporary path and never rewrite `config/gui.toml`.

### Scope boundaries

- Camera snapshots are not ordinary settings and should not be accidentally folded into the top-level config.
- Read-only diagnostics should not become fake persisted values.
- Panel open/closed state, scroll position, and selected collapsible sections are UI state; persist them only if deliberately added as user preferences.
- Generated/runtime defaults, developer tuning defaults, and per-user overrides are distinct concepts even if they currently share one file.

## Adding Settings After the Redesign

### Simple scalar setting

The intended workflow should be:

1. Add one declaration to the immutable generic schema.
2. Regenerate through `cargo check`.
3. Consume the generated strongly typed field in runtime code.
4. Add behavior-specific tests if needed.

Load, UI, dirty tracking, and Save should be automatic from the same declaration.

### Complex settings group

The intended workflow should be:

1. Define a serializable typed group with validation/defaults.
2. Add it as a field of `DebugSettingsValues`.
3. Implement its focused `draw` method.
4. Compose it statically inside `DebugSettings::draw`.
5. Add it to the full round-trip fixture.

There should be no new Save argument and no unrelated `App` persistence field.

## Recommended Decisions Before Implementation

Confirm these before Phase 3 or Phase 6:

1. Is the Debug Panel strictly a developer tuning tool, or will its Save button become a player settings feature?
2. Should Save update repository defaults, per-user overrides, or both through separate actions?
3. For time of day, should Save capture current time or preserve a distinct initial/manual time?
4. Should unknown config fields fail fast or be preserved across versions?
5. Is one release/version of backward-compatible combined `gui.toml` loading sufficient before removing old flattening?

Recommended defaults:

- Treat the current panel as developer tuning until a player-facing settings UI is explicitly designed.
- Keep current format compatibility during ownership/UI refactors.
- Use a distinct persisted initial/manual time and read-only current time.
- Fail clearly on unknown developer-config fields; use explicit migrations for format changes.
- Split schema from writable values only after the aggregate owner is stable.

## Definition of Done Checklist

- [x] One `DebugSettings` owner exists in `App`.
- [x] All editable Debug Panel settings are fields under its desired values.
- [x] The top-level Save takes no per-setting arguments.
- [x] Tree and Render Leaves have one desired-state authority.
- [x] Wind vector length is the only source of truth for source count.
- [x] No hidden save denylist remains.
- [x] Runtime/live values are read-only or clearly separated from persisted settings.
- [x] CLI overrides do not mutate desired values.
- [x] Full settings round-trip tests cover non-default values.
- [x] Legacy config tests cover missing fields required by the current format.
- [x] Save is atomic and path-injectable for tests.
- [ ] Dirty state is visible in the panel. Save success and failure are already visible.
- [x] Generated load and save paths come from the same declarations.
- [x] Documentation explains how to add simple and complex settings.
- [x] Existing hidden release run remains error-free.

## Implementation Progress

- 2026-07-14: Added a persistence coverage test spanning all generic parameter kinds, typed Tree settings, Render Leaves, and dynamic wind sources.
- 2026-07-14: Introduced the aggregate `DebugSettings` owner and moved `App` consumers to that single settings boundary.
- 2026-07-14: Closed the editable UI boundary under `DebugSettings::draw`; Tree and Audio Ray Tracing no longer enter through unrelated `App` wiring.
- 2026-07-14: Reduced Save to `DebugSettings::save()` with no per-setting arguments, removed the hidden `time_of_day` denylist, and removed obsolete save entry points.
- 2026-07-14: Made Render Leaves a desired Tree setting with effective runtime state derived from CLI flora constraints.
- 2026-07-14: Split persisted initial/manual time of day from the live day/night clock.
- 2026-07-14: Split water CLI/profile overrides from persisted desired water settings so diagnostic startup flags cannot leak into Save.
- 2026-07-14: Added atomic temporary-file replacement, path-injected persistence tests, and visible Save status.
- 2026-07-14: Added generic fallback rendering for parameters not claimed by custom Flora/Wind layouts, so future schema additions cannot silently disappear from those sections.
- 2026-07-14: Validated with formatting, compile checks, 160 passing tests (1 ignored), default and performance-profile hidden release runs, and checks that runtime validation did not alter `config/gui.toml`.

Deferred decisions:

- Phase 6's schema/defaults/user-values file split remains intentionally deferred until the project decides whether this panel writes developer defaults or per-user settings.
- Dirty-state UI remains optional follow-up work; Save success and failure are already reported in the panel.
