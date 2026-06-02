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
- Render current animated leaf/apple caster geometry into an `R8G8B8A8_UNORM` color target and accumulate fragment alpha with standard over blending; alpha is interpreted as leaf opacity and red stores opacity-weighted light-space depth for receiver depth gating.
- Do not feed animated leaf depth into the terrain VSM path. The main `shadow_map_tex` and VSM moments remain terrain/stable-scene only.
- Build a small low-resolution light-space influence mask from the opacity map. Receivers sample this mask first and skip high-resolution leaf-opacity sampling when the mask is empty.
- Receiver rule: terrain VSM visibility is multiplied by leaf transmittance `1 - opacity * strength` for receivers inside the mask, including terrain, tree leaves, and apples.
- Depth model: first implementation is still not layered/deep opacity, but the opacity map carries approximate light-space caster depth. Receivers ignore opacity samples that are not in front of them along the light ray, which keeps tree-leaf/apple self-shadow direction plausible while avoiding a full deep-opacity path.

### Phase 2: Add leaf shadow resources and descriptors

- Objective: add separate leaf shadow texture(s), sampler, descriptor bindings, and any push/uniform data.
- Expected output: new resources in `src/tracer/resources.rs` and pipeline descriptor updates.
- Dependencies/blockers: Phase 1 format/layout decision.
- Status: done.

Implementation notes:

- Added `leaf_shadow_opacity_tex` at shadow-map resolution.
- Added `leaf_shadow_mask_tex` at 1/8 shadow-map resolution.
- Added descriptor bindings for flora/leaf receivers and a compute mask pass.

### Phase 3: Render animated leaves into the separate leaf shadow path

- Objective: reuse current animated leaf shadow geometry/wind path, but write leaf opacity/transmittance instead of feeding leaf depth into the main VSM.
- Expected output: shader/pass changes for leaf-only shadow accumulation; main VSM excludes or ignores leaf depth.
- Dependencies/blockers: Phase 2 resources; need blend/atomic strategy for opacity accumulation.
- Status: done.

Implementation notes:

- Reused `shader/foliage/leaves_shadow.vert` so shadow casters use current wind/leaf/apple animation.
- Changed `shader/foliage/leaves_shadow.frag` to write opacity alpha plus opacity-weighted light depth into the separate color target.
- Retargeted the leaf shadow graphics pass to `leaf_shadow_opacity_tex` and disabled depth testing/writes for opacity accumulation.
- Stopped feeding animated leaf depth into `shadow_map_depth_tex`; main VSM now starts from clear depth plus terrain traced by `tracer_shadow.comp`.

### Phase 4: Build a cheap leaf-shadow influence mask

- Objective: identify light-space tiles/chunks where leaf shadow can affect receivers.
- Expected output: low-res mask or CPU/GPU metadata so most grass can skip leaf-shadow sampling.
- Dependencies/blockers: Phase 1 representation and pass layout.
- Status: done.

Implementation notes:

- Added `shader/tracer/leaf_shadow_mask.comp` to downsample and dilate the opacity map into a conservative mask.
- The flora receiver path samples this low-res mask before doing high-res opacity PCF.

### Phase 5: Sample leaf shadow in flora/grass shaders

- Objective: multiply terrain VSM shadow by separate leaf transmittance only when the receiver may be affected.
- Expected output: `flora_common.glsl` or related shader changes that compute `final_shadow = terrain_vsm * leaf_shadow`.
- Dependencies/blockers: Phase 2-4.
- Status: done.

Implementation notes:

- Added `shader/include/leaf_shadow.glsl`.
- Flora/leaf vertex shaders bind the opacity and mask textures.
- `flora_common.glsl` multiplies terrain VSM visibility by depth-gated leaf transmittance for flora receivers, including tree leaves and apples.

### Phase 6: Tune and validate quality/performance

- Objective: tune resolution, blur/mip, opacity strength, and skip thresholds.
- Expected output: validated settings and documented tradeoffs.
- Dependencies/blockers: implementation phases complete.
- Status: done.

Tuning notes:

- Leaf shadow controls are exposed in GUI `Shadow` section.
- Current defaults: fragment opacity `0.4`, receiver strength `1.15`, minimum transmittance `0.14`, temporal alpha `0.4`, filter radius `2` texels.
- Mask threshold: `0.003` generation, `0.01` receiver sampling.
- GPU profiler hidden release run before temporal blending showed the extra non-temporal passes at roughly `leaf_shadow_opacity.pass=17-19us` and `leaf_shadow_mask.pass=39-40us` on the tested RTX 3060 Ti path.

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
- Tree leaves and apples also receive the leaf/apple opacity shadow for canopy/fruit self-shadowing.
- No new Vulkan validation/log errors in hidden release run logs.

