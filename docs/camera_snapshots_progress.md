# Camera Snapshot Presets Progress

## Goal

Add a camera snapshot preset system for reproducible visual checks.

Done means:

- Players can use the existing in-game menu UI to save the current camera pose as a named snapshot.
- Snapshot names are unique and normalized to `foo-bar` style.
- Snapshots persist in one local, readable text file.
- Hidden runs can load stored snapshots and select one by name for screenshots.
- If no snapshot is selected outside screenshot mode, the game keeps the current default player camera behavior.
- Screenshot capture requires an explicit preset name and explicit delay value.

## Current State

Known systems and files:

- Worktree: `/home/terence/code/verdarium-agent-camera-snapshots`
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
- Camera snapshot implementation files:
  - `src/app/camera_snapshots.rs`
  - `src/app/core/camera_snapshot_ui.rs`
  - `docs/camera_snapshots.md`

Commands:

- `cargo run --release -- --list-camera-snapshots`
- `cargo run --release -- --hidden --screenshot player-default screenshots/check.png --screenshot-delay 2 --auto-exit 4`
- `cargo run --release -- --hidden --screenshot tree-closeup screenshots/tree-closeup.png --screenshot-delay 2 --auto-exit 4`
- `cargo run --release -- --latest-log`
- `cargo run --release -- --tail-latest-log 200`

Implemented decisions:

- No new keyboard shortcuts.
- Snapshot management lives in the existing debug/config menu UI.
- Snapshots are stored in readable TOML at `config/camera_snapshots.toml`.
- Stored pose fields are position, yaw, pitch, FOV, description, and fly mode.
- Snapshot names are normalized to lowercase kebab-case (`foo-bar`) and made unique automatically.
- Hidden mode loads snapshots at startup.
- `player-default` is always available as the virtual/default camera option.
- Missing requested startup/screenshot snapshots fail clearly, list available names, and mention `--list-camera-snapshots`.
- Screenshot mode uses `--screenshot <preset> <path>` and requires `--screenshot-delay <sec>`.

## Plan / Phases

### Phase 1: Snapshot data model and persistence

- Objective: Define the snapshot schema and load/save manager.
- Expected output:
  - Camera snapshot structs.
  - TOML load/save support.
  - Name normalization and uniqueness helpers.
  - Unit tests for parsing/naming behavior.
- Dependencies/blockers: none remaining.
- Status: done

### Phase 2: Camera pose access and application

- Objective: Expose safe methods to read current camera pose and apply a stored pose.
- Expected output:
  - Camera/tracer methods for get/apply snapshot pose.
  - Temporal history reset hook when applying a pose.
- Dependencies/blockers: none remaining.
- Status: done

### Phase 3: In-game menu UI

- Objective: Add snapshot controls to the existing menu/config UI without new hotkeys.
- Expected output:
  - UI area for current pose, snapshot name input, save button, list/apply controls, and delete controls.
  - Automatic normalized unique names.
  - Persistence to the snapshot file.
- Dependencies/blockers: none remaining.
- Status: done

### Phase 4: CLI and hidden-mode integration

- Objective: Let hidden/screenshot automation select a snapshot by name.
- Expected output:
  - `--screenshot <preset> <path>` for one-shot screenshot runs.
  - `--camera-snapshot <name>` for non-screenshot startup camera selection.
  - `--list-camera-snapshots`.
  - Startup loads the snapshot file and applies the selected snapshot before screenshot timing begins.
  - No selected snapshot outside screenshot mode preserves current default camera behavior.
- Dependencies/blockers: none remaining.
- Status: done

### Phase 5: Screenshot delay and syntax simplification

- Objective: Make screenshot timing explicit and syntax errors useful for agents.
- Expected output:
  - `--screenshot` requires exactly one preset and one output path.
  - `--screenshot-delay <sec>` is required whenever `--screenshot` is used.
  - Syntax and missing-preset errors mention `--list-camera-snapshots`.
- Dependencies/blockers: none remaining.
- Status: done

### Phase 6: Documentation and examples

