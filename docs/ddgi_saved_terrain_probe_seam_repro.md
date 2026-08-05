# Saved-terrain DDGI probe-seam reproduction

Status: reproducible baseline, no renderer fix included

Repro tooling commit: `1fc89d75` (`repro: capture saved terrain probe seam with hand tool`)

This document records the user-authored terrain state and the exact release-mode capture path.
It is the reference procedure for later diagnosis or renderer changes. It is intentionally
separate from the historical procedural `patt-seam` fixture: do not replace this path with
`--environment-lighting-test-scene` or another generated terrain.

## Fixed inputs

Run from the repository root in this worktree. The input files used by the captured evidence are:

- terrain: `saves/terrain_snapshot.rflterrain`, 134,218,112 bytes,
  SHA-256 `c1994a9bb602a2d172545c85ae17ba5e72346aedb340f31575b60cf8170ece72`;
- camera: `config/camera_snapshots.toml`, containing exactly one snapshot named `snapshot`;
- camera pose: position `(0.64095235, 0.52771884, 1.0284802)`, yaw `81.902435°`, pitch
  `-9.50234°`, FOV `60°`, fly mode enabled;
- lighting: `time_of_day=0.455705`, `latitude=-0.24`, `season=0.25`,
  `sun_luminance=1.65`, and `god_ray_weight=0`;
- terrain ray origin offset: `0.0065` world units; and
- the default player tool is Hand (`selected_item_panel_slot=None`), so the normal capture has no
  blue terrain-edit radius.

The terrain snapshot is authoritative. Startup logs must report that the snapshot was validated
and that the procedural tuning-tree stamp was suppressed. The screenshot preset applies the only
saved camera snapshot; do not add a separate `--camera-snapshot` argument to a screenshot run.

## One-command capture

```bash
scripts/repro_saved_terrain_probe_seam.sh \
    target/ddgi-seam-repro/user-state/captured
```

The script defaults to a ten-second post-startup screenshot delay and an eighteen-second
auto-exit. They can be overridden for local experiments without changing the fixture:

```bash
SAVED_TERRAIN_SCREENSHOT_DELAY=10 \
SAVED_TERRAIN_AUTO_EXIT=18 \
scripts/repro_saved_terrain_probe_seam.sh target/ddgi-seam-repro/user-state/captured
```

The runner performs three independent release-hidden runs, each with
`--mute --no-god-rays --terrain-load saves/terrain_snapshot.rflterrain` and
`--screenshot snapshot`. It then center-crops each 2880x1620 source image with ImageMagick using
`-crop 55%x68%+0+0`, producing 1584x1102 crops. The terrain is not procedurally rebuilt or
modified by the runner.

Use `--dry-run` to inspect the exact commands without starting Vulkan:

```bash
scripts/repro_saved_terrain_probe_seam.sh --dry-run target/ddgi-seam-repro/dry-run
```

## Screenshot reference

The first validated capture set is under
`target/ddgi-seam-repro/user-state/captured/`. These files are generated artifacts under
`target/`, not tracked source assets; rerun the command above if they are absent.

| Capture | Command mode | What it shows | How to interpret it |
| --- | --- | --- | --- |
| `normal.png` / `normal-crop.png` | Default `final` mode | Final terrain shading: DDGI environment irradiance multiplied by voxel albedo, plus direct sun. | This is the player-visible result. The seam can be subtle because material color, direct light, tone mapping, and the dark enclosure are all present. |
| `exact-irradiance.png` / `exact-irradiance-crop.png` | `--ddgi-debug-view exact-irradiance` plus `--environment-probe-visualization` | The exact DDGI terrain-reference irradiance is displayed directly. Green diamonds are valid probe markers drawn as a depth-tested overlay. | This is the clearest diagnostic view of the spatial bands. It is not final material color and is not path-traced ground truth; it still comes from the DDGI probe field and its exact per-probe visibility query. |
| `dominant-probe.png` / `dominant-probe-crop.png` | `--ddgi-debug-view dominant-probe` | Each pixel is colored from the index of the probe with the largest contribution. | The large flat color regions map probe ownership/cell transitions. Their hard color boundaries are expected for this diagnostic and are not, by themselves, a brightness or lighting verdict. |

