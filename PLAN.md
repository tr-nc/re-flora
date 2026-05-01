# Model Voxelization Robustness Plan

## Problem

The current model voxelizer classifies each voxel center with a single +X ray and odd/even triangle intersection count. This is fragile when the ray hits triangle edges, vertices, shared edges, or near-parallel triangles. The result is occasional incorrect inside/outside classification.

Keep the existing near-surface distance fill as a shell fallback in both approaches.

## Approach 1: Winding Number Classification

Replace single-ray parity in `shader/builder/chunk_writer/model_voxelize.comp` with signed solid-angle accumulation.

For each voxel center:

1. Iterate all model triangles.
2. Compute each triangle's signed solid angle as seen from the voxel center.
3. Sum all solid angles.
4. Classify as inside when `abs(total_solid_angle) > 2 * PI`.
5. Fill when `inside || near_surface`.

Expected behavior:

- More robust than ray parity for edge/vertex degeneracies.
- Does not depend on choosing a lucky ray direction.
- Best for closed meshes with consistent triangle winding.
- Can still fail on open meshes, heavily self-intersecting meshes, or inconsistent winding.

Runtime cost:

- Same big-O as current shader: `voxel_count * triangle_count`.
- Higher per-triangle cost due to vector lengths and `atan`.
- Expected one-time voxelization cost increase: roughly `3x-10x`, depending on GPU math throughput and triangle count.
- No steady-state frame cost after voxelization completes.

Implementation notes:

- Add a `signed_solid_angle()` helper in the compute shader.
- Guard degenerate triangles and near-zero denominator cases to avoid NaNs.
- Keep `sqr_distance_point_triangle()` unchanged for the surface shell.
- Run `cargo check` after shader edits so generated Rust structs stay current.

## Approach 2: Surface Voxelization Plus Exterior Flood Fill

Stop relying on point-in-mesh tests for every voxel. Instead, build occupancy in multiple phases.

Phase outline:

1. Mark surface voxels by triangle distance/coverage within the model bounds.
2. Allocate temporary state for the model bounds, or reuse an available scratch representation if suitable.
3. Flood fill from the outside of the model bounds through empty voxels.
4. Classify any unvisited empty voxel as interior.
5. Write both surface and interior voxels to `chunk_atlas`.

Expected behavior:

- More robust for imperfect assets than parity or winding, especially when classification artifacts are caused by ray degeneracy.
- Handles inconsistent triangle winding better because it does not use signed orientation.
- Still depends on a sufficiently closed surface shell; real holes can let the exterior flood into the interior unless the surface thickness seals them.

Runtime cost:

- More implementation complexity and more compute passes.
- Work is closer to `surface_marking + bounds_voxel_count`, rather than `bounds_voxel_count * triangle_count` for every classification pass.
- Likely scales better than winding number for larger or higher-triangle models once implemented well.
- Requires temporary GPU memory proportional to the model bounds.

Implementation notes:

- Start with a bounded local volume covering only the model AABB, not the full world atlas.
- Keep the existing model bounds calculation in `src/builder/plain/mod.rs`.
- Add explicit benchmark instrumentation around each phase before optimizing.
- Prefer this approach if winding number fixes numerical issues but remains too slow or fails on dirty meshes.

## Benchmark Method

Use the same asset, placement, and camera path for every run.

Baseline setup:

1. Build/check after every shader or Rust change: `cargo check`.
2. Run model loader tests when model code changes: `cargo test model::tests`.
3. Use release mode for performance comparisons.

Primary smoke/perf command:

```bash
cargo run --release -- --windowed --auto-exit 20 --perf
```

Do not pass `--present-mode` by default. The app auto-selects the best supported mode.

Metrics to record:

- Total startup-to-first-render time if visible in logs.
- Existing `[PERF]` frame timing after loading.
- Dedicated BENCH timing for model voxelization dispatch, if present.
- If not present, add BENCH records around `PlainBuilder::voxelize_model()` and any new compute phases.
- Visual correctness screenshots or notes for known-problem voxels.

Benchmark protocol:

1. Run the current implementation three times and record median timings.
2. Implement Approach 1 and run three times with the same command.
3. Compare voxelization timing, total loading time, and visual artifacts.
4. Only implement Approach 2 if Approach 1 is still wrong or too slow.
5. If Approach 2 is implemented, benchmark each phase separately and compare total loading time against Approach 1.

Decision criteria:

- Prefer the smallest implementation that removes visible misclassified voxels.
- Accept higher one-time loading cost if steady-state frame timings are unchanged and the delay is not noticeable.
- Move to flood fill if winding number is too slow for model placement or cannot handle the asset topology.
