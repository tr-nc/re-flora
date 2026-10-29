# Brush edit reduction acceptance

Historical measurements on Apple M4 Pro / macOS MoltenVK, hidden native FIFO surface,
5120×2880 screen / 1024×576 scene. No visible window, perf panel or saved-default changes.
Baseline binary: `cd00604c` (research baseline plus acceptance replay).
Local-surface candidate: `9bb78821`; no-op candidate: `e5a79463`.
Binary hashes are in [binaries.sha256](binaries.sha256).

## Reproduce

Build each revision using `cargo build --release`, preserve its binary, and run from the
repository root:

```sh
node scripts/compare_brush_edits.mjs BASELINE CANDIDATE OUTPUT_DIR 12
```

The runner alternates binary order, never runs GPU samples concurrently, requires clean
shutdown/error-free logs, checks every sample's canonical root/instance payload and material
hashes, and compares both versions exactly. The in-app replay separately verifies surviving
root payload/hydration and repeated-dab idempotence. Each case has three excluded warmups.
Event cost excludes picking, fixture reset, validation and future frames; **not input-to-present**.
Raw per-run JSON/logs remain under `target/brush-edit-delivery/{local-surface,noop}` on the
measurement machine; committed summaries preserve sample median/p95 and exact signatures.

## Results

- [local-surface.json](local-surface.json): 12 cases × 2 binaries × 9 measured samples.
- [noop.json](noop.json): 12 cases × 2 binaries × 12 measured samples.
- Across both rounds: 504 measured edits, 144 warmup edits; all paired output signatures match.
- Final baseline → candidate median ms: dense point add **7.169 → 5.574**;
  dense cross-chunk add **10.469 → 7.210**; repeated point **7.144 → 4.578**;
  repeated cross-chunk **10.379 → 5.352**; empty removal **4.701 → 0.006**.
- Terrain dig **8.422 → 8.515**, place **8.481 → 8.482**: effectively unchanged, not a terrain speedup.
- Shared-queue wait remains. These within-round comparisons are not comparable to the older
  research medians as exact gains: background clock/queue conditions drift.

No persistent surface cache was introduced: the shared canonical extractor computes only
brush tiles against current terrain, preserving its normal halo and packing. This removes the
extra full-chunk build/submission/wait without a new cache-invalidation contract. Removal and
trim do not need surface extraction. Whole-chunk occupancy clear and changed-result compaction
remain; edit dispatch is local. GPU admission shares the actual placement policy, so rejected
candidates cannot falsely trigger compaction. A zero indirect dispatch preserves instance order
and skips CPU root/hydration reconciliation. Authored trim publication remains independent.

Validation: `cargo fmt --check`, `cargo check`, `cargo test` (4 + 1774 passed, 15 ignored),
`cargo build --release`, hidden muted smoke and log inspection. Also ran
`RE_FLORA_GRASS_SOIL_COVER_REVIEW=1 cargo run --release -- --hidden --mute --auto-exit 0.5`:
bare soil, flowers excluded, both grass species, chunk borders, tight/interior-gap coverage,
last-root removal, moisture, restoration, burial and uncover all passed.

Async editing is deliberately not part of these main-branch changes.
