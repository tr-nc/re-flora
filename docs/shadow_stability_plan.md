# Shadow Stability Plan

## Problem

The sun advances at a fixed cadence, and each sun update can trigger a shadow map update. Because the current shadow map resolution is limited, high-frequency receivers such as grass show large frame-to-frame shadow changes and visible flicker.

This plan targets shadow stability before adding cascaded shadow maps. CSM remains useful for resolution distribution, but it should come after the single-map path is stable.

## Scope

In scope for this optimization pass:

1. Fix VSM sampling/filtering behavior so filtered shadow moments are interpolated as intended.
2. Stabilize the directional-light shadow matrix with a fixed-size bounding volume.
3. Add dual shadow maps and temporally fade between old and new shadow results.
4. Decouple visual sun updates from shadow-map updates.
5. Apply a low-pass treatment to grass shadowing.

Out of scope for this pass:

- Cascaded shadow maps.
- Major renderer architecture rewrites.
- Changing generated files by hand.

## Step 1: Fix VSM sampling/filtering

Goal: avoid texel-level jumps caused by nearest sampling of an already-filtered VSM texture before changing shadow projection behavior.

Implementation outline:

- Audit sampler setup for `shadow_map_tex_for_vsm_ping` and related VSM textures.
- Prefer linear sampling for filtered VSM moments when the texture format/platform supports it.
- If linear sampling of the chosen format is not reliable across platforms, implement manual bilinear sampling with `texelFetch` in `shader/include/vsm.glsl`.
- Keep storage-image writes unchanged unless a format change is required.
- If considering `RGBA16F`, benchmark/validate before switching from current `RGBA32F`.

Likely files:

- `src/tracer/resources.rs`
- `shader/include/vsm.glsl`

Validation:

- `cargo fmt --check`
- `cargo check`
- Release hidden run.
- Verify grass/tree shadow edges move smoothly instead of snapping one texel at a time.

Commit checkpoint after this step:

```bash
git commit -am "smooth vsm shadow sampling"
```

## Step 2: Stable fixed-size shadow bounds

Goal: prevent the shadow projection from changing size or drifting sub-texel every update.

Implementation outline:

- Replace tight per-update light-space AABB fitting with a fixed-size bounding sphere/diameter for the shadowed region.
- Use a stable orthographic width/height based on the sphere diameter, not the current light-space AABB extents.
- Snap the light-space shadow center to shadow-map texel increments:

```text
world_units_per_texel = shadow_world_diameter / shadow_map_resolution
snapped_center.xy = floor(center_light_space.xy / world_units_per_texel) * world_units_per_texel
```

- Keep near/far handling conservative enough that existing terrain and tree shadow casters are not clipped.

Likely files:

- `src/gameplay/camera/shadow.rs`
- `src/tracer/mod.rs` if shadow map resolution needs to be passed into matrix calculation

Validation:

- `cargo fmt --check`
- `cargo check`
- Hidden release run and log check if practical:

