# GPU Resource Refactor Progress

## Goal

Improve maintainability and readability of GPU resource ownership/binding without replacing typed resources with a single global dynamic map.

Done means:

- `TracerResources` and related resource structs are grouped by clear domain and lifecycle.
- Existing shader descriptor auto-binding continues to work by field name.
- Pipeline setup/update code is no more verbose than today and ideally clearer.
- Runtime behavior is unchanged, validated by normal checks and a hidden release run.

## Current State

Known:

- Branch: `agent/resource-containers`.
- Current descriptor auto-binding uses `ResourceContainer` plus `#[derive(ResourceContainer)]`.
- `Resource<T>` wraps `Buffer`/`Texture`; field names are matched against reflected GLSL descriptor names.
- `TracerResources` is the main maintainability issue: it mixes uniforms, static textures, shadow/VSM resources, wind, terrain query, flora mesh data, extent-dependent targets, and denoiser resources.
- Existing subgrouping already exists for `ExtentDependentResources` and `DenoiserResources`.
- Builder resources are comparatively scoped:
  - `src/builder/plain/resources.rs`
  - `src/builder/surface/resources.rs`
  - `src/builder/contree/resources.rs`
  - `src/builder/scene_accel/resources.rs`

Relevant files:

- `crates/verdarium-vkn/src/resource.rs`
- `resource_container_derive/src/lib.rs`
- `crates/verdarium-vkn/src/pipeline/descriptor_set_utils.rs`
- `src/tracer/resources.rs`
- `src/tracer/extent_dependent_resources.rs`
- `src/tracer/denoiser_resources.rs`
- `src/tracer/pipeline_builder.rs`
- `src/tracer/mod.rs`

Constraints:

- Keep changes small and focused.
- Do not hand-edit generated files.
- Run `cargo check` after Rust/shader changes.
- For final validation, use hidden release run and inspect latest log.
- Avoid one global `HashMap<String, Buffer/Texture>` owner unless evidence shows typed grouping cannot work.

Assumptions to confirm:

- Shader descriptor names should continue matching Rust field names.
- Nested typed resource containers are acceptable as the primary organization mechanism.
- A lightweight binding/view helper is acceptable, but ownership should remain typed.

## Plan / Phases

### Phase 1: Resource inventory

- Objective: document current resource groups, lifecycles, and descriptor-binding dependencies.
- Expected output: short inventory of static, resize-dependent, per-frame/per-update, and manual draw resources.
- Dependencies/blockers: none.
- Status: done.

### Phase 2: Split tracer resource domains

- Objective: break `TracerResources` into clear typed sub-resources while preserving public descriptor names.
- Expected output: new/refined structs such as uniforms, shadow/VSM, wind, terrain query, noise/static textures, extent-dependent, and denoiser resources.
- Dependencies/blockers: Phase 1; ensure derive macro handles nested structs as expected.
- Status: done.

### Phase 3: Simplify construction helpers

- Objective: reduce repeated buffer/texture construction code without changing behavior.
- Expected output: small helper functions for uniform buffers, shader-layout buffers, and common texture descriptors.
- Dependencies/blockers: Phase 2 shape should be mostly settled first.
- Status: done.

### Phase 4: Clarify pipeline resource binding

- Objective: reduce repeated `&[&dyn ResourceContainer]` assembly and make pipeline dependencies explicit.
- Expected output: small typed helper/view functions for tracer-only and tracer+builder resource sets.
- Dependencies/blockers: Phase 2.
- Status: done.

### Phase 5: Final cleanup and docs

- Objective: remove stale comments/dead code introduced by the refactor and update this progress file.
- Expected output: concise final notes, risks, and validation results.
- Dependencies/blockers: Phases 2-4.
- Status: done.

## Verification Method

Incremental checks after meaningful Rust changes:

```bash
cargo fmt --check  # passed 2026-06-01
cargo check       # passed 2026-06-01
```

Final checks:

```bash
cargo test                                      # passed 2026-06-01
cargo run --release -- --hidden --auto-exit 0.5 # passed 2026-06-01
cargo run --release -- --tail-latest-log 200    # inspected 2026-06-01
```

Acceptance criteria:

- Descriptor auto-update finds all previously bound resources.
- No `Resource not found` or duplicate resource binding errors in logs.
- Hidden release run exits cleanly without Vulkan/shader/resource errors.
- Public behavior and rendering pipeline order are unchanged.
- Any generated diffs, if present, come from normal build/check outputs only.

## Progress Log

- 2026-06-01: Discussed resource struct maintainability. Decision: avoid a single global dynamic GPU resource container; prefer typed subgrouping with `ResourceContainer` views/helpers.
- 2026-06-01: Created branch `agent/resource-containers` in the current worktree, per request.
- 2026-06-01: Created this progress document under `docs/` following existing progress-doc convention.


- 2026-06-01: Split `TracerResources` into typed domain groups (`uniforms`, `shadow`, `wind`, `terrain_query`, `textures`, `meshes`) while keeping descriptor names on leaf fields for auto-binding.
- 2026-06-01: Added subgroup constructors to shorten `TracerResources::new` and keep lifecycle/domain setup close to each group.
- 2026-06-01: Added typed descriptor-resource helper methods on `Tracer` to centralize tracer-only and all-resource binding slices.
- 2026-06-01: Fixed `ResourceContainer` derive for structs with only nested containers by typing the empty direct-name slice.
- 2026-06-01: Verified with `cargo fmt --check`, `cargo check`, `cargo test`, hidden release run, and latest-log inspection.

## Open Questions / Risks

- The derive macro skips some container-like fields such as `Vec`; manual draw resources may need to stay outside automatic descriptor binding.
- Runtime duplicate-name checks in nested containers may become noisier as grouping increases.
- Splitting structs can increase indirection if names are too granular; prefer domain/lifecycle groups, not one struct per resource.
- Some resources mix ownership data with side metadata (`indices_len`, capacities, instance counts); these should remain typed and close to their buffers.
- Need confirm whether any external code assumes the exact current `TracerResources` field paths.
