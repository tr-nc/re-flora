# Soil large-pile performance investigation

## Result (2026-10-07)

A behavior-equivalent, saved **runtime A/B** candidate is ready in Release. It improves large-pile editing; it does **not** demonstrate 10× or 100× total speedup.

**Debug → Soil Deposition → Soil: faster sampling and settling (A/B)**

- Unchecked: original sampling/planner mode. Default remains unchecked for comparison.
- Checked: sampling-reach estimate, request-local dense columns, lazy radial eligibility and adaptive sampled height.
- Same individual grains, source distribution, support/repose rules, quantity, material ownership and synchronous Visible Terrain Publication. No particle FX, rendering shortcut or global soil redistribution.

### Final interleaved Release measurements

RTX 3060 Ti, native hidden 5120×2880 window, automatic present mode, muted audio. Actual local settings: **512 voxels/batch, 20 batches/s, spread 13, clump 0, slope 1**, Infinite Materials. Committed spread default remains 0. Each workload ran **A1 → B1 → B2 → A2**, with 60 warm-up frames and no simultaneous re-flora GPU runs.

| Workload | Original | Candidate | Interpretation |
| --- | --- | --- | --- |
| 1,048,576-voxel pile, native LMB held 5 s: mean FPS | 16.53–16.56 | 58.19–58.31 | About **3.52×** frame-rate improvement in this reproduced workload |
| Same native run: frame p95 | 82.15–84.20 ms | 31.00–31.21 ms | Lower edit spikes, not zero hitches |
| Same native run: actual added volume | 48,640 | 49,152 | Candidate does not reduce output; one-batch difference is real-clock boundary/pacing |
| 524,288-voxel pile, native LMB held 5 s: mean FPS | 58.28–58.35 | 58.44–58.53 | Already near idle/presentation rate; little mean-FPS gain |
| Same 524,288 native run: frame p95 | 32.78–33.86 ms | 27.01–27.90 ms | Useful tail improvement despite similar mean FPS |
| 1,048,576 pile, fixed identical 512-voxel event: mean cost | 57.85–58.08 ms | 25.99–26.02 ms | About **2.23×** end-to-end edit-call improvement |
| 65,536 pile, fixed identical event: mean cost | 11.25–11.60 ms | 8.55–9.06 ms | No small-pile regression in this control |

Fixed replay deposits **40,960** voxels in each mode, using identical seeds/positions. All four million-pile runs end at hash **`4e5e1e5177ebbd05`**; all four small-pile runs end at **`2662e2d248a25965`**. This establishes byte-identical geometry/material output for those matched production-path workloads.

Native holding uses the real hover ray, player-tool action, wall-clock flow, stroke path, muted audio and publication. Frame pacing changes event grouping, source hits and RNG progression, so native final hashes are **not** expected to match and are not used as an equivalence oracle. Shared starting hashes are `a519a462983da705` (million) and `eeaccc87b394d095` (524,288). Per-call accounting and final atlas count verify every requested/written voxel.

The user's approximately 30 FPS observation was not an exact captured scene. Our larger fixture reproduces an even stronger slowdown; these numbers must not be advertised as a guarantee for their hill, another GPU, resolution or settings.

## Checkpoint, configuration and rollback

- Visual-approved checkpoint: **`fea03f56`**.
- Annotated local tag: **`soil-before-perf-20261007`**, created before experiments. Not pushed; not an official release tag.
- Exact pre-experiment local config, patch, revision and baseline executable: `target/soil-performance-baseline/`.
- User-local spread **13** and numeric-format changes remain uncommitted. No unrelated defaults changed.
- Immediate runtime rollback: uncheck the A/B control. Source rollback: use the checkpoint tag after preserving local work/config. A commit tag alone does not preserve uncommitted settings; use the backup as well.

## Reproduce

Use this worktree and serialize GPU runs. Do not launch a visible game automatically.

```bash
# Native held-input candidate. Omit RE_FLORA_SOIL_PERF_FAST for original mode.
env -u WAYLAND_DISPLAY \
  RE_FLORA_SOIL_PERF_REPORT=target/soil-perf/held-million-candidate.json \
  RE_FLORA_SOIL_PERF_FAST=1 \
  RE_FLORA_SOIL_PERF_FIXTURE=1048576 \
  RE_FLORA_SOIL_PERF_HOLD_SECONDS=5 \
  RUST_LOG='warn,re_flora::app::core::soil_performance_bench=info,re_flora::builder::plain=info,re_flora::app::core::visible_terrain=info,re_flora::run_log_binding=info' \
  flock --close --timeout 120 /tmp/re-flora-summer-gpu.lock \
  cargo run --release -- --hidden --mute --rooftop-poc --perf --auto-exit 60
```

