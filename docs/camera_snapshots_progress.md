# Camera Snapshot Presets Progress

## Goal

Add a camera snapshot preset system for reproducible visual checks.

Done means:

- Players can use the existing in-game menu UI to save the current camera pose as a named snapshot.
- Snapshot names are unique and normalized to `foo-bar` style.
- Snapshots persist in one local, readable text file.
- Hidden runs can load stored snapshots and optionally select one by name for screenshots.
- If no snapshot is selected, the game keeps the current default player camera behavior.
- Screenshot capture uses a fixed internal delay suitable for temporal accumulation, without requiring the model/user to specify a delay.

## Current State

Known systems and files:

- Worktree: `/home/terence/code/re-flora-agent-camera-snapshots`
- Branch: `agent/camera-snapshots`, based on `main`.
- Camera input/state is centered around:
  - `src/gameplay/camera/controller.rs`
  - `src/gameplay/camera/movement.rs`
  - `src/tracer/mod.rs`
- App lifecycle, hidden mode, screenshot, and CLI options are centered around:
  - `src/app/core/mod.rs`
  - `src/app/core/boot.rs`
  - `src/app/core/screenshot.rs`
  - `src/cli.rs`
- In-game config/menu UI already exists in `src/app/core/mod.rs` and related UI modules.
- Existing hidden/screenshot commands:
  - `cargo run --release -- --hidden --screenshot screenshots/check.png --screenshot-delay 5 --auto-exit 7`
  - `cargo run --release -- --latest-log`
  - `cargo run --release -- --tail-latest-log 200`

Constraints and decisions so far:

- Do not add new keyboard shortcuts.
- Snapshot management should live in the existing menu UI.
- Store snapshots in a single readable text file, likely TOML under `config/`.
- Store camera pose in a readable form, not only raw matrices.
- Snapshot names should be normalized to lowercase kebab-case (`foo-bar`) and made unique automatically.
- Hidden mode should load all snapshots at startup.
- A virtual/default camera option should preserve current player default behavior.
- Screenshot delay should be fixed internally, currently proposed as 2 seconds.

Assumptions to confirm:

- TOML is acceptable for the snapshot file, e.g. `config/camera_snapshots.toml`.
- A snapshot pose should include at least position, yaw, pitch, and possibly FOV/fly-mode.
- Applying a snapshot should reset temporal/render history where needed.
- A missing requested snapshot should fail with a clear error rather than silently falling back.

## Plan / Phases

### Phase 1: Snapshot data model and persistence

- Objective: Define the snapshot schema and load/save manager.
- Expected output:
  - Camera snapshot structs.
  - TOML load/save support.
  - Name normalization and uniqueness helpers.
  - Unit tests for parsing/naming behavior.
- Dependencies/blockers:
  - Confirm final file path and fields.
- Status: not started

### Phase 2: Camera pose access and application

- Objective: Expose safe methods to read current camera pose and apply a stored pose.
- Expected output:
  - Camera/tracer methods for get/apply snapshot pose.
  - Temporal history reset hook when applying a pose.
- Dependencies/blockers:
  - Need to inspect exact camera previous-frame/history interactions.
- Status: not started

### Phase 3: In-game menu UI

- Objective: Add snapshot controls to the existing menu/config UI without new hotkeys.
- Expected output:
  - UI area for current pose, snapshot name input, save button, and list/apply controls.
  - Automatic normalized unique names.
  - Persistence to the snapshot file.
- Dependencies/blockers:
  - Phase 1 and Phase 2.
- Status: not started

### Phase 4: CLI and hidden-mode integration

- Objective: Let hidden/screenshot automation select a snapshot by name.
- Expected output:
  - CLI option such as `--camera-snapshot <name>`.
  - Optional listing/help behavior if useful.
  - Startup loads snapshot file and applies selected snapshot before screenshot timing begins.
  - No selected snapshot preserves current default camera behavior.
- Dependencies/blockers:
  - Phase 1 and Phase 2.
- Status: not started

### Phase 5: Screenshot delay simplification

- Objective: Make screenshot timing model-safe and temporal-friendly.
- Expected output:
  - `--screenshot` captures after a fixed internal delay, proposed 2 seconds.
  - Existing `--screenshot-delay` behavior is either removed from help or retained only as a compatibility/internal override, pending decision.
  - Hidden screenshot examples no longer require model/user-provided delay.
- Dependencies/blockers:
  - Confirm compatibility expectations for existing scripts.
- Status: not started

### Phase 6: Documentation and examples

- Objective: Document the workflow for players, agents, and manual validation.
- Expected output:
  - Short usage notes in an appropriate docs file or README section.
  - Example TOML and CLI commands.
- Dependencies/blockers:
  - Implementation details finalized.
- Status: not started

## Verification Method

Planned checks:

- Formatting and compile checks:
  - `cargo fmt --check`
  - `cargo check`
- Unit tests:
  - Name normalization: spaces/punctuation/case become `foo-bar`.
  - Duplicate names get deterministic suffixes.
  - Snapshot TOML round-trip preserves pose fields.
  - Missing/invalid snapshot file is handled safely.
- Hidden screenshot validation:
  - Create or commit a small sample snapshot file.
  - Run: `cargo run --release -- --hidden --camera-snapshot <name> --screenshot screenshots/<name>.png --auto-exit 3`
  - Confirm the screenshot is created and the run log reports the selected snapshot.
  - Run: `cargo run --release -- --tail-latest-log 200` and check for errors.
- UI/manual validation:
  - Open the game normally.
  - Save multiple snapshots from the menu.
  - Confirm names are normalized and unique.
  - Restart and confirm snapshots are reloaded.
  - Apply each snapshot and confirm the camera moves to the stored pose.
- Regression validation:
  - Hidden run without any snapshot option still uses the current default camera.
  - Hidden run with no snapshot file does not fail unless a specific missing snapshot was requested.

Verification is not yet possible because implementation has not started.

## Progress Log

- 2026-06-03: Discussed and chose camera snapshots as the higher-ROI path over free-form LLM gameplay. Reason: reproducible named viewpoints are more reliable for visual QA and LLM image review.
- 2026-06-03: Decided not to add new shortcuts. Snapshot management should be integrated into the existing menu UI.
- 2026-06-03: Decided snapshot names should follow normalized kebab-case (`foo-bar`) and be made unique automatically.
- 2026-06-03: Decided screenshots should use a fixed internal temporal-warmup delay, proposed as 2 seconds, instead of exposing delay selection to the model.
- 2026-06-03: Created isolated worktree `/home/terence/code/re-flora-agent-camera-snapshots` on branch `agent/camera-snapshots` from `main`.
- 2026-06-03: Created this progress document. No implementation changes made.

## Open Questions / Risks

- Should the snapshot file path be exactly `config/camera_snapshots.toml`?
- Which fields are required in the snapshot schema: position/yaw/pitch only, or also FOV, fly mode, time of day, render flags, or config overrides?
- Should `--screenshot-delay` be removed, hidden, ignored, or kept for backwards compatibility?
- Should requesting a missing snapshot fail hard, or fall back to default only in non-agent/manual usage?
- Applying a snapshot during runtime may need denoiser, shadow, and previous-frame camera history resets to avoid transient artifacts.
- UI persistence can race with manual edits if the file is changed while the game is running; decide whether this matters for MVP.
- Screenshot determinism may still be affected by particles, water simulation, temporal effects, GPU/driver differences, and time-of-day animation.
