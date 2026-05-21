# Surface generation optimization plan and results

Branch context: `agent/contree-optimizations` after contree Phases 1-4.

## Status summary

Implemented and validated:

- Phase 0: surface/flora GPU timestamp instrumentation.
- Phase 1: removed redundant GPU clears from the normal surface path.
- Phase 2: replaced normal flora rebuild with a direct active-surface-brick shader.
- Phase 3: sparse surface generation from GPU solid `8^3` workgroup flags.
- Phase 4 subset: folded normal direct flora rebuild into the surface command buffer using an indirect dispatch prepared on GPU.
- Phase 5 subset: replaced duplicate normal-length square-root work with a squared-length check.

Final measured Phase 3 run: `target/re-flora-logs/re-flora-20260521-174411.058-38643.log`.

| Metric, 8 chunk tree rebuild | Baseline after contree (`160303`) | After Phase 2/5 (`170231`) | After Phase 4 (`171913`) | After Phase 3 sparse (`174411`) |
| --- | ---: | ---: | ---: | ---: |
| Total rebuild | `26.36ms` | `17.76ms` | `15.16ms` | `13.67ms` |
| Surface total | `23.41ms` | `14.74ms` | `12.52ms` | `10.16ms` |
| Contree total | `1.57ms` | `1.50ms` | `1.33ms` | `1.63ms` |
| Scene texture total | `1.27ms` | `1.37ms` | `1.23ms` | `1.76ms` |

The largest single win remains Phase 2: normal flora rebuild no longer performs full-volume occupancy passes. Phase 4 then removes the extra per-chunk flora submit/wait by recording `make_surface -> prepare_flora_dispatch -> active_surface_to_flora` in one command buffer. Phase 3 further reduces the remaining surface shader work by dispatching surface generation only for solid `8^3` workgroups instead of the whole `256^3` chunk.

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

`shader/builder/surface/prepare_sparse_surface_dispatch.comp` scans the chunk's solid `8^3` workgroup flags and writes:

1. `surface_solid_workgroup_indices`: a compact list of solid local workgroups;
2. `surface_solid_workgroup_dispatch_indirect`: an indirect dispatch whose `x` count is the compact list length;
3. `make_surface_result.solid_workgroup_len`: a debug/readback count for logs and bounds checks.

`shader/builder/surface/make_surface_sparse.comp` then runs only for that compact workgroup list:

1. maps `gl_WorkGroupID.x` through `surface_solid_workgroup_indices`;
2. preloads the selected `8x8x8` block plus halo into shared memory;
3. skips empty voxels and fully occluded solid voxels;
4. computes a normal from a `5x5x5` neighborhood for exposed voxels;
5. writes packed surface data to the scratch `surface` image;
6. emits active surface brick metadata used by the optimized contree and flora paths.

For normal non-preserve-flora rebuilds, `SurfaceBuilder::submit_build_surface` records direct flora generation into the same command buffer as sparse surface generation:

1. `make_surface_sparse` writes the active surface brick list and `active_brick_len`;
2. `prepare_active_surface_flora_dispatch.comp` writes an indirect dispatch size from `active_brick_len`;
3. `active_surface_to_flora_instances.comp` scans only the 64 voxels inside each active surface brick;
4. the flora shader loads packed surface data directly;
5. it checks plantability/density/biome and writes flora instances directly.

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

Run evidence before Phase 4 from `target/re-flora-logs/re-flora-20260521-170231.671-68224.log`:

- large active chunks: `active_surface_to_flora=0.071ms` to `0.083ms` GPU;
- small active chunks: `active_surface_to_flora=0.011ms` to `0.012ms` GPU;
- surface total for the 8-chunk rebuild: `14.74ms`.

## Phase 3: sparse surface generation from active solid bricks

Implemented after the Phase 4 subset.

Changes:

1. Added `shader/include/solid_workgroups.glsl` for global solid `8^3` workgroup indexing and marking.
2. Added `solid_workgroup_flags`, a GPU bitset with one bit per `8^3` atlas workgroup. It is cleared together with the atlas.
3. Marked solid workgroups in voxel write paths:
   - `shader/builder/chunk_writer/chunk_init.comp`;
   - `shader/builder/chunk_writer/chunk_modify.comp`;
   - `shader/builder/chunk_writer/model_voxelize.comp`.