Omit `RE_FLORA_SOIL_PERF_HOLD_SECONDS` for **fixed-work replay**: one saved-size batch every three rendered frames, 240 measured frames/80 events after 60 warm-up frames. Its rendered FPS is not the player's wall-clock holding FPS. `RE_FLORA_SOIL_PERF_IDLE=1` provides the static-render control. Overrides: `RE_FLORA_SOIL_PERF_FRAMES`, `RE_FLORA_SOIL_PERF_WARMUP`, `RE_FLORA_SOIL_PERF_FIXTURE`.

The harness requires an empty isolated rooftop atlas and Infinite Materials. Reference fixture construction/upload/publication is unmeasured. Versioned parameter-keyed artifacts under `target/soil-perf/fixtures/` verify byte checksum, size and Dirt count on reuse; corrupt artifacts fail explicitly. Generation uses the original planner, not the candidate. This is benchmark fixture reuse, **not a production occupancy cache**.

Optional `RE_FLORA_SOIL_PERF_MAX_P95_MS=33.333` reproduces the approximate 30-FPS hitch scale; it is **not** a new project acceptance budget. Reports are written before a requested gate fails.

## Diagnosis and experiment sequence

The first large replay measured 47.571 ms mean/86.011 ms event p95 and frame p95 87.109 ms. A repeated unchanged stage probe measured 27.371 ms mean/31.017 ms event p95. **The initial extreme tail is not a stable baseline**; final claims use repeated interleaved runs, not the most dramatic first number.

Static 524,288-pile control: frame mean **17.075 ms**, p95 **17.984 ms**. Large geometry alone does not reproduce the edit spikes. `perf` is unavailable on this host; no CPU flame graph is claimed.

Initial stage means at 524,288 voxels: readback **9.564 ms**, planning **9.912 ms**, crop **0.146 ms**, upload **1.454 ms**, dirty flags **0.108 ms**, publication roughly **4–8 ms**. Each event repeated one expansion and read **8,125,440 bytes**.

| Validated step | Large event mean | What changed |
| --- | --- | --- |
| Original matched mode | 26.946 ms | Baseline |
| Reach estimate | 21.126 ms | Avoid repeated speculative expansion near the same source/legal bounds; every request still reads authoritative bytes |
| Dense column lookup | 18.789 ms | Lazy request-local X/Z table replaces neighbor hashing; blocked columns and ceiling intervals preserved |
| Lazy radial eligibility | 18.503 ms | Reject impossible downhill candidates before unchanged Euclidean `hypot`; no metric approximation |
| Adaptive sampled height | 14.787 ms | Sample legal bottom through source height + 2; detect unread top, expand/replay before upload |

These intermediate runs isolate experiments but are not substitutes for the final order-reversed comparisons. Each validated step was committed before the next.

### Sampling is not a physical cap

The remembered halo is only a spatial reach estimate. No occupancy/state bytes survive across requests. Movement/legal-bound changes fall back to the original minimum; adaptive expansion still handles growth. The estimate can conservatively over-read after large nearby terrain edits; disabling/re-enabling resets it. This affects performance, not correctness.

In checked mode, ascending occupancy or an accepted column reaching unread Y sets `needs_more_height`; speculative writes are discarded, Y expands, and the unchanged atlas/request/seed is replayed. Unknown space is never silently accepted as a real ceiling. X/Z and Y expansion are independent; legal world/workspace limits remain authoritative. Existing caves, caps, moisture bytes and finite inventory remain intact.

All ordinary deposition fixtures execute both lookup planners and assert identical result and every atlas byte. A separate cropped-height replay regression covers a rock cap, buried source, state bytes and discarded plans. Existing roundness, support, interior-void, quantity and adaptive-replay assertions remain unchanged.

## Remaining costs and 10× / 100× potential

Matched million-pile fixed replay stage means (original → candidate):

- Readback: **26.111 → 11.217 ms**, bytes **30,769,152 → 12,043,098** including retries.
- Planning: **20.274 → 3.645 ms** — about **5.56×** for this component.
- Crop: **0.386 → 0.381 ms**.
- Upload: **2.505 → 2.428 ms**.
- Dirty flags: **0.150 → 0.099 ms**.
- Synchronous publication still costs roughly **8 ms** for four touched chunks.

