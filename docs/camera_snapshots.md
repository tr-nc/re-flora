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

Use the virtual default player camera preset:

```bash
cargo run --release -- --hidden --screenshot player-default screenshots/default.png --screenshot-delay 2 --auto-exit 4
```

Use a saved snapshot:

```bash
cargo run --release -- --hidden --screenshot tree-closeup screenshots/tree-closeup.png --screenshot-delay 2 --auto-exit 4
```

Screenshot runs must name exactly one preset and provide an explicit `--screenshot-delay <sec>`. If syntax is wrong or the preset does not exist, the CLI tells you to run `re-flora --list-camera-snapshots`.