- Objective: Document the workflow for players, agents, and manual validation.
- Expected output:
  - Short usage notes.
  - Example TOML and CLI commands.
- Dependencies/blockers: none remaining.
- Status: done

## Verification Method

Completed checks:

- `cargo fmt --check`
- `cargo check`
- `cargo test`
- `cargo run -- --list-camera-snapshots`
- `cargo run -- --help`
- Temporary snapshot file validation:
  - created `config/camera_snapshots.toml` with `test-overview`
  - ran `cargo run -- --list-camera-snapshots`
  - confirmed `player-default` and `test-overview` printed
  - removed the temporary snapshot file before final status
- Hidden snapshot screenshot validation:
  - `cargo run --release -- --hidden --screenshot test-overview target/camera-snapshot-check.png --screenshot-delay 2 --auto-exit 3`
  - confirmed log applied `test-overview`
  - confirmed screenshot saved after about 2 seconds
  - confirmed `target/camera-snapshot-check.png` exists and is a valid 1920x1200 PNG
- Hidden default-camera regression:
  - `cargo run --release -- --hidden --auto-exit 0.5`
  - confirmed startup with no snapshot file and no requested snapshot uses default player camera and exits successfully
- Revised screenshot CLI validation:
  - `cargo run -- --screenshot initial target/foo.png` reports missing `--screenshot-delay` and suggests `--list-camera-snapshots`
  - `cargo run -- --screenshot missing target/foo.png --screenshot-delay 1` reports the missing preset and suggests `--list-camera-snapshots`
  - `cargo run --release -- --hidden --screenshot initial target/camera-snapshot-initial-new-cli.png --screenshot-delay 2 --auto-exit 3`
  - confirmed screenshot saved as a valid 1920x1200 PNG

Manual validation still recommended:

- Open the game normally.
- Save multiple snapshots from the menu.
- Confirm names are normalized and unique.
- Restart and confirm snapshots are reloaded.
- Apply/delete snapshots from the UI.

## Progress Log

- 2026-06-03: Discussed and chose camera snapshots as the higher-ROI path over free-form LLM gameplay. Reason: reproducible named viewpoints are more reliable for visual QA and LLM image review.
- 2026-06-03: Decided not to add new shortcuts. Snapshot management should be integrated into the existing menu UI.
- 2026-06-03: Decided snapshot names should follow normalized kebab-case (`foo-bar`) and be made unique automatically.
- 2026-06-03: Initially decided screenshots should use a fixed internal temporal-warmup delay.
- 2026-06-03: Created isolated worktree `/home/terence/code/verdarium-agent-camera-snapshots` on branch `agent/camera-snapshots` from `main`.
- 2026-06-03: Created this progress document. No implementation changes made.
- 2026-06-03: Implemented snapshot persistence at `config/camera_snapshots.toml` with readable TOML, normalized unique kebab-case names, and unit tests.
- 2026-06-03: Added camera pose get/apply methods and reset camera movement/input/history when applying snapshots.
- 2026-06-03: Added Camera Snapshots controls to the existing debug/config menu without new shortcuts.
- 2026-06-03: Added `--camera-snapshot <name>` and `--list-camera-snapshots`; hidden runs keep the default camera when no snapshot is selected.
- 2026-06-03: Revised screenshot CLI so `--screenshot` takes one preset and one path, requires `--screenshot-delay <sec>`, and reports `--list-camera-snapshots` on syntax/name errors.
- 2026-06-03: Added `docs/camera_snapshots.md` and linked it from `README.md`.
- 2026-06-03: Validated with formatting, check, tests, snapshot listing, hidden snapshot screenshot, and hidden default-camera runs.
- 2026-06-03: Revalidated the revised screenshot CLI with help output, required delay error, missing preset error, and a hidden `initial` screenshot run.

## Open Questions / Risks

- UI persistence can race with manual edits if the file is changed while the game is running; this is acceptable for MVP.
- Screenshot determinism may still be affected by particles, water simulation, temporal effects, GPU/driver differences, and time-of-day animation.
- No automated UI click test exists; menu save/apply/delete behavior is covered by compile-time checks and manual validation remains recommended.
