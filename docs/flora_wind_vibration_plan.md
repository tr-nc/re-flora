# Flora Wind Vibration Plan

## Goal

Improve flora wind motion tunability and tree leaf motion quality without changing the grass behavior that is already working well.

This plan tracks two related changes:

1. expose grass and leaf vibration controls through the GUI/uniform path;
2. move tree leaf wind bucketing from instance-level behavior to per-voxel behavior inside each leaf instance, with bucket-aware vibration timing.

## Current Model

The wind volume is stored as one 3D texture atlas split into several x-axis buckets. Conceptually, each bucket is a separate wind map. The app updates one wind bucket per world tick, then shaders render all flora every frame while sampling the bucket selected from a seed.

Current bucket semantics:

- grass uses an instance seed, so one grass instance samples one wind bucket;
- tree leaves already have shader access to `vox_local_pos`, so they can choose buckets per voxel rather than per instance;
- all flora still renders every frame; stale buckets simply preserve older wind values until their turn is updated again.

## Desired Behavior

### Grass

Do not change grass bucketing or motion behavior in this pass.

Grass should continue to behave as:

```text
grass instance seed -> wind bucket
whole grass instance samples that bucket
```

### Tree Leaves

Tree leaf instances should distribute their internal voxels across wind buckets:

```text
tree leaf instance seed + vox_local_pos -> wind bucket
```

With four buckets, the intended behavior is:

```text
tick 0: leaf voxels assigned to bucket 0 refresh/move
tick 1: leaf voxels assigned to bucket 1 refresh/move
tick 2: leaf voxels assigned to bucket 2 refresh/move
tick 3: leaf voxels assigned to bucket 3 refresh/move
tick 4: bucket 0 again
```

This means every tree leaf instance participates every frame, but only part of its voxels receive freshly updated wind/vibration state on a given world tick. After a full bucket cycle, all voxels in the instance have been refreshed once.

We intentionally want **per-voxel** assignment, not patch/cluster assignment. The visual target is small internal leaf variation rather than whole-instance paddling.

## Vibration Timing Model

Leaf vibration should follow the same bucket state as leaf wind sampling. This is the selected "方案 B" from design discussion.

```text
active bucket voxel:
    vibration time = current render time

inactive bucket voxel:
    vibration time = last time this voxel bucket stopped being active
```

This keeps inactive leaf voxels frozen instead of continuing to run their sine-based vibration every frame. When a bucket becomes active again, those voxels resume from current time.

Grass vibration can stay continuous and should not be converted to this bucket-aware timing unless a later tuning pass explicitly asks for it.

## GUI Parameters To Expose

The current shader constants are hard-coded and should be moved into GUI-adjustable uniform fields:

- grass vibration amplitude in voxels;
- grass vibration primary speed;
- grass vibration secondary speed;
- leaf paddle/vibration amplitude in voxels;
- leaf paddle/vibration primary speed;
- leaf paddle/vibration secondary speed.

Potential follow-up controls, if needed after testing:

- leaf active bucket count, if we ever want fewer/more than the wind volume bucket count;
- leaf vibration secondary harmonic weight;
- leaf shell/gradient weighting exponent;
- wind planar strength smoothstep min/max.

Start with the minimal six controls above.

## Implementation Outline

1. Add GUI config entries in `config/gui.toml`.
2. Add corresponding fields to `shader/include/gui_input.glsl`.
3. Thread values through:
   - `src/app/core/mod.rs`;
   - `src/tracer/mod.rs`;
   - `src/tracer/buffer_updater.rs`.
4. Replace hard-coded constants in `shader/foliage/flora_wind_motion.glsl` with `gui_input` fields.
5. Add tree-leaf-only per-voxel bucket selection.
   - Grass keeps instance-level bucket selection.
   - Tree leaf wind seed/bucket uses `instance_seed + vox_local_pos`.
6. Add bucket-aware leaf vibration time.
   - Active voxel bucket uses current `pc.time`.
   - Inactive voxel bucket uses the bucket's last active time.
7. Keep shadow and main leaf passes consistent.
8. Run `cargo check` to validate shaders and regenerate shader-derived Rust structs.

## Open Design Detail

The shader currently knows the sampled wind bucket through the seed-to-bucket function, but it does not directly know the app's current wind bucket step/time history unless we expose it. The implementation should choose one of these approaches:

- pass the current wind bucket index and bucket timing data through uniforms/push constants; or
- derive equivalent bucket timing from `pc.time`, world tick seconds, and bucket count in shader.

Implemented choice: derive bucket timing in shader from `pc.time`, `gui_input.world_tick_seconds`, and the wind volume bucket count. This avoids adding per-pass push-constant state while keeping leaf vibration aligned with the wind volume bucket cadence.

## Progress Checklist

- [x] Document plan and expected semantics.
- [x] Expose vibration controls in GUI config.
- [x] Add GUI uniform fields and Rust plumbing.
- [x] Replace hard-coded vibration constants.
- [x] Implement tree leaf per-voxel wind bucket selection without changing grass.
- [x] Implement bucket-aware leaf vibration timing.
- [x] Keep main and shadow leaf passes visually consistent.
- [x] Run `cargo check` and include generated outputs.
- [ ] Validate with a hidden release run if the change reaches runtime testing.

## Non-goals

- Do not change grass bucketing or grass visual behavior in this pass.
- Do not rewrite the wind volume update system.
- Do not hand-edit generated Rust files except as outputs from the normal build/check generation path.