Targeted `[PERF][SOIL_READ]` splits show transfer/wait dominates, not a slow byte loop: a 6,500,352-byte read commonly takes 6.3–7.6 ms transfer/wait versus 0.78–0.95 ms host slice copy. The buffer implementation already uses a normal slice copy.

**No measured 10×/100× end-to-end improvement.** Simply accelerating the CPU planner cannot remove transfer, publication or rendering. At the measured workload, even hypothetically removing all remaining readback/planning leaves about 11 ms of edit-call work. Candidate native mean FPS is already near the approximately 59-FPS idle/presentation rate at these rendering settings.

Potential deeper research, not implemented or promised:

1. GPU occupancy planning or a compact, lossless query/delta path to avoid dense round trips; must preserve arbitrary old material/state bytes, exact writes and accounting.
2. A revision-aware authoritative CPU mirror; requires complete invalidation coverage for every atlas writer, not a stale per-stroke shortcut.
3. Coalesced resource-specific transfers/publication work while retaining coherent synchronous visibility. The existing one-time-command implementation records a MoltenVK fence/recycling crash history; replacing global wait-idle blindly is unsafe.
4. Release measurements of moving strokes, larger spread, garden caves/mixed materials, maximum 1024 batches and other GPUs.

Cold native candidate frames still reached **71–73 ms** in final million runs; p95 is approximately 31 ms, not a no-hitch guarantee. Normal-garden terrain save/load end-to-end acceptance is still deferred.

## Validation and artifacts

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed; GUI structs regenerated by normal builds, never hand-edited.
- Deposition **23 passed**, player tools **13 passed**; saved checkbox uses the declarative field and generic persistence regression.
- Full `cargo test`: **1419 passed, 5 failed, 6 ignored**; collision benchmark **4 passed**. The same five previously reproduced unrelated default assertions remain; full suite is not green.
- Checked-mode real rooftop: **4224** supported writes, preserved old materials/floor/publication/inventory, production LMB batch **1/10/50/512/1024**, exact click/hold counts and no short-tap repeat.
- Checked-mode normal-garden supply: finite-empty **0**, infinite **16**, finite-stock **7**, exact debit and real inventory preserved.
- Normal hidden muted startup and all matched Vulkan runs exited successfully. Logs inspected without ERROR/panic/VUID; existing fruit-physics hitch warning remains. Final startup log: `target/re-flora-logs/re-flora-20261007-153927.596-2077766.log`. No visible game launch or official packaging.
- Updated offline HTML report checked in Chromium at 1280×900 and 390×844: no page-wide overflow, missing anchors or broken images. Performance section axe scan: 0 violations / 0 incomplete checks. Screenshots: `target/soil-performance-report-{desktop,mobile}.png`.

Final reports: `target/soil-perf/final-{fixed-million,held-million,held-large,fixed-small}-{a1,b1,b2,a2}.json`. Final fixed million hash: `4e5e1e5177ebbd05`. Logs: `/tmp/terrain-magic-soil-perf-final-*.log`; full test log: `/tmp/terrain-magic-soil-perf-final-all-tests.log`.

Earlier controls/experiments: `target/soil-perf/baseline-{large-1,idle-large,small}.json`, `profile-large.json`, `reach-{baseline-large,large}.json`, `dense-large.json`, `lazy-large.json`, `height-{large,million}.json`.

## Primary sources and implementation

- [NVIDIA APOD / Amdahl's law](https://docs.nvidia.com/cuda/cuda-c-best-practices-guide/index.html#strong-scaling-and-amdahl-s-law): measure hotspots, minimize host/device transfer and bound total speedup by the optimized fraction. General principles, not CUDA-specific performance predictions for this Vulkan application.
- [Khronos wait-idle sample](https://github.khronos.org/Vulkan-Site/samples/latest/samples/performance/wait_idle/README.html): broad idle waits drain the pipeline; resource-specific synchronization can reduce bubbles. Its 22% Mali example is not a prediction for this GPU/app.
- `src/terrain_deposition.rs`: supported per-grain planner, dense lookup and speculative retry contract.
- `src/builder/plain/mod.rs::deposit_soil`: authoritative sampling/replay/crop/upload/invalidation.
- `src/app/core/soil_deposition.rs`: inventory ownership and coherent publication.
- `src/app/core/soil_performance_bench.rs`: fixed-work/native app benchmark.
- `docs/terrain_visual_rebuild_pipeline.md`, `docs/performance-benchmarking.md`: publication policy and authoritative Release benchmarking.
