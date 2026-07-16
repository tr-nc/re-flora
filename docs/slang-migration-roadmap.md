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
| Native Slang complete | 17 | Native source is independently selectable and has passed local gates |
| Slang backend only | 0 | Existing GLSL compiles through Slang; native rewrite remains TODO |
| GLSL only | 59 | No completed Slang replacement yet |

Current aggregate `slang-validation` build:

```text
59 shaderc GLSL + 0 Slang GLSL + 17 native Slang = 76 entry points
```

The validated native entry points are post-processing, composition, sparse surface extraction and its indirect-dispatch preparation, the complete six-entry contree construction family, the main, shadow, and player-collider tracer passes, and the egui and flora vertex/fragment pairs. The retained composition and main-tracer backend features remain available as frontend baselines but are no longer backend-only candidates.

The aggregate build now dynamically loads the Slang compiler library once and reuses one global compiler session for all 34 selected reflection/optimized artifacts. At the earlier 16-artifact snapshot, the local Linux Vulkan SDK 2025.23.2 toolchain reduced the median package-clean aggregate check from 6.29 s to 5.05 s and the median shader-touched incremental check from 5.83 s to 4.66 s. The build now also records compiler-resolved transitive dependencies and reuses unchanged reflection/optimized artifacts. Those API-produced artifacts remain byte-identical to uncached output, and the current aggregate's 152 artifacts pass Vulkan 1.3 SPIR-V validation. The 17-entry aggregate hidden release smoke run completes on native Vulkan; the preceding 16-entry aggregate also passed through MoltenVK.

## Phase 2 reassessment

**Decision: continue the staged native migration, but keep GLSL as the default and do not begin family-wide translation until build invalidation and source layout are productionized.**

The decisive candidates cover the largest source, the branch-heavy main tracer, traversal and workgroup synchronization, atomics, runtime and fixed arrays, matrix-heavy resources, formatted storage images, and complex vertex/fragment interfaces. Their ABI, SPIR-V, semantic output, and runtime gates pass. Measured GPU results range from parity to a documented `+1.3%` native main-tracer median (`+0.5%` for the enclosing render scope); no material frame-level regression has appeared. Native code is split across 51 focused files with shared traversal, contree-build layouts, packing, lighting, and type modules rather than entry-point copies. The remaining compiler-specific handling is localized to build/reflection boundaries plus explicit source annotations such as raw Vulkan instance indexing.

A fresh three-sample Apple M4 Pro check used order `default, aggregate, aggregate, default, default, aggregate` with Slang `2025.11-12-gc5295eae2`:

| Build case | Default GLSL median (range) | 11-entry aggregate median (range) | Aggregate delta |
| --- | ---: | ---: | ---: |
| Package-clean `re-flora-vkn` rebuild | 3.72 s (3.63-4.57) | 6.37 s (6.20-7.31) | +2.66 s / +71.5% |
| Any-shader-touched rebuild | 3.13 s (3.09-4.22) | 5.54 s (5.47-6.08) | +2.41 s / +76.9% |

At reassessment time, the aggregate reran every shader compilation after any shader-tree change, a cost that could not scale linearly to 76 native entries. Accepted sources now live in the stable `shader/slang/` root, and compiler-reported transitive dependency caching reuses unchanged per-entry reflection and optimized SPIR-V. On the same machine, the cached aggregate median is 2.29 s when all 76 entries are reusable, 2.42 s for a one-entry GLSL edit, and 4.03 s when a shared native module invalidates four entries; the package-clean median remains 6.73 s. Explicit resource declarations plus the automatic GLSL-reference ABI gate remain the binding source of truth; schema generation is deferred unless declaration drift becomes a recurring failure. Compiler pinning, broader native-Vulkan evidence, and cross-platform CI remain release/default-switch gates rather than blockers for isolated migration work.

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
- [x] Track compiler-resolved transitive GLSL includes and Slang imports, then reuse unchanged per-entry reflection/optimized artifacts with dependency and artifact integrity digests.
- [x] Retain explicit declarations plus automatic GLSL-reference ABI checking as the binding source of truth; defer schema generation unless drift becomes recurring.
- [x] Keep automatic binding allocation disabled during migration.
- [x] Define `shader/slang/` as the production native-source root.
- [x] Move the initial 45 accepted sources out of `shader/experiments/slang/` into `shader/slang/` without changing logical shader identities.

