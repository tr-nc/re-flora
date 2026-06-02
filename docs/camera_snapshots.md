# Camera Snapshots

Camera snapshots are named, reproducible viewpoints for visual checks and hidden screenshots.

## In-game workflow

Open the existing debug/config panel, use the **Camera Snapshots** section, enter a name and optional description, then save the current camera. Names are normalized to lowercase kebab-case, for example `Tree Closeup!!` becomes `tree-closeup`. Duplicate names get suffixes such as `tree-closeup-2`.

Snapshots are stored in:

```text
config/camera_snapshots.toml
```

## File format

```toml
[[snapshots]]
name = "tree-closeup"
description = "close view for leaves, trunk, and shadows"
position = [2.1, 0.45, 1.8]
yaw_deg = 42.0
pitch_deg = -8.0
fov_deg = 60.0
fly_mode = true
```

## Hidden screenshot usage

List available snapshots:

```bash
cargo run --release -- --list-camera-snapshots
```

Use the default player camera:

```bash
cargo run --release -- --hidden --screenshot screenshots/default.png --auto-exit 4
```

Use a saved snapshot:

```bash
cargo run --release -- --hidden --camera-snapshot tree-closeup --screenshot screenshots/tree-closeup.png --auto-exit 4
```

Screenshots use a fixed 2-second render warmup before capture.
