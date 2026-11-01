# Startup optimization acceptance evidence

Open [the HTML acceptance report](index.html). This is point-in-time native Release evidence, not a parallel feature manual.

Runtime under acceptance: `70179a06`; baseline: `4ce0dbcc`. Both were measured on the same i5-12600KF / RTX 3060 Ti 8 GiB / driver 580.159.04, default 4×2×4 world. Normal hidden benchmark extent is 2880×1620; screenshots use 1600×1000 and are **not** benchmark runs. No present-mode override.

## Evidence

- `raw/*.log.gz`: 34 complete captured native stdout/stderr, build/check and test logs. Local machine paths are intentional. Final normal runs are `accepted-warm-{1,2,3}` and `accepted-cold`; don't include saving, screenshots, read-only replay or verbose collider debugging in their median.
- `measurements.json`: reconstructed timing/cache/publication data. The no-probe baseline is the old report's `clean-2` (42.5197 s), not its instrumented median (42.803 s). Final median is 9.5334 s.
- `snapshots.json`: captured comparison of local old/new save fixtures. All 32 terrain payloads were compared byte for byte (512 MiB), as were sorted grass/flower root identities and complete saved tree records. Mutable flora spawn ages and the fruit-cycle clock are not asserted identical. The large saves remain local and are not in this bundle.
- `check.mjs`: dependency-free parser/assertions. Run `node docs/reports/courtyard-startup/optimization/check.mjs`. It checks logs and the recorded snapshot result. To reperform the large comparison, pass `--snapshots <old.rflterrain> <new.rflterrain>`; without those files, the parser does **not** independently re-prove the byte comparison.
- `images/`: actual native Release screenshots and their self-contained provenance.
- `SHA256SUMS`: checksums of this bundle.

The step logs are sequential implementation checkpoints, not isolated component A/B measurements. The small generation-sharing improvement is not independently distinguishable from whole-run noise. Cache tightening has a real native control: sequential publication reuses 0/1/2/3/4 fragments, versus the old 0/1/1/3/4. Final batch publication needs only one forest preparation, so don't attribute its already-removed cache miss again.

The initially widened grass-validation box removed 69 extra roots. `batch-before-sparse-fix` and `sequential-save-control` record the discrepancy; `batch-after-sparse-fix` plus the exact snapshot comparison record its resolution. No unexplained visual/state discrepancy was accepted.

## Reproduce normal runs

```sh
cargo fmt --check
cargo check
cargo test
cargo test -p re-flora-terrain-collider
cargo test -p re-flora-physics
cargo build --release
RUST_LOG=info cargo run --release -- --hidden --mute --auto-exit 0.5
# Repeat unchanged; accept warm samples only when PIPELINE_CACHE says hit.
# Also repeat with --garden-seed 7.
cargo run --release -- --tail-latest-log 100
```

For the accepted cold run, only the current application's pipeline-cache file was temporarily renamed, then restored after the run (including on failure). No driver disk cache was cleared. Cold startup was 35.3286 s, compute-pipeline preparation 24.9859 s. Warm figures must not be presented as first-run figures.

Save/load and native growth checks:

```sh
target/release/re-flora --hidden --mute --terrain-save target/new.rflterrain --auto-exit 0.5
target/release/re-flora --hidden --mute --terrain-load target/new.rflterrain --auto-exit 0.5
# Also load the pre-optimization save; do not regenerate it with the new code.
RE_FLORA_SAPLING_REVIEW=blocked target/release/re-flora --hidden --mute --auto-exit 1.5
RE_FLORA_SAPLING_REVIEW=cycle target/release/re-flora --hidden --mute --auto-exit 1.5
RE_FLORA_SAPLING_REVIEW=edit target/release/re-flora --hidden --mute --auto-exit 1.5
RE_FLORA_SAPLING_REVIEW=override target/release/re-flora --hidden --mute --auto-exit 1.5
RE_FLORA_SAPLING_REVIEW=0.5 target/release/re-flora --hidden --mute --terrain-save target/half.rflterrain --auto-exit 0.5
RE_FLORA_SAPLING_REVIEW=load target/release/re-flora --hidden --mute --terrain-load target/half.rflterrain --auto-exit 0.5
```

## Temporary controls

Use an isolated checkout, never a working tree with unrelated edits. Patches are evidence-only and removed from delivered runtime code. Run `cargo check` and rebuild after applying or removing either patch.

- `sequential-control.patch` switches just the fresh courtyard back to one-at-a-time planting. Recorded control is based on `bf0b172c`; the patch also applies to `70179a06`. Run until the pipeline cache hits and inspect `[TREE][COMPILE_CACHE]`. This is not a shipped CLI/UI mode.
- `collider-replay.patch` adds an opt-in read-only comparison against the original center sampler. It applies to `04f5c1e8` or `70179a06`. With `RE_FLORA_COLLIDER_EXPORT_REPLAY=1`, run the default seed and seed 7. Each compared 8,192 sampled blocks / 268,435,456 voxel bytes, with zero disagreements. Other all-absent chunks return earlier through the unchanged zero-source path and are not counted. Replay logs include substantial **extra** work and must never be used as optimized startup timings.

## Limits

No final fine-grained whole-startup partition was collected; the older diagnostic partition is not claimed to describe the optimized runtime. Wall times include scheduling and synchronization; no pure GPU speedup or steady-state FPS acceptance is claimed. The surface fragment cache still reads the broad atlas region and conservatively retains cone-AABB + radius-two dependencies; this change does not eliminate every readback or every conservative miss. Other hardware and long-running interactive workloads remain outside these measurements.
