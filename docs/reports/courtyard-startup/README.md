# Courtyard startup investigation

Follow-up: [implemented optimization and Release acceptance evidence](optimization/index.html) (runtime `70179a06`). The original investigation below remains a historical baseline.

Point-in-time Release evidence for commit `4ce0dbcc470de2c10681b8457927ed0b22c92d1a`, measured locally on 2026-11-02. Open [the HTML report](index.html) for findings. This is research evidence, not implemented optimization or a continuing feature manual.

## Bundle

- `index.html`: visual report, ranked non-overlapping wall-clock partition, repeated-work analysis, read-only replay results and limits. No external fonts, scripts, images or assets.
- `raw/*.log.gz`: complete captured stdout/stderr from Release runs, including `/usr/bin/time -v` statistics. Logs contain machine-local paths. `profile-1` used an earlier, less detailed probe; `profile-2` onward and both replays use the bundled patch.
- `instrumentation.patch`: temporary Rust probes and read-only replay. These changes were removed from the delivered application sources. No occupancy decision is replaced by the replay, and no terrain is modified by it. The normal algorithm still runs first.
- `analyze.mjs`: dependency-free Node script that parses the compressed logs, checks parent/child scope accounting, stage totals, cache states and actual admission/publication counts, then reconstructs `measurements.json`.
- `measurements.json`: parsed results and nested/exclusive timing summaries. The ranked partition uses the two cache-hit runs `profile-3` and `profile-4`. Cache-miss and replay runs are not mixed into this median.
- `SHA256SUMS`: checksums of the report evidence, commands, patch, analyzer and generated data.

Run `node analyze.mjs` from any directory to regenerate and validate the evidence. It uses only Node standard-library modules.

## Reproduce

Use an isolated checkout at the base commit with the same `config/gui.toml`; copy this evidence folder into the same relative path there before running the commands below. Do not apply diagnostic changes on top of unrelated uncommitted work. Hardware, driver, world size and the GUI settings affect timings. This study used an i5-12600KF, RTX 3060 Ti, driver 580.159.04, default 4×2×4 world and 2880×1620 physical render extent. No present mode override.

Baseline (no Rust patch):

```sh
cargo build --release
env RUST_LOG=info RE_FLORA_TREE_EDIT_DIAGNOSTIC=1 \
  /usr/bin/time -v target/release/re-flora --hidden --mute --auto-exit 0.5
```

Apply the bundled diagnostic patch only for measurement:

```sh
git apply --check docs/reports/courtyard-startup/instrumentation.patch
git apply docs/reports/courtyard-startup/instrumentation.patch
cargo fmt --check
cargo check
cargo build --release
env RUST_LOG=info RE_FLORA_TREE_EDIT_DIAGNOSTIC=1 RE_FLORA_STARTUP_PROBE=1 \
  /usr/bin/time -v target/release/re-flora --hidden --mute --auto-exit 0.5
```

The first run after changing the executable can miss the Vulkan pipeline cache (~70 seconds in this study). Repeat unchanged until the log says `[PIPELINE_CACHE] status=hit`; collect at least two hit runs. `--auto-exit 0.5` is a post-loading run duration, not a limit on initialization, and the reported first-frame time excludes its wait.

Read-only replay, without replacing real admission:

```sh
env RUST_LOG=info RE_FLORA_TREE_EDIT_DIAGNOSTIC=1 RE_FLORA_STARTUP_PROBE=1 \
  RE_FLORA_STARTUP_REPLAY=1 /usr/bin/time -v target/release/re-flora \
  --hidden --mute --auto-exit 0.5
# Repeat with --garden-seed 7 for the second geometry/material distribution.
```

The replay visits the same candidate points with the existing exact cone BVH, asserts the same zero intersections as the successfully completed real admission, and compares additional branch-center/end/boundary fixtures against the naive query. It also inspects material changes causing a per-tree surface-cache miss. Replay timing is **additional** work and must not be included in the normal-startup median or represented as a shipped optimization.

Load comparison fixture (the ~512 MiB saved world is intentionally not bundled):

```sh
target/release/re-flora --hidden --mute --auto-exit 0.5 \
  --terrain-save target/courtyard-startup.rflterrain
env RUST_LOG=info RE_FLORA_TREE_EDIT_DIAGNOSTIC=1 RE_FLORA_STARTUP_PROBE=1 \
  /usr/bin/time -v target/release/re-flora --hidden --mute --auto-exit 0.5 \
  --terrain-load target/courtyard-startup.rflterrain
```

After collecting evidence, reverse only the task-owned diagnostic patch, run `cargo fmt --check`, `cargo check`, `cargo test`, `cargo build --release`, and a hidden muted run; inspect the native run log. `clean-1` and `clean-2` record the actual post-cleanup validation, with pipeline miss/hit separated.

## Limits

Wall-clock scopes include blocking and scheduling; transfer/upload calls are not GPU timestamps. The ~0.35-second initial-phase residual and ~0.62-second population residual are not attributed more finely. Whole-process CPU statistics include worker threads and shutdown, not just the nested startup interval. No `perf` executable was available. The successful-admission SDF count is derived exactly from candidate count × cone count, avoiding a counter inside 1.5 billion evaluations. No game steady-state FPS or other hardware was benchmarked. Boundary fixtures and two complete clear-world replays are evidence for the mechanism, not exhaustive validation of a future production fix.
