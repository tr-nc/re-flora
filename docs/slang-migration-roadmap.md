# Slang migration roadmap

## Purpose

This is the authoritative execution tracker for migrating re-flora's production shaders from GLSL to native Slang without a one-shot language switch. It answers what “complete” means, which work comes next, how each migration is validated, and which of the 76 production entry points remain.

Use the other Slang documents for supporting detail:

- [`slang-validation-plan.md`](slang-validation-plan.md): validation protocol, acceptance gates, and evidence summary;
- [`slang-poc.md`](slang-poc.md): build commands, compiler flags, compatibility findings, and measured results.

Update this roadmap in the same commit that changes a shader's migration status. Do not maintain a second shader-status list elsewhere. After changing the shader inventory, override registry, or status, run:

```bash
scripts/check_slang_roadmap.py
```

## Goal

Make native Slang the primary source language for all active Vulkan shader entry points while preserving correctness, GPU performance, and an independently selectable GLSL reference throughout the migration.

The migration is complete only when:

- all 76 active `.comp`, `.vert`, and `.frag` logical entry points have native Slang implementations;
- every native implementation passes the GLSL-reference ABI, correctness, SPIR-V, and applicable performance gates;
- the aggregate Slang build passes on MoltenVK and native Vulkan;
- Windows, macOS, and Fedora CI can reproduce the build with a pinned Slang toolchain;
- Slang compiler startup cost is removed from normal per-entry-point iteration through a shared compiler session, batching, or an equivalent cache;
- the default frontend decision and GLSL fallback lifetime are explicitly approved and documented.

## Non-goals

- Do not translate all shaders in one change.
- Do not change Rust resource layouts merely to make a Slang port easier.
- Do not combine source translation, rendering redesign, and unrelated shader cleanup.
- Do not treat GLSL-through-Slang as a completed native migration.
- Do not claim support for inactive buffer-reference or sparse-residency features without a separate production need and validation case.
- Do not use debug-mode timings as performance evidence.

## Current progress

Inventory: **76 entry points** = 61 compute + 9 vertex + 6 fragment.

| State | Entry points | Meaning |
| --- | ---: | --- |
| Native Slang complete | 8 | Native source is independently selectable and has passed local gates |
| Slang backend only | 0 | Existing GLSL compiles through Slang; native rewrite remains TODO |
| GLSL only | 68 | No completed Slang replacement yet |

Current aggregate `slang-validation` build:

```text
68 shaderc GLSL + 0 Slang GLSL + 8 native Slang = 76 entry points
```

The validated native entry points are post-processing, composition, sparse surface extraction, contree leaf writing, the main and shadow tracer passes, and the egui vertex/fragment pair. The retained composition and main-tracer backend features remain available as frontend baselines but are no longer backend-only candidates.

The aggregate build now dynamically loads the Slang compiler library once and reuses one global compiler session for all 16 reflection/optimized artifacts. On the local Linux Vulkan SDK 2025.23.2 toolchain, this reduced the median package-clean aggregate check from 6.29 s to 5.05 s and the median shader-touched incremental check from 5.83 s to 4.66 s. All 16 API-produced artifacts were byte-identical to separate `slangc` output, all 152 aggregate artifacts passed Vulkan 1.3 SPIR-V validation, and a hidden release smoke run completed on an NVIDIA RTX 3060 Ti through native Vulkan.

## Coexistence contract

These invariants must remain true for every migration step:

