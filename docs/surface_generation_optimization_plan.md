# Surface generation optimization plan

Branch context: `agent/contree-optimizations` after contree Phases 1-4.

## Why this is worth doing

After contree optimization, the tested 8-chunk tree rebuild is dominated by surface generation.

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

Conclusion: yes, surface optimization is possible and likely worthwhile. The current surface path still does full-volume work over `256^3` chunks and then runs flora generation as additional full-volume passes.

## Current algorithm summary

`shader/builder/surface/make_surface.comp`:

1. Dispatches over the whole chunk volume.
2. Preloads an `8x8x8` workgroup plus halo into shared memory.
3. Skips empty voxels.
4. Skips fully occluded solid voxels.
5. Computes a normal from a `5x5x5` neighborhood for exposed voxels.
6. Writes packed surface data to a scratch `surface` image.
7. Emits active surface brick metadata used by the optimized contree path.

`SurfaceBuilder::finish_build_surface` optionally calls `seed_and_rebuild_flora_from_surface`, which currently:

1. clears `occupancy_data`;
2. scans the whole chunk with `edit_occupancy_sphere.comp` to mark plantable stems;
3. scans the whole chunk with `occupancy_to_flora_instances.comp` to emit instances;
4. waits with `queue_wait_idle` before reading instance counts.

## Phase 0: add surface/flora pass timing

Before shader rewrites, add timestamp instrumentation equivalent to contree pass timing.

Measure separately:

- surface image clear;
- redundant/required occupancy clear work;
- `make_surface.comp`;
- flora `clear_occupancy.comp`;
- flora `edit_occupancy_sphere.comp`;
- flora `occupancy_to_flora_instances.comp`;
- flora result readback / wait.

Goal: split the current `fence_latency` and `flora` buckets into actual GPU pass costs.

Validation:

```bash
cargo fmt --check
cargo check
cargo test
source ~/.zshrc
cargo run --release -- --hidden --auto-exit 0.5
```

## Phase 1: remove obviously redundant work

Candidate: `submit_build_surface` clears `occupancy_data` before `make_surface`, but `make_surface` does not read `occupancy_data`. Flora rebuild paths clear or rebuild occupancy again before using it.

Plan:

- Remove the unconditional `occupancy_data` clear from `SurfaceBuilder::submit_build_surface` if validation confirms no dependency.
- Keep the explicit occupancy clear inside flora/edit paths.
- Measure before/after with surface pass timestamps.

Expected risk: low, but verify preserve-flora and edit paths because they share the same scratch texture.

## Phase 2: build flora directly from active surface voxels

This is the most promising near-term optimization.

Current flora generation does full-volume passes even though `make_surface` already knows exactly which voxels are exposed surface voxels.

Plan:

1. Extend `make_surface.comp` to write `surface_active_voxel_indices` using the existing `active_voxel_len` atomic.
2. Add a new 1D flora shader that dispatches over active surface voxels only.
3. For each active surface voxel:
   - load packed surface data;
   - check plantability and normal;
   - run the existing density/biome selection logic;
   - emit at most one flora instance.
4. Replace the startup/tree-add `seed_and_rebuild_flora_from_surface` full-volume path with this direct active-surface path.
5. Keep the existing occupancy-based path for preserve-flora edits, trimming, growth, and instance-to-occupancy flows until those are separately optimized.

Expected win source:

- Current measured `flora` time is often `~1.0ms` to `1.47ms` per rebuilt chunk.
- Direct active-surface dispatch avoids at least two full `256^3` scans and one occupancy clear for normal flora rebuilds.

Correctness checks:

- Flora counts and species distribution should remain close enough to the old deterministic placement rules.
- Mature/growing state semantics must remain valid for paths that use growth.
- Preserve-flora edit behavior must not regress.

## Phase 3: sparse surface generation from active solid bricks

This is the biggest potential shader-side win, but it needs more source data.

Problem: `make_surface.comp` still scans all `256^3` voxels to discover exposed voxels. Sparse chunks with only a small amount of geometry still pay the full scan.

Plan options:

1. Maintain a per-chunk solid-brick list when writing to the voxel atlas.
2. Dispatch `make_surface` over active solid bricks plus a one-brick halo for boundary exposure.
3. Preserve correctness at chunk boundaries by including neighbor chunk halo reads.
4. Feed the resulting active surface brick/voxel lists into contree and flora.

This likely requires changes in voxel write paths, not just surface shaders. It is higher risk but can reduce the core surface `fence_latency` bucket.

## Phase 4: pipeline or batch surface work

Current direct multi-chunk rebuilds call `surface_builder.build_surface`, which submits and waits per chunk. Contree waits are pipelined, but surface is still serial from the CPU point of view.

Possible approaches:

- Add a small ring of per-job surface scratch resources so multiple surface builds can be submitted before readback.
- Or record surface + contree + scene update for a chunk into a larger batch where CPU readbacks are minimized.

Constraints:

- Current surface resources are scratch resources shared by all chunks.
- Contree consumes the current surface scratch image and active lists.
- Empty-chunk decisions currently depend on CPU readback of `active_voxel_len`.

This phase should follow Phase 0 timing and Phase 2 flora simplification.

## Phase 5: make-surface shader micro-optimizations

Only do these after timestamp evidence identifies `make_surface.comp` itself as the limiting pass.

Candidates:

- Replace `length(normal) > EPSILON` with squared-length checks before normalization.
- Benchmark smaller or alternative normal kernels if visual quality is acceptable.
- Revisit workgroup size (`8^3`) and shared-memory halo layout on Apple/MoltenVK.
- Evaluate whether surface clear can be replaced by shader writes or an epoch/tag scheme.
- Reduce atomics if active-brick/active-voxel output becomes a bottleneck.

## Recommended order

1. Phase 0 timing.
2. Phase 1 redundant clear removal.
3. Phase 2 direct flora from active surface voxels.
4. Re-benchmark 8-chunk rebuild.
5. Decide whether Phase 3 sparse surface generation is still necessary.

The most likely first meaningful win is Phase 2 because current logs show flora rebuild alone costs about `1ms+` per chunk and it is doing full-volume work that can be replaced by the active surface list.