### Phase 2 — decisive native high-risk coverage

- [x] Rewrite the full `tracer.comp` entry point in native Slang, reusing the validated contree/DDA modules.
- [x] Rewrite `composition.comp` in native Slang modules after its backend baseline, including its currently disabled panel, glass, volumetric-cloud reflection, and SSR helpers.
- [x] Port `flora.vert` + `flora.frag` as the representative complex graphics pair.
- [x] Port `player_collider.comp` for an additional synchronization-heavy consumer of traversal modules.
- [x] Re-evaluate adoption risk before broad mechanical migration: continue staged migration with GLSL default, after productionizing source layout and incremental artifact reuse.

### Phase 3 — migrate complete compute families

The production source move and dependency-aware artifact cache are complete, and Phase 3 is underway. The order maximizes reuse and keeps failures local:

- [x] Finish contree construction: all 6 entry points are native.
- [ ] Finish surface construction: 8 remaining entry points.
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

### Builder: contree — 6/6 native

- [x] `shader/builder/contree/buffer_setup.comp` — `slang-contree-buffer-setup`
- [x] `shader/builder/contree/buffer_update.comp` — `slang-contree-buffer-update`
- [x] `shader/builder/contree/concat.comp` — `slang-contree-concat`
- [x] `shader/builder/contree/last_buffer_update.comp` — `slang-contree-last-buffer-update`
- [x] `shader/builder/contree/leaf_write.comp` — `slang-contree-leaf`
- [x] `shader/builder/contree/tree_write.comp` — `slang-contree-tree-write`

### Builder: scene acceleration — 0/1 native

- [ ] `shader/builder/scene_accel/update_scene_tex.comp`

### Builder: surface — 2/10 native

- [ ] `shader/builder/surface/active_surface_to_flora_instances.comp`
- [ ] `shader/builder/surface/clear_occupancy.comp`
- [ ] `shader/builder/surface/edit_occupancy_capsule.comp`
- [ ] `shader/builder/surface/instances_to_occupancy.comp`
- [x] `shader/builder/surface/make_surface_sparse.comp` — `slang-surface-make-sparse`
- [ ] `shader/builder/surface/make_surface.comp`
- [ ] `shader/builder/surface/occupancy_to_flora_instances.comp`
- [ ] `shader/builder/surface/prepare_active_surface_flora_dispatch.comp`
- [x] `shader/builder/surface/prepare_sparse_surface_dispatch.comp` — `slang-surface-prepare-sparse-dispatch`
- [ ] `shader/builder/surface/update_flora_growth.comp`

### Denoiser — 0/2 native

- [ ] `shader/denoiser/spatial.comp`
- [ ] `shader/denoiser/temporal.comp`

### Egui — 2/2 native

- [x] `shader/egui/egui.frag` — `slang-egui`
- [x] `shader/egui/egui.vert` — `slang-egui`

### Foliage — 2/7 native

- [ ] `shader/foliage/flora_lod.vert`
- [x] `shader/foliage/flora.frag` — `slang-flora`
- [x] `shader/foliage/flora.vert` — `slang-flora`
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