1. **Stable logical identity**: Rust requests the existing path such as `shader/tracer/tracer.comp`; it does not branch on source language.
2. **GLSL remains the default during migration**: a default build neither locates nor invokes `slangc`.
3. **Independent replacement**: one feature selects one shader or one inseparable graphics-stage pair; unrelated shaders remain GLSL.
4. **Single override registry**: `crates/re-flora-vkn/build.rs` owns logical path, source path, stage, frontend, include root, and defines.
5. **Explicit frontend**: an override is either native Slang 2025 or GLSL through Slang. Backend validation is never labeled native completion.
6. **Runtime neutrality**: both frontends emit embedded SPIR-V and use the same `ShaderModule::from_precompiled()` path.
7. **Rust ABI authority**: existing descriptor/resource definitions remain authoritative. Do not renumber bindings during translation.
8. **Explicit Vulkan declarations**: native Slang uses `vk::binding`, `vk::location`, `vk::image_format`, push-constant annotations, and explicit read/write qualifiers.
9. **Fixed layout policy**: Vulkan SPIR-V 1.6, `-fvk-use-gl-layout`, column-major native Slang, and row-major lowering only for Slang's GLSL frontend.
10. **Automatic ABI gate**: enabled replacements must match the GLSL reference for stage, workgroup size, descriptors, image contract, top-level buffer layout, push constants, and graphics-stage IO/interpolation.
11. **Aggregate only after validation**: add a candidate to `slang-validation` only after its individual feature passes.
12. **Immediate rollback**: disabling the candidate feature must restore the exact GLSL logical path without Rust code changes.

## Per-migration workflow

Treat each shader or inseparable vertex/fragment pair as a separate validated commit series.

### 1. Scope and baseline

- [ ] Select one logical entry point or graphics pair.
- [ ] Identify its Rust pipeline/resource owners and likely shared modules.
- [ ] Record the GLSL descriptor ABI and relevant SPIR-V capabilities.
- [ ] Capture a release-mode baseline screenshot or semantic output.
- [ ] Capture an order-reversible release performance baseline when the pass is measurable.

For a large or risky GLSL shader, first add a temporary GLSL-through-Slang backend feature. This isolates parser/backend behavior from native translation. It does not satisfy native completion.

### 2. Native implementation

- [ ] Port reusable pure logic into focused `.slang` modules.
- [ ] Keep the entry point responsible only for resource declarations and orchestration where practical.
- [ ] Preserve descriptor sets, bindings, image formats, workgroup size, push constants, and byte layouts.
- [ ] Declare storage access explicitly with read-only/write-only types or qualifiers.
- [ ] Avoid opportunistic algorithm changes; make optimization a later separable commit.
- [ ] Add an independent Cargo feature and one declarative build override.

### 3. Automated validation

- [ ] `cargo check` passes without requiring Slang.
- [ ] `cargo check --features <candidate>` passes its automatic GLSL-reference ABI check.
- [ ] `cargo check --features slang-validation` passes with all accepted candidates together.
- [ ] Reflection and optimized SPIR-V pass `spirv-val --target-env vulkan1.3`.
- [ ] Required capabilities/extensions are no broader than intended.
- [ ] Relevant Rust tests pass.

### 4. Runtime correctness

- [ ] Hidden release smoke run exits cleanly.
- [ ] Run log has no candidate-specific validation error or panic.
- [ ] Deterministic counters, hashes, dimensions, and allocation sizes match where available.
- [ ] Fixed-camera output is byte-identical or has a documented tolerance and dynamic-region exclusion.
- [ ] Aggregate hidden release run also succeeds.

### 5. Performance

- [ ] Use matched hidden release workloads from the same worktree.
- [ ] Run at least one order-reversed A/B pair; use more samples for noisy or sub-100-us passes.
- [ ] Report sample count, mean, median, P95, range, and run order.
- [ ] Investigate every repeatable delta outside normal run variance.
- [ ] Record stripped SPIR-V bytes/instruction count as diagnostics, not as a substitute for GPU timing.
- [ ] Leave the item incomplete if a material regression has no accepted explanation.

### 6. Handoff

- [ ] Add the candidate to `slang-validation` only after all applicable gates pass.
- [ ] Update this inventory and the evidence document in the same commit.
- [ ] Record changed files, commands, known risks, and generated-file changes.
- [ ] Commit the validated unit before starting the next shader.

## Roadmap phases

### Phase 0 — feasibility and mixed-build foundation

- [x] Preserve a GLSL-only default build.
- [x] Add independent feature-gated Slang overrides.
- [x] Validate storage images, uniforms, shared memory, barriers, atomics, structured/runtime SSBOs, matrices, traversal, and basic graphics stages on MoltenVK.
- [x] Add automatic frontend ABI comparison.
- [x] Distinguish native Slang from GLSL-through-Slang in build output.
- [ ] Pin and log an approved Slang compiler version in the build/CI contract.
- [ ] Add a portable SPIR-V validation command or CI step for every selected artifact.

