# Grass audio main-thread readback regression

## Impact and introduction

On 2026-10-29, the default fresh garden on an NVIDIA RTX 3060 Ti (driver
580.159.04, Linux/Wayland) ran at approximately 1 FPS in Release with grass
rustle enabled. The last half of the original eight-second sample had median
1.3 FPS (0.9–1.6), at a 2400×1350 swapchain extent. Disabling only grass rustle
restored approximately 86–93 FPS. Global `--mute` silences output but preserves
processing; it is not equivalent to disabling the grass category.

The failing observer was introduced by `b78a6eca` (`add wind-driven grass patch
audio and saved tuning`, 2026-10-29 15:42 +0800).

## Confirmed cause

`FloraInstanceResources::rebuild_grass_audio` copied the entire 8 MiB packed
potential field and both occupied grass streams directly from mapped `CpuToGpu`
allocations. On this device their actual flags were
`DEVICE_LOCAL | HOST_VISIBLE | HOST_COHERENT`, **without `HOST_CACHED`**.
Host coherence provides visibility, not fast CPU reads.

A temporary split timing probe measured approximately 283–491 ms for the
potential copy, 12–31 ms for instance copies, and about 1 ms for the actual
projection. One original frame's aggregate `projection_us` was 974072.
Growth/hydration changes keep invalidating the projection, so limiting refresh
to one chunk per frame did not prevent recurring stalls.

The potential field was already computed on the CPU, uploaded, and discarded.
Reading it back was unnecessary. Instance growth and structural edits do occur
on the GPU, so those streams need a real committed CPU observation rather than
an independently simulated copy.

## Ownership and fix

`FloraInstanceResources` owns both publications:

- Retain CPU-authored packed potential words. `publish_grass_growth_potential`
  validates/uploads the field before publishing the CPU words and shape revision.
  The same packed-word decoder is used for acoustic sampling.
- Allocate a bounded tall/short grass readback buffer with `GpuToCpu` memory
  (the allocator prefers host-cached, coherent memory and requires coherence).
  Surface creation, occupancy edits and growth copy into it **inside their
  existing submission**, followed by a transfer-to-host barrier. Existing fences
  establish completion; the audio observer adds no submission or wait.
- CPU writes, append and snapshot/undo restoration update the readback publication
  before exposing the new count. Hydration reconciliation and audio read this
  same committed stream. Pending surface builds remain unreadable and inaudible.
- Keep both 40,000-instance capacities and the existing 8-byte GPU vertex layout.
  Additional retained storage per 256³ chunk is 8 MiB of CPU potential words plus
  640,000 bytes of readback storage. A GPU write copies at most 640,000 bytes;
  ordinary projection no longer reads the uncached draw allocation.

No sound category, saved setting, voice budget, refresh policy or rendering
quality was disabled or reduced. No save-format or shader change was needed.

## Release measurements

Desktop scaling changed during investigation, so the following controlled pair
both used the later **2880×1620** native swapchain extent. Both ran sequentially,
without a concurrent build/test, using the default fresh garden and:

```sh
RUST_LOG=info,re_flora::builder::surface::grass_audio=debug \
  cargo run --release -- --hidden --mute --auto-exit 8 --perf
```

| Measurement | Negative control | Final fix |
| --- | ---: | ---: |
| FPS, median of last half of post-ready FPS samples | 2.0 | 89.3 |
| FPS, range of that half | 1.3–4.1 | 82.7–92.6 |
| Nonempty chunk projection median | 234.294 ms | 1.149 ms |
| Nonempty chunk projection p95 | 586.842 ms | 1.257 ms |
| Nonempty chunk projection maximum | 612.612 ms | 1.310 ms |

The negative control temporarily reinstated **only** the original full potential
readback and byte decoding in the audio observer; it retained the new instance
readback. It is a causal fault-injection comparison, not a full old-commit
benchmark. All temporary changes were removed before final validation.

Projection statistics include every nonempty chunk rebuild after
`Loading complete; first game frame ready`; FPS statistics exclude loading and
use the final half of those periodic samples. Final audio telemetry reported
75 candidate patches and 12 active voices. These are local measurements, not a
cross-platform performance guarantee.

## Regression and correctness validation

```sh
RE_FLORA_GRASS_AUDIO_REVIEW=1 \
  cargo run --release -- --hidden --mute --auto-exit 8 --perf
```

Require the `[GRASS_AUDIO_REVIEW]` success marker, not merely exit status: a very
short auto-exit can finish before this startup review runs. The opt-in review
compares CPU observations byte-for-byte with real GPU draw streams and checks
CPU append, pending-build exclusion, growth-potential publication/competition,
GPU growth, mowing, removal/undo, snapshot restoration, hydration, bare ground,
tall/short grass, vertical separation and terrain support removal.

Its 80,000-root, 12-sample projection benchmark has a deliberately loose 50 ms
median **stall-regression guard**, not a general frame-time acceptance budget.
The same review failed with the negative control:

```text
grass audio runtime acceptance: grass projection stall regression: median=493.810ms (limit 50ms)
```

Final success included:

```text
[GRASS_AUDIO_REVIEW] cpu_draw_match=true cpu_append=true pending_excluded=true competition=true bare=true tall_short=true elevations=true override=true mowing=true gpu_growth=true remove=true undo=true load=true hydration=true terrain_remove=true max_roots=80000 projection_median_ms=2.508 projection_max_ms=2.525 idle_mean_us=1.4
```

Other validation:

- `cargo fmt --check`: passed.
- `cargo check --bin re-flora`: passed.
- `cargo test --bin re-flora`: 1,783 passed, 15 ignored, no failures.
- `cargo build --release --bin re-flora`: passed; main Release binary refreshed.
- Final hidden Release performance/review logs had no warnings, errors, panics or
  Vulkan validation errors. Default Vulkan validation layers remained enabled.
- Full `cargo check`, `cargo test`, and `cargo build --release` are blocked by the
  pre-existing missing `std::fs` import at `src/bin/collision_bench_rapier.rs:821`
  (E0433 and consequential E0282). This unrelated target was not changed.
- No generated files or saved GUI settings changed.
