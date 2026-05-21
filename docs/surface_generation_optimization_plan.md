# Surface generation optimization plan and results

Branch context: `agent/contree-optimizations` after contree Phases 1-4.

## Status summary

Implemented and validated:

- Phase 0: surface/flora GPU timestamp instrumentation.
- Phase 1: removed redundant GPU clears from the normal surface path.
- Phase 2: replaced normal flora rebuild with a direct active-surface-brick shader.
- Phase 5 subset: replaced duplicate normal-length square-root work with a squared-length check.

Final measured run: `target/re-flora-logs/re-flora-20260521-170231.671-68224.log`.

| Metric, 8 chunk tree rebuild | Baseline after contree (`160303`) | Final surface work (`170231`) | Delta |
| --- | ---: | ---: | ---: |
| Total rebuild | `26.36ms` | `17.76ms` | `-8.60ms` / `-32.6%` |
| Surface total | `23.41ms` | `14.74ms` | `-8.67ms` / `-37.0%` |
| Contree total | `1.57ms` | `1.50ms` | roughly unchanged |
| Scene texture total | `1.27ms` | `1.37ms` | roughly unchanged |

The main win is Phase 2: normal flora rebuild no longer performs full-volume occupancy passes. In the final run, the active-surface flora GPU pass is `0.011ms` to `0.083ms` per tested chunk instead of the previous CPU-side flora bucket of about `1.00ms` to `1.47ms` per chunk.

## Baseline evidence

After contree optimization, the tested 8-chunk tree rebuild was dominated by surface generation.

Evidence from `target/re-flora-logs/re-flora-20260521-160303.041-54290.log`:

- 8-chunk rebuild total: `26.36ms`
- Surface total: `23.41ms`
- Contree total: `1.57ms`
- Scene texture total: `1.27ms`

Representative per-chunk surface lines in that rebuild:

- Total per chunk: about `2.53ms` to `3.56ms`
- Fence latency per chunk: about `1.46ms` to `2.34ms`
- Flora rebuild per chunk: about `1.00ms` to `1.47ms`
- Active surface voxels range from tiny chunks (`1,383` to `2,247`) to larger chunks (`135,665` to `158,318`)

Conclusion: surface optimization was worthwhile because the path still did full-volume work over `256^3` chunks and then ran flora generation as additional full-volume passes.

## Current algorithm summary

`shader/builder/surface/make_surface.comp`:

1. Dispatches over the whole chunk volume.
2. Preloads an `8x8x8` workgroup plus halo into shared memory.
3. Skips empty voxels.
4. Skips fully occluded solid voxels.
5. Computes a normal from a `5x5x5` neighborhood for exposed voxels.
6. Writes packed surface data to a scratch `surface` image.
7. Emits active surface brick metadata used by the optimized contree and flora paths.

`SurfaceBuilder::finish_build_surface` optionally calls `seed_and_rebuild_flora_from_surface`, which now:

1. uses `make_surface`'s active surface brick list;
2. scans only the 64 voxels inside each active surface brick;
3. loads packed surface data directly;
4. checks plantability/density/biome;
5. writes flora instances directly.

The occupancy-based path remains for preserve-flora edits, trimming, growth, and instance-to-occupancy flows.

## Phase 0: add surface/flora pass timing

Implemented in commit `23e391e9`.

Added timestamp instrumentation equivalent to contree pass timing for:

- surface image clear;
- active brick flags clear;
- `make_surface.comp`;
- flora rebuild passes;
- flora edit/growth passes.

This split the previous coarse `fence_latency` and `flora` buckets into GPU pass costs.

Validation used:

```bash
cargo fmt --check
cargo check
cargo test
source ~/.zshrc
cargo run --release -- --hidden --auto-exit 0.5
```

## Phase 1: remove obviously redundant work

Implemented in commits `772b7517` and `5de6f4da`.

Changes:

- Removed the unconditional `occupancy_data` clear from `submit_build_surface`; `make_surface` does not read `occupancy_data`, and flora/edit paths clear or rebuild occupancy before using it.
- Moved `make_surface_result` cleanup to CPU pre-submit, removing a GPU clear pass from the surface command buffer.

