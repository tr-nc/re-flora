# VSM Temporal History Plan

## Goals

- Render the shadow map every frame when shadows are enabled.
- Keep PCSS sampling the latest raw shadow map directly so terrain/object shadows stay immediate.
- Make grass/flora VSM shadows stable under high-frequency leaf/grass occluder changes.
- Keep terrain and other shadow-relevant world edits visually real-time for VSM by resetting history when edits occur.
- Expose the VSM temporal blend factor in the GUI for live tuning.
- Keep the first implementation small and measurable; do not introduce cascaded shadow maps or shadow-space reprojection yet.

## Current Problem

The old VSM temporal path blends final visibility between two filtered VSM maps:

```glsl
vis = mix(previous_visibility, current_visibility, alpha);
```

That path was designed for a low shadow-map update cadence. If the shadow map is rendered every frame, this old approach is not the right model because the blend alpha is reset around update events and the history is not an exponential accumulation.

For the new architecture, temporal filtering should happen on filtered EVSM/VSM moments, before grass/flora sample visibility:

```glsl
history = mix(history, current_filtered_moments, alpha);
```

## Target Frame Pipeline

```text
Every frame while shadows are enabled:

1. Render latest leaf/flora shadow depth into shadow_map_tex.
2. Trace latest terrain/scene shadow depth into shadow_map_tex.
3. PCSS samples shadow_map_tex directly.

VSM path:
4. Convert latest shadow_map_tex depth into EVSM moments.
5. Apply spatial VSM blur.
6. Alpha-blend current filtered moments into persistent VSM history.
7. Grass/flora sample the blended VSM history.
```

Expected texture flow, reusing the existing resources:

```text
shadow_map_tex                        latest raw depth, sampled by PCSS
shadow_map_tex_for_vsm_ping            current/final VSM output sampled by flora
shadow_map_tex_for_vsm_pong            spatial blur scratch
shadow_map_tex_for_vsm_prev            previous blended VSM history

vsm_creation:                          shadow_map_tex -> ping
vsm_blur_h:                            ping -> pong
vsm_blur_v_temporal:                   pong + prev_history -> ping_final
copy_history:                          ping_final -> prev_history
flora/leaves draw:                     sample ping_final
```

The vertical blur pass can absorb the temporal blend to avoid adding another output texture:

```glsl
vec4 current_filtered = vertical_blur(pong, uv);
vec4 history_prev = imageLoad(shadow_map_tex_for_vsm_prev, uv);
vec4 out_moments = reset_history ? current_filtered
                                 : mix(history_prev, current_filtered, alpha);
imageStore(shadow_map_tex_for_vsm_ping, uv, out_moments);
```

## Blend Factor Semantics

Add GUI parameter:

```text
Shadow / VSM Temporal Alpha
id: vsm_temporal_alpha
kind: float
range: 0.0 .. 1.0
default: 0.2
```

Meaning:

```text
1.0 = latest current filtered VSM only, temporal smoothing disabled
0.2 = 20% current frame, 80% history at 60 FPS
0.0 = freeze history, useful only for debugging
```

Use a frame-rate-adjusted alpha in Rust so the GUI value feels consistent across FPS:

```rust
effective_alpha = 1.0 - (1.0 - gui_alpha_60fps).powf(delta_seconds * 60.0);
```

Clamp the result to `0.0..=1.0`. If history is invalid or reset is requested, force effective alpha to `1.0` for that frame.

## History Reset Policy

Temporal VSM history should be reset whenever old VSM moments would be misleading. Reset means:

```text
out_moments = current_filtered_moments
history_valid = true after the frame
```

Reset triggers:

- First frame / history not initialized.
- Terrain edits or any operation that changes shadow-casting scene geometry.
- Tree regeneration or other procedural flora/tree rebuilds that affect shadow casters.
- Shadow map resolution or render extent changes.
- VSM blur radius changes, so GUI tuning responds immediately.
- Manual time-of-day/sun jumps or large shadow-camera matrix discontinuities.

Do not reset for normal wind/leaf animation; smoothing that high-frequency motion is the point of the VSM history.

Small automatic sun movement can initially be allowed to blend without reprojection. If visible smearing appears, add a sun-angle threshold reset before considering shadow-space reprojection.

## Expected Behavior

- PCSS remains sharp and current because it samples `shadow_map_tex` directly.
- Terrain/world edits are immediate for PCSS and immediate for VSM after a history reset.
- Grass/flora VSM shadows become a temporally low-passed version of the latest per-frame shadow map.
- Lower `vsm_temporal_alpha` increases stability but adds more lag/ghosting.
- Higher `vsm_temporal_alpha` reduces lag but allows more high-frequency flicker.
- `vsm_temporal_alpha = 1.0` should match the no-temporal VSM behavior except for the spatial blur setting.

## Implementation Checklist

