# Flora Single Instance Buffer Progress

## Goal

Replace the current per-flora-species manual flora instance buffers with one fixed-range flora instance buffer per surface chunk.

Done means:

- Each surface chunk owns one manual flora instance buffer instead of one buffer per flora species.
- Generation, edit preservation, trimming/regeneration, growth updates, and rendering read/write the correct per-species ranges.
- Flora behavior and rendering are preserved.
- `cargo fmt --check`, `cargo check`, `cargo test`, and a release hidden run pass with clean logs.

## Current State

Known layout and constraints:

- Branch: `flora-single-instance-buffer`.
- Flora species count is currently 4: `tall_grass`, `short_grass`, `lavender`, `ember_bloom`.
- Rust species source: `src/flora/species.rs`.
- GLSL species source: `shader/include/flora_registry.glsl`.
- Manual flora instance payload is still one packed `u32` from `shader/include/instance.glsl`:
  - bits 0..7: local x
  - bits 8..15: local y
  - bits 16..23: local z
  - bits 24..31: growth progress
- Per-species capacity remains `MAX_FLORA_INSTANCES_PER_SPECIES = 40_000`.
- Implemented fixed layout:
  - species `i` starts at `i * MAX_FLORA_INSTANCES_PER_SPECIES`
  - per-species lengths are tracked on the Rust side
- Current world chunk grid is `5 x 2 x 5 = 50` chunks.
- Resource count target is achieved logically: manual flora instance buffers go from 200 to 50 for the current world size.
- Reserved payload memory is intentionally unchanged for this phase: about 30.5 MiB. Compact shared capacity is a separate follow-up.

Relevant files:

- Resource allocation: `src/builder/surface/resources.rs`
- Surface build/edit/growth orchestration: `src/builder/surface/mod.rs`
- Flora rendering: `src/tracer/mod.rs`
- Generation shaders:
  - `shader/builder/surface/active_surface_to_flora_instances.comp`
  - `shader/builder/surface/occupancy_to_flora_instances.comp`
- Edit/growth shaders:
  - `shader/builder/surface/instances_to_occupancy.comp`
  - `shader/builder/surface/update_flora_growth.comp`
- Registry/constants: `shader/include/flora_registry.glsl`

## Plan / Phases

### Phase 1: Fixed-range single-buffer design

- Objective: Define the one-buffer-per-chunk fixed species range layout.
- Expected output: species offsets are deterministic and no compaction/prefix-sum is required.
- Dependencies/blockers: none.
- Status: done.

### Phase 2: Rust resource structure update

- Objective: Replace per-species `Vec<InstanceResource>` with one `InstanceResource` plus per-species length helpers.
- Expected output: one flora instance buffer allocated per chunk and one descriptor write per chunk/pipeline binding.
- Dependencies/blockers: Phase 1.
- Status: done.

### Phase 3: Compute shader update

- Objective: Make generation, edit, and growth shaders address one buffer with species offsets.
- Expected output: shader buffer declaration is a single `flora_instances`; reads/writes use `flora_species_instance_offset(species_idx) + idx`.
- Dependencies/blockers: Phase 2.
- Status: done.

### Phase 4: Render path update

- Objective: Render each species from the shared chunk buffer.
- Expected output: render loop binds the shared chunk buffer, uses species length for `instance_count`, and uses species offset as Vulkan `firstInstance`.
- Dependencies/blockers: Phase 3.
- Status: done.

### Phase 5: Validation

- Objective: Prove build/runtime correctness.
- Expected output: formatting, check, tests, hidden release run, and latest-log inspection pass.
- Dependencies/blockers: Phases 2-4.
- Status: done.

### Phase 6: Optional compact shared-capacity buffer

- Objective: Reduce reserved payload memory by compacting all species into shared per-chunk capacity.
- Expected output: prefix/compaction scheme and smaller per-chunk allocation, potentially about 30.5 MiB to about 7.6 MiB if total capacity becomes 40,000 instances per chunk.
- Dependencies/blockers: fixed-range version must remain correct and be benchmarked first.
- Status: not started.

## Verification Method

Commands run:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Results:

- `cargo fmt --check`: passed.
- `cargo check`: passed.
- `cargo test`: passed, 80 tests.
- Hidden release run: passed, application exited successfully.
- Latest log inspection: no errors found; only existing butterfly-atlas warning appeared.

Acceptance criteria:

- No shader compilation or descriptor layout errors.
- No Vulkan/runtime errors in hidden-run log.
- Single-buffer fixed species ranges are used in resource allocation, compute writes/reads, and render draws.
- Manual visual spot-check is still useful for confirming all species appearances, but headless validation passed.

## Progress Log

- 2026-06-03: Inspected flora instance resource layout.
  - Found current model: per chunk, `FloraInstanceResources` stored a `Vec<InstanceResource>`, one buffer per species.
  - Found instance payload is one packed `u32`.
  - Found compute shaders bound flora buffers as descriptor array `flora_instances[MAX_FLORA_SPECIES]`.
- 2026-06-03: Decided initial implementation should use fixed per-species ranges in a single buffer.
  - Reason: low-risk migration that reduces buffer/descriptor count without introducing compaction complexity.
- 2026-06-03: Created branch `flora-single-instance-buffer`.
- 2026-06-03: Created this progress document.
- 2026-06-03: Implemented fixed-range single-buffer flora resources.
  - Rust tracks one chunk flora buffer plus per-species lengths.
  - Compute shaders use `flora_species_instance_offset` for reads/writes.
  - Renderer uses `firstInstance` as the species range offset.
- 2026-06-03: Verified with formatting, check, tests, hidden run, and latest-log inspection.

## Open Questions / Risks

- Render correctness depends on Vulkan `gl_InstanceIndex` including `firstInstance`; hidden release validation succeeded, but manual visual confirmation is still recommended.
- Fixed per-species ranges avoid write collisions but do not reduce reserved memory. Compact shared-capacity requires additional GPU-side count/prefix/compaction work.
- If any species exceeds `MAX_FLORA_INSTANCES_PER_SPECIES`, fixed-range layout can overflow just like the old per-species buffers could; bounds diagnostics may be a future improvement.
- Prior perf note warned that a 4-byte flora instance vertex stride was slower than 8-byte stride on tested hardware. This task did not change the instance payload/stride.