4. Added `shader/builder/surface/prepare_sparse_surface_dispatch.comp` to scan the 32,768 possible workgroups in a `256^3` chunk and prepare a compact indirect dispatch.
5. Added `shader/builder/surface/make_surface_sparse.comp`, which preserves the old surface logic but maps dispatch workgroups through the compact solid-workgroup list.
6. Added readback/logging for `solid_workgroup_len`.

Correctness notes:

- The flags are conservative. Removing solids does not immediately clear individual workgroup bits, so stale bits can only cause extra sparse work; `make_surface_sparse` still reads the real atlas and the surface image is cleared before each rebuild.
- Chunk-boundary reads keep the previous `is_crossing_boundary` behavior through `load_atlas`.

Run evidence from `target/re-flora-logs/re-flora-20260521-174411.058-38643.log`:

- 8-chunk rebuild total: `13.67ms`.
- Surface total: `10.16ms`.
- The large lower chunks dispatched `17,720` to `21,382` solid workgroups and `make_surface_sparse` took `0.882ms` to `1.068ms`.
- The sparse upper chunks dispatched only `61` to `95` solid workgroups and `make_surface_sparse` took `0.012ms` to `0.014ms`.

Compared with the Phase 4 run (`target/re-flora-logs/re-flora-20260521-171913.113-90526.log`), where the same upper chunks still ran full-volume `make_surface` at about `0.671ms` to `0.676ms`, sparse dispatch removes nearly all surface-shader cost for mostly empty chunks.

## Phase 4: pipeline or batch surface work

Partially implemented in this pass.

Implemented subset:

- Added `shader/builder/surface/prepare_active_surface_flora_dispatch.comp`.
- Added a GPU-only indirect dispatch buffer for active-surface flora generation.
- Changed the direct flora shader to read `active_brick_len` from `make_surface_result` instead of a CPU push constant.
- Recorded direct flora generation in the same command buffer as `make_surface`, with compute barriers and `record_indirect`.
- Removed the separate direct-flora command submission and queue idle wait from normal surface builds.

Measured result in `target/re-flora-logs/re-flora-20260521-171913.113-90526.log`:

- 8-chunk rebuild total: `15.16ms`
- Surface total: `12.52ms`
- Contree total: `1.33ms`
- Scene texture total: `1.23ms`
- Surface pass timing now includes `prepare_flora_dispatch` at about `0.002ms` and `active_surface_to_flora` at `0.006ms` to `0.075ms` for the tested 8 chunks.

Remaining Phase 4 work, if needed:

- Add a small ring of per-job surface scratch resources so multiple surface builds can be submitted before readback.
- This is more invasive because current contree pipelines are built against the single `SurfaceResources` scratch image/buffer set, and empty-chunk decisions still depend on CPU readback of `active_voxel_len`.

## Phase 5: make-surface shader micro-optimizations

Partially implemented in commit `45513b94`.

Change:

- Replaced `length(normal) > EPSILON` plus `normalize(-normal)` with a squared-length check and `inversesqrt`, avoiding duplicate square-root work for valid normals.

Evidence:

- This was safe and validated, but the measured performance impact is small relative to run-to-run variance.
- 8-chunk run after this change and before Phase 4: `17.76ms` total, `14.74ms` surface.

Other Phase 5 experiments tried and not kept:

- Active-brick epoch/tag in place of clearing `surface_active_brick_flags`: worsened the 8-chunk run to `23.73ms` total / `21.23ms` surface in `target/re-flora-logs/re-flora-20260521-165805.255-58795.log`, with `make_surface` roughly doubling on large chunks. Reverted.
- Fence wait instead of `queue_wait_idle` for the separate flora command: did not improve the measured 8-chunk run (`18.57ms` total / `15.03ms` surface in `target/re-flora-logs/re-flora-20260521-170145.888-66354.log`). Reverted.

## Remaining recommendation

Phase 3 is implemented. The next meaningful optimization is full Phase 4 batching and/or reducing the remaining per-chunk clears:

1. Full Phase 4 batching would reduce per-chunk surface submit/wait serialization by adding per-job surface scratch resources and making contree consume those resources safely.
2. `surface_active_brick_flags` clear is now a large fixed cost on sparse chunks (`~0.28ms` to `0.34ms` per chunk in the final run), but the attempted epoch/tag replacement regressed, so any new approach needs measurement.
3. If edit-heavy workloads become important, add a more precise solid-workgroup update/clear path; the current flags are conservative and can accumulate stale set bits after removals.

The normal flora rebuild bottleneck and the full-volume surface scan are now mostly removed for sparse chunks. The remaining costs are fixed clears, per-chunk synchronization, and genuinely dense surface work.
