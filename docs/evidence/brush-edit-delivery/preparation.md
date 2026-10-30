# Grass transaction preparation — 2026-10-30 evidence

Historical experiment, not a cross-platform acceptance or maintained implementation inventory.
Baseline: `0350bfe4` (saved runtime switch). Candidate: the single-upload snapshot constructor
committed with this evidence. Binary/source/log hashes and unfiltered events are in
[`preparation-data.json`](preparation-data.json).

## Question and measured cause

Can recurring transaction preparation be reduced without weakening input isolation?
On Apple M4 Pro / macOS / MoltenVK / FIFO, temporary Release probes measured the dense-point
case's per-chunk buffer allocation median at 0.020 ms, versus 0.072 ms for default CPU potential
initialization, 0.216 ms for its upload, and 0.454 ms for the subsequent actual-potential clone /
overwrite. These individual medians are not additive. This supports removing duplicate field
initialization before introducing a resource pool. It does not establish the RTX cost split.

The resource owner can initialize a private transaction directly from its actual CPU-authored
potential. For each affected 256³ chunk, this removes one 8 MiB default CPU initialization and
one 8 MiB mapped GPU-buffer upload. The actual snapshot copy/upload, allocations, instance
snapshots, hydration work and atomic publication remain. No mutable live buffer is shared.

The temporary [`profile patch`](preparation-profile.patch) applies to `0350bfe4`; it was removed
before candidate builds. Enable its log target with `RUST_LOG=info,grass_edit_preparation=debug`.
Profile logs include constructor calls outside transactions; archived summaries select only
measured edits (after sample 3 begins). Probe timings are diagnostic, not the A/B result.

## Release A/B

All GPU runs were sequential, hidden and muted, using the same configuration and assets.
No settings were saved. The normal runner excluded 3 warmups and measured 12 events per run.
All seven asynchronous grass cases had identical output signatures and lower CPU-phase medians.

| Case | Before total / ms | After total / ms | Before submit / ms | After submit / ms |
| --- | ---: | ---: | ---: | ---: |
| Empty tiny | 1.250 | 1.012 | 1.167 | 0.915 |
| Sparse point | 1.175 | 0.977 | 1.055 | 0.841 |
| Dense point | 1.819 | 1.590 | 1.192 | 1.004 |
| Dense long | 1.843 | 1.679 | 1.042 | 0.888 |
| Dense cross | 3.218 | 2.847 | 1.886 | 1.480 |
| No-op point | 1.229 | 0.939 | 1.191 | 0.900 |
| No-op cross | 1.799 | 1.587 | 1.720 | 1.513 |

A separate 24-sample repeat alternated before/after order for four grass cases and exercised
synchronous mode in the same candidate binary. All three variants produced identical signatures:

| Case | Before async / ms | After async / ms | Candidate sync / ms |
| --- | ---: | ---: | ---: |
| Empty tiny | 1.320 | 0.885 | 5.577 |
| Dense point | 1.787 | 1.557 | 8.181 |
| Dense cross | 3.238 | 2.782 | 9.875 |
| No-op point | 1.250 | 1.109 | 7.175 |

Sync elapsed time includes queue waiting; async sums enqueue/submit/finish phases across frames.
These are neither CPU utilization nor full-frame or display latency. Publication latency did not
uniformly improve (repeat no-op point median 8.939 → 9.652 ms). Do not infer smoother interaction,
RTX improvement, or a resolved cross-platform regression. No visible try-out ran.

## Incomplete full-suite acceptance

The full runner stopped on the twelfth case: candidate `place-dense-point` produced both
`616 / e71e86ed899b5005` and `647 / 350d613a715f28f7` (changed voxels / atlas hash), while its
root signature was unchanged. The candidate ran first in this pair; the runner did not reach
that pair's baseline. Eleven preceding pairs passed, including all seven async grass cases.

Separate before/after/before placement repeats, 30 measured samples each, all returned the
616-voxel result. The discrepancy's cause is **unconfirmed**; it is not established as a baseline
bug or a new regression. The failed event is retained, not filtered out or counted as a suite pass.
This limits aggregate acceptance despite the focused grass evidence and passing native checks.
The archive contains 42 runs, 702 measured events and 126 warmups (profiling, paired, repeated and
placement follow-up runs; native correctness reviews are separate).

## Correctness and reproduction

- `cargo fmt --check`, `cargo check`, `cargo test` (4 + 1775 passed, 15 ignored),
  `cargo test -p re-flora-vkn` (74 passed), `cargo build --release`: passed on this Mac.
- Native transaction review passed: queued/submitted disable and ordered subsequent sync paint,
  scripted API remaining synchronous with the switch on, nondefault competition-field snapshot
  matching synchronous output, conflicts, capacity, cancellation, hydration, audio publication,
  bounded retry, saving and atomic publication.
- Grass audio and soil-cover reviews emitted their success markers; hidden smoke exited cleanly.
  Inspected logs had no errors, panics or Vulkan validation failures. GUI migration/persistence
  tests passed; the checkbox was not manually clicked in a visible game.

Build the baseline and candidate in Release and keep binaries at
`target/grass-edit-preparation/bin/{before,after}`. Run from the repository root:

```sh
RE_FLORA_BRUSH_BENCH_ASYNC=1 node scripts/compare_brush_edits.mjs \
  target/grass-edit-preparation/bin/before target/grass-edit-preparation/bin/after \
  target/grass-edit-preparation/paired 12
node docs/evidence/brush-edit-delivery/preparation-repeat.mjs

RUST_LOG=info RE_FLORA_BRUSH_EDIT_BENCH=add-dense-cross \
RE_FLORA_BRUSH_BENCH_ASYNC=1 RE_FLORA_GRASS_TRANSACTION_REVIEW=1 \
RE_FLORA_BRUSH_BENCH_SAMPLES=3 \
RE_FLORA_BRUSH_BENCH_REPORT=target/grass-edit-preparation/transactions.json \
cargo run --release -- --hidden --mute --flat-terrain
```

Local logs/binaries remain under `target/grass-edit-preparation/`. The repeat runner sets the
in-memory checkbox via the existing benchmark environment, without saving configuration.
Future evaluation still needs the RTX device and continuous-interaction frame/presentation data.
