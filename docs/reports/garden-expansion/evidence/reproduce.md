# Reproduction and player controls

Run game commands from the **re-flora project worktree**, not this report directory. Validation used Linux/X11 and NVIDIA RTX 3060 Ti. `env -u WAYLAND_DISPLAY` avoids this host's Wayland settings-portal delay; it is not a portable Windows shell command. These runs exercise production APIs through fixtures, not physical mouse input. No three-platform packages were built.

## Baseline checks

```sh
cargo fmt --check
cargo check
cargo test # recorded: main 1759 passed / 9 baseline failures / 14 ignored, exit 101
cargo test --test clipboard_initialization --test display_authorization
# recorded integration targets: 1 passed, 4 ignored
cargo build --release
env -u WAYLAND_DISPLAY RUST_LOG=info cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

## Grass and recorded sound

```sh
env -u WAYLAND_DISPLAY RE_FLORA_GRASS_SOIL_COVER_REVIEW=1 \
  RE_FLORA_GRASS_SOIL_COVER_BENCH=1 RUST_LOG=info \
  cargo run --release -- --hidden --mute --auto-exit 1
python3 scripts/prepare_tree_leaf_loops.py --check
env -u WAYLAND_DISPLAY RE_FLORA_TREE_LEAF_AUDIO_REVIEW=1 RUST_LOG=info \
  cargo run --release -- --canopy-audio-budget-diagnostic --hidden --mute --auto-exit 14
```

The grass benchmark times coverage refresh **after** root publication, not the complete terrain edit. The audio review checks six mixing/lifecycle stages; mute prevents listening approval.

## Visual fixtures

Common capture form (substitute the environment assignment and output):

```sh
env -u WAYLAND_DISPLAY RE_FLORA_GARDEN_PLANT_REVIEW=sunflower RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --screenshot garden-arrival target/garden-sunflower.png --screenshot-delay 2 --auto-exit 3
```

| Visual subject | Fixture assignment / special handling |
|---|---|
| Moon | `RE_FLORA_NIGHT_REVIEW=moon` |
| Blue flower day | `RE_FLORA_GARDEN_PLANT_REVIEW=forget-me-not` |
| Blue flower night | Add `RE_FLORA_NIGHT_REVIEW=flowers` |
| Fireflies | `RE_FLORA_NIGHT_REVIEW=fireflies`; `fireflies-cycle` switches to day after four seconds (run at least ten seconds for retirement) |
| Butterfly fixed poses | `RE_FLORA_MODEL_VIEW_REVIEW=32`; reproducing the old brightness needs the pre-197c4df0 implementation, not the current shader |
| Lantern | `RE_FLORA_LANTERN_REVIEW=1`, optionally `--terrain-save target/garden-lantern.rflterrain`; `load` checks the actual saved world; night load adds `RE_FLORA_NIGHT_REVIEW=lantern` |
| Fence | `RE_FLORA_FENCE_REVIEW=1|load|edit` |
| Plow | `RE_FLORA_SOIL_MOVE_REVIEW=1|load` |
| Shovel | `RE_FLORA_SHOVEL_REVIEW=hold|load-drop|drop|partial|moisture`; hold can save, load-drop must load that actual saved world |
| Saplings | `RE_FLORA_SAPLING_REVIEW=0.375|0.5|0.75|1`; also `load|cycle|edit|blocked|override` |
| Economy | `RE_FLORA_ECONOMY_REVIEW=before|trade|fallen|shop|load`; shop screenshot uses `player-default`; fallen run at least four seconds |

A `|` in the table means choose **one** mode, not a literal shell argument or pipe. These are isolated fixtures; combining all review hooks at once is not a supported dense-garden benchmark. Snapshot files are reproducible outputs in the worktree, not report-reading dependencies, and are not bundled here.

## Actual persistence example

```sh
env -u WAYLAND_DISPLAY RE_FLORA_ECONOMY_REVIEW=trade RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --terrain-save target/garden-economy.rflterrain --auto-exit 3
env -u WAYLAND_DISPLAY RE_FLORA_ECONOMY_REVIEW=load RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --terrain-load target/garden-economy.rflterrain --auto-exit 1
```

Other old-save checks substitute the corresponding hook and filename: `garden-fence`, `garden-plow`, `garden-shovel-held`, `garden-sapling-0.5`, `garden-lantern`. Final-runtime regression loaded genuinely earlier saves, not just serialized/deserialized an object in one process.

## Bees

```sh
env -u WAYLAND_DISPLAY RE_FLORA_BEE_REVIEW=garden RUST_LOG=info \
  cargo run --release -- --hidden --mute --terrain-save target/garden-bees.rflterrain \
  --screenshot garden-arrival target/garden-bees.png --screenshot-delay 5 --auto-exit 16