```bash
source ~/.zshrc
cargo run --release -- --hidden --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Commit checkpoint after this step:

```bash
git commit -am "stabilize shadow projection bounds"
```

## Step 3: Dual shadow maps with temporal fade

Goal: when the shadow map changes, blend from the previous shadow solution to the new one instead of switching in one frame.

Implementation outline:

- Keep two filtered VSM outputs:
  - previous shadow map + previous shadow matrix
  - current shadow map + current shadow matrix
- On a shadow update:
  - preserve the old filtered result and old matrix;
  - render/filter the new shadow map into the other set;
  - start a blend timer from 0 to 1.
- In shadow sampling, evaluate both maps and blend visibility:

```glsl
float old_vis = sample_vsm(old_shadow_map, old_shadow_matrix, world_pos);
float new_vis = sample_vsm(new_shadow_map, new_shadow_matrix, world_pos);
float vis = mix(old_vis, new_vis, smoothstep(0.0, 1.0, shadow_blend));
```

- Start with a short configurable or constant fade duration, e.g. `0.15s` to `0.4s`.

Likely files:

- `src/tracer/resources.rs`
- `src/tracer/mod.rs`
- `src/tracer/buffer_updater.rs`
- `shader/include/vsm.glsl`
- foliage/particle shaders that sample VSM

Validation:

- `cargo fmt --check`
- `cargo check`
- Hidden release run and log check.
- Compare shadow update moments visually or through screenshots/video if available.

Commit checkpoint after this step:

```bash
git commit -am "fade between shadow map updates"
```

## Step 4: Decouple visual sun and shadow sun cadence

Goal: allow the visible sky/lighting sun to update smoothly while the shadow map updates on its own slower cadence without sampling old shadow data through a new matrix.

Implementation outline:

- Track separate sun state for rendering shadows:
  - visual sun direction: used for sky, direct lighting color/direction, lens flare, etc.
  - shadow sun direction/matrix: used only for shadow map rendering and shadow lookup.
- Only update `shadow_camera_info` when the shadow map itself is updated.
- If the shadow map update is skipped, keep using the previous shadow matrix and previous shadow sun direction for all shadow lookups.
- With Step 3 in place, shadow sun changes can be faded over time.

Likely files:

- `src/app/core/mod.rs`
- `src/tracer/mod.rs`
- `src/tracer/buffer_updater.rs`
- generated GPU structs only via `cargo check` if shader uniform layouts change

Validation:

- Toggle or speed up day/night cycle and confirm the sun/sky can move without per-frame shadow jitter.
- Confirm no frame uses a new shadow matrix with an old shadow texture.
- `cargo fmt --check`
- `cargo check`

Commit checkpoint after this step:

```bash
git commit -am "decouple shadow sun updates"
```

## Step 5: Low-pass grass shadowing

Goal: grass should not amplify high-frequency shadow-map changes.

Implementation options, ordered from least invasive to more involved:

1. Reduce grass shadow strength:

```glsl
grass_shadow = mix(1.0, grass_shadow, strength); // strength around 0.4..0.7
```

2. Increase grass minimum light/shadow floor.
3. Sample shadow once per grass instance or anchor position instead of per voxel/vertex.
4. Use a wider effective VSM filter for grass than for solid terrain/tree receivers.

Likely files:

- `shader/foliage/flora_common.glsl`
- `shader/foliage/flora.vert`
- `shader/foliage/flora_lod.vert`
- possibly particle foliage shaders if they share the same issue

Validation:

- Inspect dense grass areas during sun/shadow updates.
- Confirm grass still grounds visually and does not become uniformly flat.
- `cargo check`

Commit checkpoint after this step:

```bash
git commit -am "soften grass shadow response"
```

## Future plan: Cascaded shadow maps

CSM is deferred until the single-shadow-map path is stable. If added later:

- Use stable sphere-fit bounds per cascade.
- Snap each cascade to its own texel grid.
- Cross-fade between cascades to hide seams.
- Reuse the dual-map temporal fade for cascade updates.
- Keep cascade split distances stable unless a later adaptive system has explicit temporal smoothing.

## References

- Microsoft: Common Techniques to Improve Shadow Depth Maps  
  https://learn.microsoft.com/en-us/windows/win32/dxtecharts/common-techniques-to-improve-shadow-depth-maps
- Microsoft: Cascaded Shadow Maps  
  https://learn.microsoft.com/en-us/windows/win32/dxtecharts/cascaded-shadow-maps
- MJP: A Sampling of Shadow Techniques  
  https://therealmjp.github.io/posts/shadow-maps/
- Long Forgotten Blog: Stable Cascaded Shadow Maps  
  http://longforgottenblog.blogspot.com/2014/12/rendering-post-stable-cascaded-shadow.html
- Michal Valient, GDC09 Killzone 2 rendering talk  
  https://www.guerrilla-games.com/media/News/Files/GDC09_Valient_Rendering_Technology_Of_Killzone_2.pdf
