# Leaf Shadow Opacity Progress

## Goal

Implement a separate moving-leaf shadow path that keeps leaves casting shadows from their current animated geometry while avoiding the cost and quality problems of putting high-frequency leaf detail into the main terrain VSM.

Done means:

- main VSM continues to handle terrain/stable scene shadows;
- animated leaves no longer pollute the main VSM moments;
- grass/flora can receive current animated leaf shadows through a separate leaf shadow texture/path;
- most grass receivers avoid expensive leaf-shadow work when they are outside possible leaf-shadow regions;
- validation shows stable terrain shadows, believable animated leaf shadows, and no obvious shader/runtime errors.

## Current State

Known pipeline pieces:

- `shader/foliage/leaves_shadow.vert` renders animated leaf/apple shadow casters into `shadow_map_depth_tex`.
- `shader/tracer/shadow_depth_copy.comp` copies raster depth into `shadow_map_tex`.
- `shader/tracer/tracer_shadow.comp` merges terrain depth into `shadow_map_tex`.
- `shader/tracer/vsm_creation.comp`, `vsm_blur_h.comp`, and `vsm_blur_v.comp` convert/filter the single shadow depth into VSM moments.
- Flora/grass currently samples VSM in `shader/include/vsm.glsl` via `get_shadow_weight_vsm_temporal(...)` from `shader/foliage/flora_common.glsl`.
- Shadow resources are defined in `src/tracer/resources.rs`; pass scheduling is mostly in `src/tracer/mod.rs`.
- GUI shadow controls currently include `vsm_blur_radius` and `vsm_temporal_alpha` in `config/gui.toml`.

Constraints and assumptions:

- Leaves should cast shadows from their actual animated pose; no frozen/rest-pose proxy.
- PCSS for all grass receivers is too expensive; only receivers likely under leaf shadows should pay extra cost.
- The preferred direction is a separate leaf opacity/transmittance/deep-opacity shadow path, not a single mixed terrain+leaf shadow map.
- Need to confirm acceptable accuracy: simple 2D opacity map vs layered/deep opacity map with receiver light-depth awareness.
- Run `cargo check` after shader/Rust changes so generated shader-derived Rust structs update normally.

## Plan / Phases

### Phase 1: Design the leaf shadow representation

- Objective: choose exact texture format, coordinate/depth model, and receiver sampling rule.
- Expected output: short design note or updated section in this document specifying 2D opacity, layered opacity, or another representation.
- Dependencies/blockers: decide how much depth correctness is required for receivers above/below canopy.
- Status: done.

Chosen design:

- Use a separate 2D light-space opacity map at the existing shadow-map resolution.
- Render current animated leaf/apple caster geometry into an `R8G8B8A8_UNORM` color target and accumulate fragment alpha with standard over blending; the alpha channel is interpreted as leaf opacity.
- Do not feed animated leaf depth into the terrain VSM path. The main `shadow_map_tex` and VSM moments remain terrain/stable-scene only.
- Build a small low-resolution light-space influence mask from the opacity map. Receivers sample this mask first and skip high-resolution leaf-opacity sampling when the mask is empty.
- Receiver rule: terrain VSM visibility is multiplied by leaf transmittance `1 - opacity * strength` only for non-tree-leaf flora receivers inside the mask.
- Depth model: first implementation is intentionally 2D opacity, not layered/deep opacity. This can over-shadow receivers in front of the canopy, but is acceptable for grass/low flora under trees and avoids a much more expensive deep-opacity path.

### Phase 2: Add leaf shadow resources and descriptors

- Objective: add separate leaf shadow texture(s), sampler, descriptor bindings, and any push/uniform data.
- Expected output: new resources in `src/tracer/resources.rs` and pipeline descriptor updates.
- Dependencies/blockers: Phase 1 format/layout decision.
- Status: not started.

### Phase 3: Render animated leaves into the separate leaf shadow path

- Objective: reuse current animated leaf shadow geometry/wind path, but write leaf opacity/transmittance instead of feeding leaf depth into the main VSM.
- Expected output: shader/pass changes for leaf-only shadow accumulation; main VSM excludes or ignores leaf depth.
- Dependencies/blockers: Phase 2 resources; need blend/atomic strategy for opacity accumulation.
- Status: not started.

### Phase 4: Build a cheap leaf-shadow influence mask

- Objective: identify light-space tiles/chunks where leaf shadow can affect receivers.
- Expected output: low-res mask or CPU/GPU metadata so most grass can skip leaf-shadow sampling.
- Dependencies/blockers: Phase 1 representation and pass layout.
- Status: not started.

### Phase 5: Sample leaf shadow in flora/grass shaders

- Objective: multiply terrain VSM shadow by separate leaf transmittance only when the receiver may be affected.
- Expected output: `flora_common.glsl` or related shader changes that compute `final_shadow = terrain_vsm * leaf_shadow`.
- Dependencies/blockers: Phase 2-4.
- Status: not started.

### Phase 6: Tune and validate quality/performance

- Objective: tune resolution, blur/mip, opacity strength, and skip thresholds.
- Expected output: validated settings and documented tradeoffs.
- Dependencies/blockers: implementation phases complete.
- Status: not started.

## Verification Method

Correctness checks:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Manual/visual acceptance:

- Leaves still cast shadows from their current animated positions.
- Terrain/stable shadows remain at least as good as current VSM output.
- Leaf shadows do not create VSM light bleeding/ghosting in unrelated areas.
- Grass outside leaf-shadow influence areas skips the extra leaf-shadow path.
- Grass under trees receives visible animated leaf shadow/transmittance.
- No new Vulkan validation/log errors in hidden release run logs.

Performance acceptance:

- Compare release-mode hidden runs before/after.
- Confirm the extra receiver cost is localized by mask/tile/chunk gating.
- If GPU profiling is available, inspect pass cost for leaf shadow generation and receiver sampling.

Verification not yet possible because implementation has not started.

## Progress Log

- 2026-06-02: Confirmed current grass/flora VSM sampling is per voxel world position, not one shared blade position.
- 2026-06-02: Discussed that exact moving leaves in the main VSM cause high-frequency instability, blur/light bleeding, and temporal ghosting.
- 2026-06-02: Decided to pursue a separate moving-leaf opacity/transmittance shadow path instead of forcing PCSS on all receivers or freezing shadow casters.
- 2026-06-02: Created worker worktree `/Users/bytedance/code/re-flora-agent-leaf-shadow` on branch `agent/leaf-opacity-shadow` for this task.
- 2026-06-02: Added this progress document; no implementation changes yet.

## Open Questions / Risks

- Should the first implementation be a simple 2D light-space opacity map, or a layered/deep opacity map to avoid affecting receivers in front of the leaves?
- What resolution is enough for believable animated leaf shadows without excessive cost?
- Should opacity accumulation use raster blending, storage image atomics, or a compute post-process?
- How should the receiver skip mask be represented: light-space tile mask, world chunk metadata, or both?
- How much blur/mip filtering is acceptable before leaf shadows feel too soft?
- Need to ensure removing leaves from main VSM does not regress other receivers that currently depend on leaf depth in `shadow_map_tex`.
