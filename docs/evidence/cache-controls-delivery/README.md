# Cache-only flower controls and quiet logs — integrated acceptance

Target branch **`vegi`**, base `3eeebc34`; validated code **`2fb853df`**.
Cache-first implementation and flower controls were integrated before final tests;
`main` was not changed, pushed or released. No visible game was launched.

## Delivered

- Removed continuous/zero-view model rendering, per-frame triangle/repair streams,
  consumer generator imports and live geometry fallbacks. Transitive shader and
  Rust publication tests prevent the old route from being reused by new consumers.
  Geometry sampling exists only in cache baking and isolated bake validation.
  Per-instance pose, wind, lighting, cached RGBA/depth lookup/display still run.
  Ordinary mesh stems, voxel foliage and non-model sprites are not removed.
- All eight flowers expose saved **Complete Head Size**, **Plant Height**,
  **Pixel Resolution**, **View Count** under **Debug → Flora → Ground Plants →
  Model Flowers (A/B)**. Existing overall scale and A/B remain compatible.
  Head scale includes corolla, center and calyx around authored attachment anchors.
  Height scales stems and moves anchors without stretching the flower heads.
- One transformed source serves native stems and cached whole/head modes. Only
  the flower bank rebuilds for flower shape/resolution/direction changes.
  Directions and surface payloads follow current demand, including shrinkage.
  Existing frame slots, cache owner and descriptor lifetimes are used; no new
  flower-only cache/retirement subsystem.
- Deleted routine DDGI history, successful footstep/wet/voice, cicada/acoustic
  and ecology messages. Kept real warnings/errors and critical lifecycle reports.
  Audio playback, telemetry attribution, deadlines and cleanup remain intact.

## Integration findings corrected

1. Changing shape by 0.0001 switched from authored framing to a new tight frame:
   whole-frame radius jumped 21–29%. The new regression failed first. `22247171`
   preserves exact authored head framing and continuously fits whole plants around
   the transported authored center with the original relative margin. The final
   native sweep below validates these corrected bounds, not the worker's earlier fit.
2. Two DDGI tests asserted text in the deleted per-texel apple path. They now
   verify shared object-light sampling and both attached/fallen cache consumers;
   terrain-only path tracing remains excluded from raster lighting.
3. The real model benchmark caught missing initial effective-mode telemetry when
   the requested 16 views matched the constructor. `2fb853df` uses an explicit
   unpublished display state, not a zero-view rendering sentinel. The misleading
   `live_tiles` label is gone. Default startup is logged once, never every frame.

Worker fixes retained shader artifact checkout identity, ready-slot descriptor
owner retirement, and the missing padding-layout lifetime exposed by transient
bake inputs. Failed reproductions are retained as failures, not green evidence.

## Final executed validation

- `cargo fmt --check`, `cargo check`, `git diff --check`: passed.
- `cargo test`: **1,246 passed, 4 ignored**, plus **4 other root tests**.
- `python3 scripts/run_slang_tests.py`: **25 passed**.
- VKN paged storage **4 passed**; descriptor retirement **1 passed**.
- 18-phase controls sweep, all eight species: **4,853,824 baked records**, zero
  bit mismatches; **106,932 independent occupied cells**, max depth error
  **0.000009924**, unchanged bound 0.00002. Includes independent size/height edits,
  opposite slider extremes, 8/32/64px, 8/16/37/512 directions, both modes,
  retained overall scale/growth, isolated rebuilds and post-consumer resize.
- Four-bank matrix: **14,534,528 baked records**, zero mismatches; **157,288**
  independent occupied cells, max depth error **0.000006706**. All consumers,
  actual apple drops, multiple cache pages and resize exercised.
- Flower A/B + nine lifecycle phases, apples, migrated cache-only leaf and
  butterfly native validators: passed. Retired continuous perspective/906-pose
  validators are not presented as passing; see the worker migration record.
