# Terrain Smoothing GPU Progress

## Goal

Build a fast, genuinely 3D terrain smoothing tool for binary/material voxel terrain.

Done means:

- Smoothing remains volumetric, not heightmap-only.
- The common interaction path avoids full GPU-to-CPU voxel readback and CPU-side full-volume sorting.
- Default and large brush sizes are measured in release-mode app runs and are responsive enough for held mouse input.
- Visual behavior is acceptable for terrain sculpting: local roughness is reduced without obvious volume loss, material corruption, or unrelated terrain changes.
- The implementation is validated by automated checks plus at least one manual/visual tool pass.

## Current State

- Work branch/worktree:
  - Branch: `agent/gpu-mbo-smoothing`
  - Worktree: `/home/terence/code/re-flora`
  - Previous smoothing worker worktree `/home/terence/code/re-flora-agent-smoothing` has been merged and removed.
  - `main` was fast-forwarded to include the prior CPU smoothing optimization and progress doc before this branch was created.
- Current implemented work:
  - CPU-side 3D smoother remains as a fallback for oversized GPU sample volumes.
  - GPU MBO first pass is implemented: init density, ping-pong 3D diffusion, score/histogram, CPU threshold from small histogram, GPU apply, and small result readback.
  - Volume preservation is approximate at the threshold tie bin using deterministic hash tie-breaking; exact prefix-sum tie selection is not implemented yet.
- Important files:
  - `src/builder/plain/mod.rs` — Smooth tool entry point, GPU MBO orchestration, CPU fallback, threshold helper, voxel atlas helpers.
  - `src/builder/plain/resources.rs` — smoothing GPU buffers/resources.
  - `shader/builder/chunk_writer/terrain_smooth_heights.comp`
  - `shader/builder/chunk_writer/terrain_smooth_target.comp`
  - `shader/builder/chunk_writer/terrain_smooth_apply.comp`
  - `shader/builder/chunk_writer/terrain_smooth_mbo_*.comp`
  - `shader/builder/chunk_writer/terrain_smooth_mbo_common.glsl`
  - `src/app/core/input.rs` — Smooth tool input path.
  - `src/app/core/vegetation.rs` — tool call into `PlainBuilder::smooth_terrain_dirt` and rebuild scheduling.
- Known performance issue addressed in current prototype:
  - The default Smooth path no longer reads the full 3D voxel region back to CPU.
  - CPU now reads only a small histogram/result buffer to choose the threshold and return changed bounds.
  - The CPU fallback still exists for sample volumes above the fixed GPU work-buffer capacity.
- Research direction:
  - Best near-term fit appears to be GPU volume-preserving MBO / threshold-dynamics smoothing: parallel 3D density blur, GPU histogram/prefix threshold selection, then parallel threshold apply.
  - Longer-term industry-standard smooth voxel terrain often uses continuous SDF/density fields plus Surface Nets, Dual Contouring, Marching Cubes, or Transvoxel; that is a larger terrain representation shift.
- Assumptions to confirm:
  - Binary/material voxel atlas remains the source of truth for now.
  - Heightfield-only smoothing is insufficient; tool should work on real 3D surfaces.
  - Approximate volume preservation is desirable; exact preservation may be optional if visual behavior is better and faster.
  - Material fill policy can initially reuse local majority/material-neighbor rules.

## Plan / Phases

### Phase 1 — Baseline and decision record

- Objective: Document current behavior, bottlenecks, and chosen algorithm direction.
- Expected output: Progress doc plus brief evidence from code inspection, synthetic evaluation, and prior hidden app runs.
- Dependencies/blockers: None.
- Status: done.

### Phase 2 — Deterministic benchmark harness

- Objective: Make smoothing performance and effect measurable without manual mouse input.
- Expected output: CLI/dev-only scenario or ignored test that triggers smoothing at fixed terrain positions/radii and logs changed voxels, timing phases, and roughness/volume metrics.
- Dependencies/blockers: Need a safe deterministic entry point that runs in the real app/render path or a faithful builder-level harness.
- Status: not started.

### Phase 3 — GPU MBO prototype

- Objective: Prototype a 3D GPU path without full voxel readback.
- Expected output: Compute shaders/buffers for brush-region occupancy extraction, narrow-band or brush-AABB density blur, changed-count/bounds stats, and threshold apply.
- Dependencies/blockers: Need buffer/image layout design and dispatch orchestration in `PlainBuilder`.
- Status: in progress.

### Phase 4 — Volume-preserving threshold selection

- Objective: Replace CPU sort with a GPU-friendly threshold method.
- Expected output: Histogram/prefix-sum or quantized score threshold pass that preserves the original solid count closely enough for sculpting.
- Dependencies/blockers: Requires Phase 3 score/density output; exact preservation may require tie-breaking or second pass.
- Status: in progress.

### Phase 5 — Tool integration and fallback