Performance acceptance:

- Compare release-mode hidden runs before/after.
- Confirm the extra receiver cost is localized by mask/tile/chunk gating.
- If GPU profiling is available, inspect pass cost for leaf shadow generation and receiver sampling.

Latest verification:

- `cargo fmt --check`
- `cargo check`
- `cargo test`
- `cargo run --release -- --hidden --auto-exit 5`
- `cargo run --release -- --hidden --auto-exit 4 --perf`
- `cargo run --release -- --hidden --screenshot target/leaf-shadow-check.png --screenshot-delay 2 --auto-exit 3`
- `cargo run --release -- --hidden --auto-exit 0.5`
- `cargo run --release -- --hidden --auto-exit 2 --perf`

Validation notes:

- Hidden 5s release smoke exited successfully before temporal blending.
- Hidden 0.5s release smoke exited successfully after temporal blending.
- Perf run included `leaf_shadow_opacity.pass`, `leaf_shadow_temporal.pass`, and `leaf_shadow_mask.pass` GPU scopes with no dropped scopes.
- After temporal blending, hidden perf showed `leaf_shadow_temporal.pass` around `89-91us` and `leaf_shadow_mask.pass` around `60-67us` on the tested RTX 3060 Ti path.
- No shader, Vulkan, panic, fatal, or error log entries were observed.
- Remaining warnings are pre-existing/non-blocking: multiple butterfly atlas files and short startup audio ring-buffer underruns.

## Progress Log

- 2026-06-03: Chose a separate 2D light-space opacity map plus low-res influence mask; deferred layered/deep opacity because grass receivers are the target and the deep path is much more expensive.
- 2026-06-03: Added GUI controls for leaf opacity strength/filtering and integrated leaf opacity with tracer direct lighting so leaf shadow affects terrain/voxel rendering, not only flora receivers.
- 2026-06-03: Added approximate light-space depth gating to leaf opacity sampling so tree-leaf/apple self-shadow direction follows the sun direction instead of behaving like a pure 2D projection.
- 2026-06-03: Enabled leaf-shadow receiving for tree leaves and apples so canopy/fruit surfaces are shadowed by leaf/apple opacity.
- 2026-06-03: Added light-space temporal blending for leaf opacity using current, previous, and blended opacity textures; mask and receivers sample the blended result.
- 2026-06-03: Added leaf shadow opacity/mask resources, render pass, mask compute pass, and receiver shader sampling. Runtime smoke passed with no shader/Vulkan errors.
- 2026-06-02: Confirmed current grass/flora VSM sampling is per voxel world position, not one shared blade position.
- 2026-06-02: Discussed that exact moving leaves in the main VSM cause high-frequency instability, blur/light bleeding, and temporal ghosting.
- 2026-06-02: Decided to pursue a separate moving-leaf opacity/transmittance shadow path instead of forcing PCSS on all receivers or freezing shadow casters.
- 2026-06-02: Created worker worktree `/Users/bytedance/code/re-flora-agent-leaf-shadow` on branch `agent/leaf-opacity-shadow` for this task.
- 2026-06-02: Added this progress document; no implementation changes yet.

## Open Questions / Risks

- The current implementation is a single-layer approximate opacity/depth map. If dense canopy self-shadow still looks wrong, move to layered/deep opacity.
- Current resolution is full shadow-map opacity plus 1/8-resolution mask. If it is too sharp/expensive, tune opacity resolution or PCF taps.
- Current accumulation uses raster alpha blending. If opacity ordering or saturation becomes a problem, consider storage-image atomics or a compute accumulation pass.
- Current receiver skip mask is light-space only. World chunk metadata may be worth adding if receiver cost remains high.
- Current filtering is a receiver-side conservative max filter with GUI-controlled radius. More blur may soften leaf shadows too much.
- Low temporal alpha can create visible leaf-shadow trails under strong wind; current tuned default is `0.4` after visual adjustment from the flickerier `0.9` default.
- Need visual confirmation that removing leaves from main VSM does not regress non-flora receivers that previously depended on leaf depth in `shadow_map_tex`.
