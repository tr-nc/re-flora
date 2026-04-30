# Flora Instance Storage Buffer Plan

## Context

The `hash` branch reduced flora instance data from 8 bytes to 4 bytes by removing the stored seed and deriving it from the instance position in the vertex shader.

Runtime testing showed that the per-vertex seed hash was not the main FPS problem. A constant seed did not improve performance. Restoring the instance vertex binding to an 8-byte stride with unused padding significantly improved FPS.

This suggests the regression comes from the 4-byte instance-rate vertex input stride/alignment path, not from the seed calculation itself.

## Goal

Test whether moving flora instance data from instance-rate vertex attributes to a storage buffer can recover the 4-byte-per-instance memory saving without regressing FPS.

## Proposed Experiment

Use a storage buffer for flora instances and fetch by `gl_InstanceIndex` in the vertex shader.

Keep the instance payload as:

```glsl
struct Instance {
    uint packed_local_pos;
};
```

In `flora.vert`, `flora_lod.vert`, and `leaves_shadow.vert`:

```glsl
layout(set = ..., binding = ...) readonly buffer B_Instances {
    Instance instances[];
};

uint in_instance_packed_local_pos = instances[gl_InstanceIndex].packed_local_pos;
```

Then stop binding flora instance buffers as vertex buffers for these pipelines.

## Expected Benefits

- True 4-byte flora instance records.
- Avoids the slow 4-byte instance-rate vertex attribute fetch path.
- Keeps seed derived from position, so no seed initialization/storage is needed.

## Risks

- Storage buffer loads may be slower than vertex input fetches on some GPUs.
- Pipeline descriptor/binding setup may need non-trivial refactoring.
- The same instance fetch path must work for regular flora, LOD flora, and leaves shadow rendering.

## Benchmark Cases

Compare these cases with the same scene and camera position:

1. `dev`: 8-byte vertex instance attributes, `packed_local_pos + seed`.
2. Current `hash`: 8-byte vertex instance attributes, `packed_local_pos + padding`.
3. Storage buffer experiment: 4-byte storage buffer records, `packed_local_pos` only.

Success criteria:

- FPS close to or better than current `hash` with 8-byte padding.
- Flora instance memory close to the original 4-byte `hash` layout.

If the storage-buffer path is slower, keep the aligned 8-byte vertex instance layout as the practical performance fix.

## Command-Line Benchmark Workflow

The app already supports enough CLI flags for a basic automated FPS run:

```bash
cargo run --release -- --windowed --auto-exit 20 --perf
```

Do not pass `--present-mode` by default. The app automatically chooses the best supported mode, preferring `MAILBOX`, then `FIFO`, then the first supported mode. On the current Linux/NVIDIA test surface, `IMMEDIATE` is not supported, so explicitly requesting it fails.

The benchmark output includes lines like:

```text
[PERF] 60.0 fps at frame 1020
[PERF] frame 1020 total 17.82ms egui 0.11ms gpu+present 16.60ms
```

Automation script idea:

```bash
./scripts/bench_fps.sh
```

The script should:

- Run `cargo run --release -- --windowed --auto-exit 20 --perf`.
- Capture stdout/stderr to a log file.
- Ignore startup/warmup samples from the first few seconds.
- Parse `[PERF]` FPS and `gpu+present` frame-time lines.
- Print min, average, median, and max for FPS and frame time.
- Exit nonzero if the run crashes or no usable perf samples are found.

Current limitation:

- The windowed run appears capped near 60 FPS on the tested setup, even with `MAILBOX`. This is still useful for smoke testing and large regressions, but small GPU-side wins may require a better benchmark path, such as offscreen rendering, a fixed-frame benchmark mode, or GPU timestamp queries that avoid present/display caps.