- Objective: Route the Smooth tool to the GPU path while preserving a safe fallback/debug option.
- Expected output: `apply_surface_terrain_smooth` uses GPU smoother by default; CPU smoother can remain temporarily for comparison.
- Dependencies/blockers: Phases 3–4 must produce stable changed bounds and material updates.
- Status: done for first prototype; further tuning remains.

### Phase 6 — Quality/performance tuning

- Objective: Tune smoothing radius, iteration count, falloff, volume preservation tolerance, and material fill behavior.
- Expected output: Release-mode benchmarks and visual/manual checks for default, medium, and max brush radii.
- Dependencies/blockers: Benchmark harness and integrated GPU path.
- Status: not started.

### Phase 7 — Cleanup and documentation

- Objective: Remove obsolete CPU-only paths if no longer needed and document the final algorithm.
- Expected output: Updated docs/comments, concise algorithm notes, and clean validation logs.
- Dependencies/blockers: Final tool behavior accepted.
- Status: not started.

## Verification Method

Current checks already used on the branch:

- `cargo fmt --check`
- `cargo check`
- `cargo test`
- `cargo run --release -- --hidden --mute --auto-exit 0.5`
- `cargo run --release -- --tail-latest-log 160`
- `cargo build`
- `cargo build --release`

Additional verification needed before GPU smoother is considered done:

- Deterministic smoothing benchmark/harness:
  - Compare CPU baseline vs GPU path at default, medium, and max radius.
  - Log per-phase timings, changed voxel count, changed bounds, candidate/band sizes if available, and total frame/tool cost.
- Correctness/quality criteria:
  - No GPU validation/runtime errors in hidden release runs.
  - Smoothing modifies only voxels inside intended brush/band bounds.
  - Solid volume is preserved exactly or within an agreed tolerance.
  - Roughness/exposed-surface metric improves on representative synthetic and real terrain cases.
  - Material types remain valid terrain/empty types.
- Manual validation:
  - Visible `cargo run` from `/home/terence/code/re-flora` on `agent/gpu-mbo-smoothing`, only when requested.
  - Try held smoothing strokes on rough terrain, slope, cliff/overhang, and large radius.

Verification gap:

- There is no current automated app path that triggers the Smooth tool in a release hidden run, so performance evidence for the real tool path is incomplete until Phase 2 or a manual visible run.

## Progress Log

- Created dedicated branch/worktree for smoothing work: `agent/smoothing-tool-optimization` at `/home/terence/code/re-flora-agent-smoothing`.
- Moved the existing uncommitted smoothing changes off `main`; main worktree remains clean.
- Inspected current CPU 3D smoothing implementation in `src/builder/plain/mod.rs` and related tool call sites.
- Validated the current branch with `cargo check` and a hidden muted release run.
- Added and committed a small CPU-path optimization in `130e0f2 optimize terrain smoothing pass`.
- Researched GPU-friendly 3D smoothing approaches:
  - MBO / threshold dynamics for mean-curvature-like motion.
  - Volume-preserving threshold dynamics / histogram-threshold variants.
  - SDF/density-field terrain with Surface Nets, Dual Contouring, Marching Cubes, or Transvoxel.
  - Allen-Cahn/phase-field smoothing as another stencil-friendly but more parameter-heavy option.
- Decision so far: prefer a GPU volume-preserving MBO/threshold-dynamics path as the next implementation direction because it preserves the current binary/material voxel representation while removing CPU readback and full sort from the interaction path.
- Created this progress document: `docs/terrain_smoothing_gpu_progress.md`.
- Fast-forward merged `agent/smoothing-tool-optimization` into `main`, then removed `/home/terence/code/re-flora-agent-smoothing`.
- Created `agent/gpu-mbo-smoothing` from the merged main worktree.
- Implemented first GPU MBO prototype:
  - Added MBO init/diffuse/score/apply compute shaders and shared GLSL helpers.
  - Added fixed-capacity GPU work buffers for density ping-pong, scores, histogram, and result readback.
  - Routed `smooth_terrain_dirt` through GPU MBO by default, with CPU fallback when the brush sample volume exceeds GPU buffer capacity.
  - Added threshold-selection unit tests.
- Validated compilation, unit tests, shader/pipeline initialization, and hidden release startup.

## Open Questions / Risks

- Exact vs approximate volume preservation: current prototype uses approximate deterministic hash tie-breaking at the threshold bin; exact preservation would need prefix-sum or compaction.
- Active region selection: current prototype uses the brush AABB/sphere rather than a BFS-connected surface band; behavior on nearby disconnected surfaces needs manual validation.
- Material assignment: adding solid voxels needs robust material choice near mixed dirt/sand/rock boundaries.
- Topology behavior: MBO can close small holes or detach small components; need to decide whether this is desirable for a sculpting tool.
- Mesh/rebuild cost may become the next bottleneck after smoothing moves to GPU.
- Existing heightfield smoothing shaders remain unused legacy/alternative path; the real 3D path now has separate MBO compute passes.
- The project currently lacks a deterministic Smooth-tool benchmark, so performance comparisons are not yet authoritative.