### Phase 1 — production build architecture

Complete this before scaling native migration far beyond the current candidates.

- [x] Measure package-clean and shader-touched incremental aggregate build cost for the current candidate set.
- [x] Replace two `slangc` process launches per selected entry point with one dynamically loaded compiler API global session.
- [ ] Track imported `.slang` module dependencies precisely enough for incremental rebuilds.
- [ ] Decide whether explicit declarations plus ABI checking remain the binding source of truth or whether Rust/Slang declarations should be generated from one resource schema.
- [ ] Keep automatic binding allocation disabled during migration.
- [ ] Define where production Slang sources live and move accepted sources out of `shader/experiments/slang/` when the layout is stable.

### Phase 2 — decisive native high-risk coverage

- [x] Rewrite the full `tracer.comp` entry point in native Slang, reusing the validated contree/DDA modules.
- [x] Rewrite the active `composition.comp` production path in native Slang modules after its backend baseline.
- [ ] Port `flora.vert` + `flora.frag` as the representative complex graphics pair.
- [ ] Port `player_collider.comp` for an additional synchronization-heavy consumer of traversal modules.
- [ ] Re-evaluate adoption risk before broad mechanical migration.

### Phase 3 — migrate complete compute families

Suggested order maximizes reuse and keeps failures local:

- [ ] Finish contree construction: 5 remaining entry points.
- [ ] Finish surface construction: 9 remaining entry points.
- [ ] Migrate scene acceleration: 1 entry point.
- [ ] Migrate denoiser: 2 entry points.
- [ ] Finish tracer compute family: 17 GLSL-only entry points at the current snapshot.
- [ ] Migrate chunk writer: 21 entry points in small behavior-related groups.

### Phase 4 — migrate graphics families

Graphics shaders must be validated as pipeline pairs even when only one stage changes.

- [ ] Finish foliage: 7 entry points.
- [ ] Migrate particles: 3 entry points.
- [ ] Migrate sprinkler props: 1 vertex entry point with its existing fragment-stage pairing verified.
- [ ] Migrate terrarium glass: 2 entry points.
- [x] Migrate egui: 2 entry points.

### Phase 5 — cross-platform release gate

- [ ] Install and pin Slang in macOS, Windows, and Fedora CI.
- [ ] Run default GLSL, each current candidate class, and aggregate Slang builds in CI.
- [ ] Validate MoltenVK on macOS and native Vulkan on at least one Windows and one Linux GPU/driver.
- [ ] Repeat authoritative hot-pass benchmarks on native Vulkan.
- [ ] Confirm descriptor limits, image formats, subgroup/workgroup behavior, and graphics interpolation across drivers.
- [ ] Document compiler/driver-specific workarounds and decide whether any shader must remain GLSL.

### Phase 6 — default switch and cleanup

- [ ] Confirm all 76 inventory items are native-complete.
- [ ] Confirm aggregate build, tests, screenshots, semantic checks, and benchmarks pass.
- [ ] Decide whether Slang becomes the default or the project remains intentionally mixed.
- [ ] If switching, add an explicit GLSL fallback mode before inverting the default.
- [ ] Keep the fallback for an agreed stabilization period; do not delete GLSL in the default-switch commit.
- [ ] Remove backend-only experiment features and obsolete reflection workarounds only after no active path uses them.
- [ ] Move accepted Slang modules to their final production directory and update documentation.
- [ ] Make the final decision in a small, reversible commit.

## Entry-point inventory

A checked item means a native Slang implementation has passed all applicable local gates and is in `slang-validation`. “Backend validated” means GLSL-through-Slang works but the native implementation is still TODO.

### Builder: chunk writer — 0/21 native