### Tracer — 5/21 native

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
- [x] `shader/tracer/player_collider.comp` — `slang-player-collider`
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
| Full composition native translation | Active sky/composition plus disabled panel, glass, volumetric-cloud reflection, and SSR logic are split into native modules; temporary helper reactivation was visually equivalent | Keep the helpers disabled until a product decision, and repeat performance gates if they are re-enabled |
| Complex graphics interfaces | Egui and the full flora pair pass, including raw Vulkan instance indexing, fixed-array push constants, many resources, and interpolation | Cover the remaining foliage LOD/leaf/shadow vertex paths during family migration |
| Incremental build scaling | Compiler-reported GLSL/Slang dependency graphs drive per-entry BLAKE3 cache manifests; all-reused, one-GLSL-entry, and four-native-entry aggregate medians are 2.29 s, 2.42 s, and 4.03 s | Preserve dependency capture and artifact-integrity checks as families migrate |
| Production source layout | All 54 accepted modules and entries live under `shader/slang/`; runtime logical paths remain unchanged | Keep native production sources in this root as families migrate |
| Binding source of truth | Explicit declarations plus automatic GLSL-reference ABI checking are retained | Revisit schema generation only if declaration drift becomes recurring |
| Matrix conventions | Native column-major; GLSL frontend row-major lowering | Keep flags centralized and covered by fixed-camera tests |
| Reflection normalization | Slang wrapper names require boundary normalization | Remove only when production reflection no longer emits those forms |
| Storage-image descriptor warning | Existing pipeline exposes 9 images against a reported limit of 8 | Track separately; do not attribute it to Slang |
| GLSL fallback lifetime | GLSL remains the default after Phase 2 | Decide final lifetime at Phase 6; preserve fallback through any initial default switch |

## Next work queue

Do these in order unless new measurements change the priority:

1. [x] **Compiler-session spike**: the build now uses one dynamically loaded global session; local aggregate checks improved by about 20% with byte-identical artifacts.
2. [x] **Full native tracer**: the native entry reuses the shared traversal modules and passes ABI, SPIR-V, screenshot, runtime, and local native-Vulkan timing gates.
3. [x] **Native composition modules**: the active entry and disabled helper paths use focused sky, starlight, sunlight, cloud, panel, glass, SSR, scene, hash, and type modules. ABI, SPIR-V, day/night screenshots, temporary helper reactivation, runtime, and local native-Vulkan timing gates pass.
4. [x] **Complex flora graphics pair**: native modules cover fixed-array push constants, raw Vulkan instance indexing, many resources, shadows, wind sampling, and interpolation. ABI, SPIR-V, authored-flora screenshots, runtime, and matched graphics timing gates pass.
5. [x] **Player collider**: the native pass reuses the shared contree/DDA modules and preserves the five-binding ABI, 64-thread workgroup, per-invocation traversal stacks, fixed-array result buffer, and workgroup reductions. Both frontends now keep all invocations active through the barrier. SPIR-V, pipeline creation, and temporary matched GPU execution/readback gates pass.
6. [x] **Phase 2 reassessment**: decisive compatibility and correctness coverage supports continued staged migration, but the default remains GLSL. The reassessment identified production source layout and incremental artifact reuse as explicit Phase 3 blockers; items 7 and 8 close both.
7. [x] **Production source layout**: the initial 45 accepted modules and entries moved under the stable `shader/slang/` root without changing logical shader identities; new native sources continue to use that root.
8. [x] **Incremental shader artifacts**: shaderc callbacks and Slang's dependency API record the resolved transitive graphs for both the GLSL ABI reference and selected replacement. Per-entry BLAKE3 manifests reuse valid reflection/optimized SPIR-V and detect dependency changes or artifact corruption.
9. [x] **Complete contree construction**: all six entries are native and independently selectable. Their aggregate feature preserves every ABI and all 28 matched node/leaf workloads; the full pipeline median was 107.5 us versus 112.5 us for GLSL. Tree writing retains uniform barrier control flow across inactive edge invocations.
10. **Complete surface construction**: the sparse extraction and indirect-dispatch preparation entries are native. Port the eight remaining surface entry points, reusing their accepted layouts and solid-workgroup helpers.
