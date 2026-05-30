# VSM temporal history

## Current goal

Render the shadow map every frame for immediate PCSS shadows, while temporally filtering VSM moments so grass/flora shadows are stable under high-frequency leaf and grass occluder changes.

PCSS samples the latest raw `shadow_map_tex` directly. Flora VSM samples a temporally accumulated filtered-moment texture.

## Frame pipeline

```text
1. Render latest leaf/flora shadow depth into shadow_map_tex.
2. Trace latest terrain/scene shadow depth into shadow_map_tex.
3. PCSS samples shadow_map_tex directly.

VSM path:
4. Convert latest shadow_map_tex depth into EVSM/VSM moments.
5. Apply horizontal spatial blur.
6. Apply vertical blur and temporal blend against previous history.
7. Copy final blended moments into previous-history texture.
8. Grass/flora sample the blended VSM output.
```

Texture flow:

```text
shadow_map_tex              latest raw depth, sampled by PCSS
shadow_map_tex_for_vsm_ping current/final VSM output, sampled by flora
shadow_map_tex_for_vsm_pong spatial blur scratch
shadow_map_tex_for_vsm_prev previous blended VSM history
```

The vertical blur pass owns the temporal blend:

```glsl
vec4 current_filtered = vertical_blur(pong, uv);
vec4 history_prev = imageLoad(shadow_map_tex_for_vsm_prev, uv);
vec4 out_moments = reset_history
    ? current_filtered
    : mix(history_prev, current_filtered, temporal_alpha);
```

## GUI parameters

- `vsm_blur_radius`: spatial blur radius, `0..=64`.
- `vsm_temporal_alpha`: 60-FPS reference blend alpha, `0.0..=1.0`, default `0.2`.

Alpha meaning:

```text
1.0 = latest current filtered VSM only, temporal smoothing disabled
0.2 = 20% current frame, 80% history at 60 FPS
0.0 = freeze history, useful only for debugging
```

Rust converts the GUI value to a frame-rate-adjusted alpha:

```rust
effective_alpha = 1.0 - (1.0 - gui_alpha_60fps).powf(delta_seconds * 60.0);
```

If history is invalid or reset is requested, the effective alpha is forced to `1.0` for that frame.

## History reset policy

Reset history when old VSM moments would be misleading:

- first frame / uninitialized history;
- terrain edits or shadow-casting scene geometry changes;
- tree/procedural shadow-caster rebuilds;
- VSM blur radius changes;
- manual time-of-day jumps or large sun/shadow-camera discontinuities;
- future shadow-map extent/resolution changes.

Do not reset for normal wind/leaf animation; smoothing that high-frequency motion is the point of the VSM history.

## Important behavior expectations

- PCSS shadows remain sharp and current.
- Terrain/world edits are immediate for PCSS and VSM after a history reset.
- Grass/flora shadows are a low-passed version of current per-frame shadow data.
- Lower `vsm_temporal_alpha` increases stability but adds lag/ghosting.
- `vsm_temporal_alpha = 1.0` should match no-temporal VSM apart from spatial blur.

## Key files

- `shader/tracer/vsm_filtering.glsl`
- `shader/tracer/vsm_blur_h.comp`
- `shader/tracer/vsm_blur_v.comp`
- `shader/include/vsm.glsl`
- `src/tracer/mod.rs`
- `src/tracer/resources.rs`
- `src/app/core/mod.rs`
- `config/gui.toml`

## Validation

Standard validation:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Visual checks still matter for this feature:

- PCSS responds immediately to terrain edits.
- Grass/flora shadows remain stable under wind/leaf motion.
- Changing `vsm_temporal_alpha` live has the expected stability/lag tradeoff.
- Changing `vsm_blur_radius` resets history and responds immediately.