- [ ] `shader/builder/chunk_writer/buffer_setup.comp`
- [ ] `shader/builder/chunk_writer/chunk_heightmap.comp`
- [ ] `shader/builder/chunk_writer/chunk_init.comp`
- [ ] `shader/builder/chunk_writer/chunk_modify_sample.comp`
- [ ] `shader/builder/chunk_writer/chunk_modify.comp`
- [ ] `shader/builder/chunk_writer/chunk_solid_sample.comp`
- [ ] `shader/builder/chunk_writer/model_voxelize.comp`
- [ ] `shader/builder/chunk_writer/terrain_fertility_brush.comp`
- [ ] `shader/builder/chunk_writer/terrain_moisture_brush.comp`
- [ ] `shader/builder/chunk_writer/terrain_moisture_dry.comp`
- [ ] `shader/builder/chunk_writer/terrain_moisture_spread.comp`
- [ ] `shader/builder/chunk_writer/terrain_smooth_apply.comp`
- [ ] `shader/builder/chunk_writer/terrain_smooth_heights.comp`
- [ ] `shader/builder/chunk_writer/terrain_smooth_mbo_apply.comp`
- [ ] `shader/builder/chunk_writer/terrain_smooth_mbo_diffuse_ab.comp`
- [ ] `shader/builder/chunk_writer/terrain_smooth_mbo_diffuse_ba.comp`
- [ ] `shader/builder/chunk_writer/terrain_smooth_mbo_init.comp`
- [ ] `shader/builder/chunk_writer/terrain_smooth_mbo_score.comp`
- [ ] `shader/builder/chunk_writer/terrain_smooth_target.comp`
- [ ] `shader/builder/chunk_writer/terrain_soil_mix.comp`
- [ ] `shader/builder/chunk_writer/voxel_property_sample.comp`

### Builder: contree — 1/6 native

- [ ] `shader/builder/contree/buffer_setup.comp`
- [ ] `shader/builder/contree/buffer_update.comp`
- [ ] `shader/builder/contree/concat.comp`
- [ ] `shader/builder/contree/last_buffer_update.comp`
- [x] `shader/builder/contree/leaf_write.comp` — `slang-contree-leaf`
- [ ] `shader/builder/contree/tree_write.comp`

### Builder: scene acceleration — 0/1 native

- [ ] `shader/builder/scene_accel/update_scene_tex.comp`

### Builder: surface — 1/10 native

- [ ] `shader/builder/surface/active_surface_to_flora_instances.comp`
- [ ] `shader/builder/surface/clear_occupancy.comp`
- [ ] `shader/builder/surface/edit_occupancy_capsule.comp`
- [ ] `shader/builder/surface/instances_to_occupancy.comp`
- [x] `shader/builder/surface/make_surface_sparse.comp` — `slang-surface`
- [ ] `shader/builder/surface/make_surface.comp`
- [ ] `shader/builder/surface/occupancy_to_flora_instances.comp`
- [ ] `shader/builder/surface/prepare_active_surface_flora_dispatch.comp`
- [ ] `shader/builder/surface/prepare_sparse_surface_dispatch.comp`
- [ ] `shader/builder/surface/update_flora_growth.comp`

### Denoiser — 0/2 native

- [ ] `shader/denoiser/spatial.comp`
- [ ] `shader/denoiser/temporal.comp`

### Egui — 2/2 native

- [x] `shader/egui/egui.frag` — `slang-egui`
- [x] `shader/egui/egui.vert` — `slang-egui`

### Foliage — 0/7 native

- [ ] `shader/foliage/flora_lod.vert`
- [ ] `shader/foliage/flora.frag`
- [ ] `shader/foliage/flora.vert`
- [ ] `shader/foliage/leaves_lod.vert`
- [ ] `shader/foliage/leaves_shadow.frag`
- [ ] `shader/foliage/leaves_shadow.vert`
- [ ] `shader/foliage/leaves.vert`

### Particles — 0/3 native

- [ ] `shader/particles/particle_lod_textured.frag`
- [ ] `shader/particles/particle_lod_textured.vert`
- [ ] `shader/particles/water_droplet.frag`

### Props — 0/1 native

- [ ] `shader/props/sprinkler.vert`

### Terrarium — 0/2 native

- [ ] `shader/terrarium/glass.frag`
- [ ] `shader/terrarium/glass.vert`

### Tracer — 4/21 native