- Ordinary hidden muted Release 0.5s and 45s runs: successful shutdown,
  `failures=0`, no application/Vulkan errors. Logs inspected using built-in helpers.
- Runtime validation did not alter `config/gui.toml` or player saves. Config diffs
  are the three saved flower declarations and removal of obsolete self-shadows.

### Demand sizing

The controls sweep observes flower surface payloads between **344,064 and
22,020,096 bytes**, according to current settings. The four-bank matrix also
exercises 64px flowers and multi-page 64px leaves. Shared direction storage follows
**256 → 8192 → 592 → 256 bytes** for current maximum counts 16 → 512 → 37 → 16.
Defaults do not allocate for the slider's maximum. Completed-slot residency returns
to active payload plus exact page/table overhead. These are buffer residency
figures, not driver allocator heap reservations.

Production currently has one frame slot. Native runs verify replacement after
its fence completes; deterministic multi-slot storage/descriptor tests verify
pending ownership. Overlapping multi-slot GPU execution is not claimed.

### Release performance (no geometry oracle during timing)

RTX 3060 Ti, Vulkan/X11, output 2880×1620, scene 1440×810, 16 views, same 32px heads.
Eight-second runs, warmup excluded; measured intervals, not reciprocal GPU time.

| Case | Earlier shared-cache tile p50 | Current tile p50 | Current actual FPS |
|---|---:|---:|---:|
| 128 plants facing plot | 0.066 ms | 0.048 ms | 58.3 |
| 128 plants looking away | 0.066 ms | 0.050 ms | 58.3 |
| 1,024 plants facing plot | 0.383 ms | 0.226 ms | 58.3 |

1,024 plants still submit 2,048 head tiles and build four banks once. Current
whole-frame GPU p50/p95: **7.603/7.834 ms**. Original uncached 128-plant work was
18–19 ms. No quality/population reduction; no new per-instance culling.
With 256 model leaves and 21 butterflies, particle tiles take **0.033–0.034 ms**;
32px attached/fallen apple tiles **0.031/0.027 ms**. Existing default-32px GPU and
cadence gates pass. These short runs are not maximum-population/device acceptance.

### Logs and remaining limits

Controller before/after 45-second normal runs: DDGI history **60 → 0**; targeted
routine audio/ecology messages **37 → 0**; formatted runtime lines **283 → 188**
(excludes Cargo warnings). Both runs completed five cicada calls with no audio
queue warning. A worker previously saw one control-queue-full warning; it remains
visible and was not reproduced, not claimed fixed. Existing fruit startup-hitch
warnings occur on both revisions. Idle/muted runs are not walking/listening tests.

High settings rebuild synchronously and can hitch or request GiB. Maximum combined
settings and other devices were not tested. The known VKN retired-shader artifact
unit test is not fixed or hidden. Final visual approval remains with the user.

## Evidence and resource cleanup

[Machine-readable data, complete matrices and log hashes](summary.json),
[native diagnostic scene](native-scene.png). Actual raw controller evidence is
under `target/cache-controls-delivery/acceptance/`; all three workers' evidence was
copied to `target/cache-controls-delivery/workers/` before cleanup. No performance
claims are based on debug builds or CPU unit tests.

Cleanup completed: `vegi-cache-only`, `vegi-flower-controls`, `vegi-quiet-logs`,
their three branches and private build copies were removed normally after clean
status and ancestry checks. Only `main` and `vegi` remain. The unused GPU lock file
was removed too. No unrelated checkout or player data was removed. Retained
`target/cache-controls-delivery/` is about **179 MiB**, kept as acceptance evidence;
it can be deleted after review if raw logs/images are no longer needed.

Workflow reported 3 highest-tier agents, approximately 622.4K tokens with 20.5M
cached tokens listed separately, and $31.36 estimated cost; controller usage is not included. Its
reported duration was 5653.3 seconds, not a measured sum of all agent execution
intervals. These are runtime estimates, not an invoice.