### 1. Shadow update cadence

- [ ] Replace the low-frequency `update_shadow_map` decision with per-frame shadow rendering while `enable_shadows` is true.
- [ ] Keep PCSS consumers sampling the latest `shadow_map_tex` unchanged.
- [ ] Keep the shadow camera info synchronized with the raw shadow map used by PCSS.
- [ ] Retire or repurpose `shadow_map_update_pending`; it can become a VSM history reset request instead of a render request.

Likely files:

- `src/app/core/mod.rs`
- `src/tracer/mod.rs`

### 2. VSM temporal accumulation shader path

- [ ] Keep VSM creation as depth-to-EVSM-moments.
- [ ] Keep horizontal spatial blur.
- [ ] Extend vertical blur to read previous blended history and write final blended output to `shadow_map_tex_for_vsm_ping`.
- [ ] Add push constants for `blur_radius`, `temporal_alpha`, and `reset_history`.
- [ ] Clamp `blur_radius` to `0..=64` in shader.
- [ ] Ensure `blur_radius = 0` still works as current/no spatial blur.
- [ ] Copy final `shadow_map_tex_for_vsm_ping` to `shadow_map_tex_for_vsm_prev` after the temporal blend pass.
- [ ] Add required compute-to-compute and compute-to-graphics barriers.

Likely files:

- `shader/tracer/vsm_filtering.glsl`
- `shader/tracer/vsm_blur_h.comp`
- `shader/tracer/vsm_blur_v.comp`
- `src/tracer/mod.rs`

### 3. VSM sampling cleanup

- [ ] Remove the old previous/current visibility blend from `shader/include/vsm.glsl`.
- [ ] Keep `get_shadow_weight_vsm_temporal()` as a compatibility wrapper if needed, but make it sample only the final blended VSM texture.
- [ ] Eventually remove unused `shadow_camera_info_prev` / `shadow_temporal_info` shader bindings and Rust resources after verifying reflection/codegen impact.

Likely files:

- `shader/include/vsm.glsl`
- `shader/foliage/*.vert`
- `shader/particles/*.vert` if they share the same VSM path
- `src/tracer/resources.rs`
- `src/tracer/buffer_updater.rs`

### 4. GUI parameters

- [ ] Keep `vsm_blur_radius` as a GUI-adjustable uint `0..=64`.
- [ ] Add `vsm_temporal_alpha` as a GUI-adjustable float `0.0..=1.0`, default `0.2`.
- [ ] Pass `vsm_temporal_alpha` through `record_trace()` to the VSM filtering pass.
- [ ] Convert GUI alpha to frame-rate-adjusted effective alpha before pushing it to the shader.
- [ ] Changing `vsm_blur_radius` should request VSM history reset.
- [ ] Changing `vsm_temporal_alpha` should not reset history; it should take effect immediately through the next blend.

Likely files:

- `config/gui.toml`
- `src/app/core/mod.rs`
- `src/tracer/mod.rs`
- generated GUI files via `cargo check`

### 5. History reset integration

- [ ] Add a clear API/state flag for VSM history reset, for example `request_vsm_history_reset()` or a `reset_vsm_history` argument.
- [ ] Trigger reset on terrain edit operations.
- [ ] Trigger reset on tree/procedural shadow-caster rebuilds.
- [ ] Trigger reset on shadow map extent/resolution changes.
- [ ] Trigger reset on manual time-of-day jumps or large sun/shadow camera discontinuities.
- [ ] Confirm normal wind animation does not reset history.

Likely files:

- `src/app/core/mod.rs`
- `src/app/world_edits.rs`
- `src/app/world_ops.rs`
- `src/app/core/vegetation.rs`
- `src/tracer/mod.rs`

### 6. Validation

- [ ] `cargo fmt --check`
- [ ] `cargo check`
- [ ] `cargo test`
- [ ] `cargo run --release -- --hidden --auto-exit 0.5`
- [ ] Inspect latest log with `cargo run --release -- --tail-latest-log 200`.
- [ ] Visual check: PCSS shadows respond immediately to terrain edits.
- [ ] Visual check: grass/flora shadows are stable under leaf/wind motion.
- [ ] Visual check: changing `vsm_temporal_alpha` live has expected behavior.
- [ ] Visual check: changing `vsm_blur_radius` live resets VSM history and responds immediately.

## Risks / Follow-ups

- Texture-space VSM history is approximate when the shadow camera rotates with the sun. Start with reset thresholds before attempting reprojection.
- Very low alpha can leave visible ghosting after moving shadow casters; terrain/tree edits must reset history.
- Blending EVSM moments is preferable to blending raw depth, but aggressive EVSM exponents can still amplify numerical issues. Validate bright leaking/darkening at low alpha values.
- If combined vertical blur + temporal blend becomes hard to maintain, add a dedicated temporal compute pass and an additional output texture later.