- [ ] `shader/tracer/cloud_shadow_temporal.comp`
- [ ] `shader/tracer/cloud_shadow.comp`
- [ ] `shader/tracer/cloud_temporal.comp`
- [ ] `shader/tracer/cloud.comp`
- [x] `shader/tracer/composition.comp` — `slang-composition` (backend baseline retained as `slang-composition-backend`)
- [ ] `shader/tracer/god_ray.comp`
- [ ] `shader/tracer/leaf_shadow_mask.comp`
- [ ] `shader/tracer/leaf_shadow_temporal.comp`
- [ ] `shader/tracer/lens_flare_downsample.comp`
- [ ] `shader/tracer/lens_flare_sun_visible.comp`
- [ ] `shader/tracer/lens_flare.comp`
- [ ] `shader/tracer/player_collider.comp`
- [x] `shader/tracer/post_processing.comp` — `slang-post-processing`
- [ ] `shader/tracer/shadow_depth_copy.comp`
- [ ] `shader/tracer/terrain_query.comp`
- [x] `shader/tracer/tracer_shadow.comp` — `slang-tracer-shadow`
- [x] `shader/tracer/tracer.comp` — `slang-tracer` (backend baseline retained as `slang-tracer-backend`)
- [ ] `shader/tracer/vsm_blur_h.comp`
- [ ] `shader/tracer/vsm_blur_v.comp`
- [ ] `shader/tracer/vsm_creation.comp`
- [ ] `shader/tracer/wind_volume.comp`

## Known risks and open decisions

| Risk or decision | Current state | Required resolution |
| --- | --- | --- |
| Slang compiler integration | Process startup is removed locally through one dynamically loaded global session; aggregate checks improved by about 20% | Pin the compiler ABI/version and reproduce the API path on Windows and macOS CI |
| Native Vulkan performance | Main tracer has order-reversed RTX 3060 Ti evidence; most other candidates have only MoltenVK evidence | Repeat hot-pass benchmarks across additional native Vulkan vendors and drivers |
| Cross-platform toolchain | CLI path tested on macOS; compiler API and aggregate runtime tested locally on Linux | Pinned compiler install plus Windows/macOS/Linux CI matrix |
| Full tracer native translation | Native resources, traversal, lighting, materials, preview, and output orchestration pass locally; native `tracer.pass` median was 1.3% above shaderc on RTX 3060 Ti | Recheck the small measured delta on additional drivers while migrating shared modules |
| Full composition native translation | The active open-tank path, including sky, sun sprite, stars, precomputed clouds, lens flare, and god rays, passes locally; native timing is within 1 us of shaderc on RTX 3060 Ti | Port the currently disabled panel/glass/SSR helper source before re-enabling it or removing the GLSL fallback |
| Complex graphics interfaces | Egui passes; foliage remains | Native flora pair plus interpolation/instance-input validation |
| Binding source of truth | Explicit declarations plus ABI checker | Decide whether generation from one schema provides enough benefit |
| Matrix conventions | Native column-major; GLSL frontend row-major lowering | Keep flags centralized and covered by fixed-camera tests |
| Reflection normalization | Slang wrapper names require boundary normalization | Remove only when production reflection no longer emits those forms |
| Storage-image descriptor warning | Existing pipeline exposes 9 images against a reported limit of 8 | Track separately; do not attribute it to Slang |
| GLSL fallback lifetime | Not decided | Decide at Phase 6; preserve fallback through initial default switch |

## Next work queue

Do these in order unless new measurements change the priority:

1. [x] **Compiler-session spike**: the build now uses one dynamically loaded global session; local aggregate checks improved by about 20% with byte-identical artifacts.
2. [x] **Full native tracer**: the native entry reuses the shared traversal modules and passes ABI, SPIR-V, screenshot, runtime, and local native-Vulkan timing gates.
3. [x] **Native composition entry**: the active production path uses focused sky, starlight, sunlight, and type modules and passes ABI, SPIR-V, day/night screenshot, runtime, and local native-Vulkan timing gates.
4. **Inactive composition helpers**: port the open-tank panel, glass, volumetric-cloud reflection, and SSR helpers before they are re-enabled or the GLSL fallback is removed.
5. **Complex flora graphics pair**: validate instance-rate input, push constants, many resources, shadows, wind sampling, and interpolation.
6. **Player collider**: validate another synchronization-heavy traversal consumer.
7. Reassess the roadmap using measured build cost, correctness, and native-code maintainability before beginning family-wide migration.