Measured result in `target/re-flora-logs/re-flora-20260521-165233.931-48567.log` after Phases 0-2 plus this cleanup:

- 8-chunk rebuild total: `18.09ms`
- Surface total: `14.96ms`
- Contree total: `1.60ms`
- Scene texture total: `1.44ms`

## Phase 2: build flora directly from active surface bricks

Implemented in commit `7072d38a`.

Original plan used active surface voxels; the implemented version uses the active surface brick list that `make_surface` already emits for contree. This avoids adding another active-voxel buffer while still limiting flora work to active surface bricks.

Changes:

1. Added `shader/builder/surface/active_surface_to_flora_instances.comp`.
2. Replaced normal startup/tree-add flora rebuild's full-volume path with a direct active-surface-brick pass.
3. Kept the occupancy-based path for edit/growth workflows.

Final run evidence from `target/re-flora-logs/re-flora-20260521-170231.671-68224.log`:

- large active chunks: `active_surface_to_flora=0.071ms` to `0.083ms` GPU;
- small active chunks: `active_surface_to_flora=0.011ms` to `0.012ms` GPU;
- surface total for the 8-chunk rebuild: `14.74ms`.

## Phase 3: sparse surface generation from active solid bricks

Not implemented in this pass.

This remains the biggest potential shader-side win, but it requires more source data than the current surface scratch path provides.

Problem: `make_surface.comp` still scans all `256^3` voxels to discover exposed voxels. Sparse chunks with only a small amount of geometry still pay the full scan.

Plan options:

1. Maintain a per-chunk solid-brick list when writing to the voxel atlas.
2. Dispatch `make_surface` over active solid bricks plus a one-brick halo for boundary exposure.
3. Preserve correctness at chunk boundaries by including neighbor chunk halo reads.
4. Feed the resulting active surface brick/voxel lists into contree and flora.

This likely requires changes in voxel write paths, not just surface shaders.

## Phase 4: pipeline or batch surface work

Not implemented in this pass.

Current direct multi-chunk rebuilds call `surface_builder.build_surface`, which submits and waits per chunk. Contree waits are pipelined, but surface is still serial from the CPU point of view.

Possible approaches:

- Add a small ring of per-job surface scratch resources so multiple surface builds can be submitted before readback.
- Or record surface + direct flora + contree + scene update for a chunk into a larger batch where CPU readbacks are minimized.

Constraints:

- Current surface resources are scratch resources shared by all chunks.
- Contree consumes the current surface scratch image and active lists.
- Empty-chunk decisions currently depend on CPU readback of `active_voxel_len`.

## Phase 5: make-surface shader micro-optimizations

Partially implemented in commit `45513b94`.

Change:

- Replaced `length(normal) > EPSILON` plus `normalize(-normal)` with a squared-length check and `inversesqrt`, avoiding duplicate square-root work for valid normals.

Evidence:

- This was safe and validated, but the measured performance impact is small relative to run-to-run variance.
- Final 8-chunk run after this change: `17.76ms` total, `14.74ms` surface.

Other Phase 5 experiments tried and not kept:

- Active-brick epoch/tag in place of clearing `surface_active_brick_flags`: worsened the 8-chunk run to `23.73ms` total / `21.23ms` surface in `target/re-flora-logs/re-flora-20260521-165805.255-58795.log`, with `make_surface` roughly doubling on large chunks. Reverted.
- Fence wait instead of `queue_wait_idle` for the separate flora command: did not improve the measured 8-chunk run (`18.57ms` total / `15.03ms` surface in `target/re-flora-logs/re-flora-20260521-170145.888-66354.log`). Reverted.

## Remaining recommendation

The next meaningful optimization should be Phase 3 or Phase 4, not more small local shader tweaks:

1. Phase 3 if voxel write paths can cheaply provide active solid brick lists.
2. Phase 4 if we want to reduce per-chunk submit/wait serialization by adding per-job surface scratch resources or indirect in-command flora dispatch.

The current normal flora rebuild bottleneck has been mostly removed; `make_surface.comp` and per-chunk synchronization are now the remaining surface costs.