The diagnosis branch also provides three read-only comparison views. They use the same saved
terrain and camera; only the query operation used for the displayed debug value changes:

| Capture | What it removes | Interpretation |
| --- | --- | --- |
| `unoccluded-irradiance` | moment and exact consumer visibility only | If this still bands, visibility attenuation is not required. |
| `equal-weight-irradiance` | position and surface-side weights, retaining trustworthy/support gates | A GREEN result here points at the spatial weighting/gating interaction. |
| `raw-cage-irradiance` | all query weights, support, and visibility; averages the eight valid nominal-cell atlas tiles | A GREEN result here means the raw local atlas values are not carrying the internal wall bands. |

For a matched hidden capture, replace the debug view in the normal command with one of:

```bash
cargo run --release -- --hidden --mute --no-god-rays \
    --terrain-load saves/terrain_snapshot.rflterrain \
    --ddgi-debug-view unoccluded-irradiance \
    --environment-probe-visualization \
    --screenshot snapshot target/ddgi-seam-repro/experiments/unoccluded32/final.png \
    --screenshot-delay 7 --auto-exit 10
```

Then center-crop with the same `magick ... -crop 55%x68%+0+0` command and run
`python3 scripts/analyze_saved_ddgi_seam.py <crop>`. The edge-excluded metric is RED for
`exact-irradiance` and `unoccluded-irradiance`, but GREEN for the equal-weight and raw-cage
diagnostics; see [`docs/references/ddgi/ddgi_probe_seam_research.md`](references/ddgi/ddgi_probe_seam_research.md)
for the retained logs and interpretation.

The exact-irradiance capture is the best first image for checking the reported seam because it
removes albedo and direct-sun composition from the displayed value while retaining the spatial
DDGI query. The probe markers make it possible to compare a wall transition with nearby probe
locations. Always compare it with the normal capture before calling a change a user-visible fix.

## Expected log evidence

Each of the three runs must contain all of the following, with no `ERROR` or panic lines:

```text
[TERRAIN_PERSISTENCE] startup load validated path=saves/terrain_snapshot.rflterrain chunks=8 bytes=134217728
[CAMERA_SNAPSHOT] Applied startup snapshot 'snapshot' from config/camera_snapshots.toml
[TERRAIN_PERSISTENCE] startup snapshot loaded; procedural tuning-tree stamp suppressed
[DDGI] transport converged ... ready=true
[SCREENSHOT] Saved 2880x1620 ...
Application exited successfully
```

Example logs from two successful three-capture runs are:

- `target/re-flora-logs/re-flora-20260806-003804.588-266415.log` (normal),
  `re-flora-20260806-003825.636-266747.log` (exact irradiance), and
  `re-flora-20260806-003846.639-266856.log` (dominant probe);
- `target/re-flora-logs/re-flora-20260806-004802.122-272033.log`,
  `re-flora-20260806-004823.190-272125.log`, and
  `re-flora-20260806-004844.183-272214.log` for the repeat run.

The hidden visible-snapshot startup equivalent is:

```bash
cargo run --release -- --hidden --mute --no-god-rays \
    --terrain-load saves/terrain_snapshot.rflterrain \
    --camera-snapshot snapshot \
    --auto-exit 0.5
```

For a later manual check, omit `--hidden --mute --auto-exit 0.5` and run the same command with
plain `cargo run`. The current reproduction work did not launch a visible window.

## Validation recorded for the repro tooling

The repro commit was validated with:

- `cargo fmt --check`;
- `cargo check`;
- `cargo test` (410 passed, 1 ignored);
- `python3 -m unittest scripts.tests.test_saved_terrain_probe_repro -v` (2 passed);
- `cargo run --release -- --hidden --mute --auto-exit 0.5`; and
- two complete executions of the three-capture runner above.

This document records the symptom and its capture surfaces only. It does not claim a root cause
and does not authorize changing DDGI transport, probe data, terrain materials, or renderer/shader
code as part of repro maintenance.
