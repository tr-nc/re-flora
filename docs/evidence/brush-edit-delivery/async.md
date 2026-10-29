# Async grass acceptance evidence

Feature code: `25b3a526`; reference: `e5a79463` (the reduction implementation shipped on
`main` at `f20c7fc3`). Same Apple M4 Pro/macOS MoltenVK, hidden native FIFO surface and
5120×2880 / 1024×576 dimensions as the reduction measurements. Default validation layer
enabled. No visible game or saved-config changes. This is not a three-platform release test.

## Reproduce

Preserve the reference Release binary, build the feature with `cargo build --release`, then:

```sh
RE_FLORA_BRUSH_BENCH_ASYNC=1 node scripts/compare_brush_edits.mjs \
  REFERENCE_BINARY FEATURE_BINARY OUTPUT_DIR 12

RE_FLORA_BRUSH_EDIT_BENCH=add-dense-cross \
RE_FLORA_BRUSH_BENCH_ASYNC=1 RE_FLORA_GRASS_TRANSACTION_REVIEW=1 \
RE_FLORA_BRUSH_BENCH_SAMPLES=3 RE_FLORA_BRUSH_BENCH_REPORT=target/transaction-review.json \
RUST_LOG=info cargo run --release -- --hidden --mute --flat-terrain
```

The reference ignores the new async benchmark variable. The feature queues through the real
application admission interface, submits at the real frame tail and renders normally until the
completion poll publishes. Ordinary scripted APIs retain synchronous semantics.

- [async-samples.json](async-samples.json): all raw event data, including excluded warmups.
- [async-summary.json](async-summary.json): final 12-case paired comparison, 288 measured
  edits plus 72 warmups; every canonical root/instance/material signature matched.
- [async-validation.log](async-validation.log): test and native acceptance markers.
- [async-sha256.txt](async-sha256.txt): measured binaries and full local acceptance-log hashes.
  Full logs remain under `target/brush-edit-delivery/async-final` and `async-acceptance.log`.

## Interpretation — CPU cost is not display latency

For async samples, `edit_ms` is **enqueue CPU + cumulative submit CPU + finish CPU**, including
explicit drain waits/retries when used. Those phases can occupy different frames. It is not
wall-clock completion latency, input picking, GPU work reduction, or input-to-present latency.
Publication latency is separately recorded. Root/water validation is outside timers; async
latest-water preservation is asserted by the native transaction acceptance, not by assuming
that ecology cannot change water between frames.

Final warm-sample median ms:

| Case | Synchronous main edit | Async CPU phases | Accepted → publication |
|---|---:|---:|---:|
| Dense point add | 5.443 | 1.819 | 7.541 |
| Dense cross-chunk add | 7.052 | 3.368 | 8.672 |
| Repeated dense point | 4.643 | 1.256 | 7.041 |

All measured async paint samples completed on the following frame with zero conflicts.
Removal/terrain remain synchronous and showed no material change in this comparison. Drift
between runs still precludes treating historical medians as exact additive gains.

**Costs not hidden by the warm table:** first-use submit allocation was 7.86–10.71 ms in the
seven paint scenarios; the first cross-chunk add used 11.73 ms total CPU. The lazily allocated
private surface/occupancy pair adds 128 MiB, plus per-chunk snapshot/output buffers, bounded to
eight chunks. Conflict/backpressure/save boundaries can deliberately block. Actual
input-to-visible-presentation latency and subjective responsiveness have **not** been verified;
lower main-thread CPU occupancy does not prove lower display latency. No visible try-out ran.

## Validation

`cargo fmt --check`, `cargo check`, `cargo test` (4 + 1774 passed; 15 ignored),
`cargo test -p re-flora-vkn` (74 passed), `cargo build --release`, hidden muted smoke and log
inspection passed. Native acceptance verified:

- no live changes before all-chunk publication; abandoned recordings do not advance versions;
- host-write, GPU-growth and real terrain-publication conflicts reject the whole batch;
- latest hydration, water reserves and authored-species payload/counts survive publication;
- capacity overflow is contained in private output and rejects without live changes;
- cancellation, lossless backpressure, captured selection across tool switch, bounded retries,
  and saving a queued edit through the real snapshot quiescence boundary;
- existing soil-cover acceptance (bare/flowers, tall/short, chunk borders, moisture, removal,
  restoration, burial/uncover) still passes.

Ownership rationale and the intentionally separate terrain/paid-planting contracts are in
[the decision](../../adr/async-grass-edit-transactions.md).

## Integration with main's grass audio readback fix

Merged local `main` at `1efbd1be` into this branch's `6bc90a06`. The moved synchronous
recording code keeps the shared async/sync backend; the new grass readback copy belongs
in that backend. Transaction publication swaps the completed CPU-readable stream with
its matching draw buffer. Input snapshots use retained CPU potential and committed
CPU-readable grass rather than reintroducing the uncached VRAM reads fixed on main.

Linux/RTX 3060 Ti Release validation (not a replacement for the macOS comparison above):

- `cargo fmt --check` and `cargo check --bin re-flora` passed.
- `cargo test --bin re-flora`: 1,783 passed, 15 ignored.
- `cargo test -p re-flora-vkn`: 75 passed.
- The transaction acceptance command above passed, including added byte-for-byte
  CPU/draw publication checks before publication, after success and after rejection,
  and a growth-potential version conflict. All three measured cross-chunk additions
  published on the following frame with zero retries; each retained root signature
  `aa1fbc943423214a` and atlas signature `14c322ad44e42325`.
- `RE_FLORA_GRASS_AUDIO_REVIEW=1 cargo run --release -- --hidden --mute --auto-exit 8 --perf`
  passed with the full success marker: 80,000-root projection median 2.695 ms,
  maximum 2.912 ms, cached query mean 1.6 µs.
- A separate default-garden hidden Release run with grass audio enabled retained
  12 active voices and approximately 80–87 FPS late in the sample. The transaction,
  audio review and default-garden logs reported no Vulkan validation errors or panics.
- Main application Release build succeeded. Full `cargo check` / `cargo build --release`
  remain blocked by the existing Linux-only `std::fs` import error in
  `src/bin/collision_bench_rapier.rs:821`; this unrelated target was not changed.
- Generated files and saved GUI settings did not change. No visible try-out or push ran.