env -u WAYLAND_DISPLAY RE_FLORA_BEE_REVIEW=load RUST_LOG=info \
  cargo run --release -- --hidden --mute --terrain-load target/garden-bees.rflterrain --auto-exit 16
```

Other accepted modes: `edit`, `cap`, `tree-delete`, `bury`, `cycle`, `flowers-removed`, `particles-off`, `native`, `without`, `tree-view`. Use one mode and omit load/save unless intended. `tree-view` deliberately hides leaves/flowers/fruit for the attachment diagnostic; it is not normal art or an active foraging scene. `native` disables the existing insect/house cache setting for that run only.

## Performance sampling

Use **`RE_FLORA_GARDEN_PERF=1`** with `--perf` and read `[PERF] frame … total …ms`, not only conditional `[PERF][FRAME]` hitch records. The latter logs every 30th frame or when total >=16 ms / queue work >=2 ms; its sample distribution is biased.

- Flowers: use both blue-flower night assignments above, `RE_FLORA_GARDEN_PERF=1`, `--hidden --mute --perf --auto-exit 8`; the off run additionally sets `RE_FLORA_NIGHT_FLOWER_LIGHTS_OFF=1`, the on run must **unset** it. Filter completed timing records to frame >120. Final off/on: 471/463 samples, median 17.10/17.10 ms, p95 17.66/17.88 ms; maxima 23.81/56.50 ms include early pending-world-work frames.
- Bees: `RE_FLORA_BEE_REVIEW=without` versus `garden`, `RE_FLORA_GARDEN_PERF=1`, `--hidden --mute --perf --auto-exit 16`; filter frame >300. 761/760 samples, median 16.96/17.02 ms, p95 17.98/17.94 ms.

The local `performance.json` includes every selected sample, source hashes and the nearest-rank p95 definition. Present-inclusive one-pair results are not isolated costs or statistical proof. Hidden `--perf` checks do not authorize visible timing panels during ordinary player try-outs.

## Player try-out map (not automatically launched)

```sh
cargo run --release
```

- Grow: choose seeds/plants/trees; finite mode spends owned stock after success.
- Lantern; Thin pole / Thick pole; Rope: connect arbitrary successive posts, click the current endpoint again to end the chain.
- Plow: drag a shallow furrow. Shovel: press once, hold/carry, release elsewhere; failed/canceled drops keep the remainder.
- Harvest or E: one aimed ripe-apple collection. B: shop; B/Esc: close shop without exiting.
- Bee house: mount on exposed tree/pole side; right-click the house to remove. Nearby flowers enable trips.
- Keep unlimited-material debugging if desired; it does not make soil movement infinite and does not make shop purchases free.

Authoritative source documents remain in the repository: `SPEC.md`, `docs/grass-soil-cover.md`, `docs/garden-{night-flower,stone-lantern,fences,soil-movement,saplings,economy,bees}.md`, and `docs/reports/recorded-tree-leaf-audio.md`. The report body and evidence here carry the outcomes/limits without requiring those files to be present.
