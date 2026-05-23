# Water / Particle Performance Report

## Summary

The poor particle-related performance is not caused by the app-level visual particle system or particle upload/rendering. The bottleneck is the water MLS-MPM simulation, which is currently run under the same `enable_particles` path and emits water debug particles.

A hidden perf run with additional breakdown logging was captured at:

```text
/tmp/re-flora-logs/re-flora-20260516-222738.348-107743.log
```

## Visual particle path

The app-level particle path is cheap, even with water debug snapshots appended:

```text
[PERF][PARTICLES] alive=13 snapshots=4109 water_debug=4096 emitters butterflies=1 leaves=36 tick_step=true dt=0.0439 total=0.148ms setup=0.001 emit=0.011 sim=0.001 collect=0.000 plan=0.016 snapshot=0.021 upload=0.098
```

Typical cost:

- Total app particle update/upload: ~0.14–0.18 ms/frame
- Upload: ~0.10 ms/frame
- Water debug snapshots: 4096

This is not the frame-rate bottleneck.

## Water MLS-MPM path

The expensive path is water simulation:

```text
[PERF][WATER] particles 4096 grid UVec3(32, 32, 32) nodes 32768 substeps 192 total 945.66ms avg 4.925ms/substep repair 224.87ms clear 4.78ms p2g 54.60ms grid 169.43ms g2p 491.93ms g2p_gather 80.69ms g2p_box 10.47ms g2p_terrain 319.42ms g2p_repair 19.97ms terrain_checks/substep 4096 active_nodes/substep 3164 particle_y 0.419..0.580 avg 0.503 terrain_sdf_min 0.0156 penetrating 0 no_sdf 0
```

Main findings:

1. G2P dominates total water time.
2. Most G2P time is terrain collision checks.
3. `repair_particles` is also very expensive and likely terrain-collision heavy.
4. Every substep checks all 4096 water particles against terrain.
5. The app-level particle upload/render path is negligible compared with water simulation.

Approximate reported costs over the profiling window:

| Phase | Time |
| --- | ---: |
| total water | 945.66 ms |
| avg/substep | 4.925 ms |
| repair | 224.87 ms |
| clear grid | 4.78 ms |
| P2G | 54.60 ms |
| grid update | 169.43 ms |
| G2P total | 491.93 ms |
| G2P gather | 80.69 ms |
| G2P box collision | 10.47 ms |
| G2P terrain collision | 319.42 ms |
| G2P repair | 19.97 ms |

## Likely optimization targets

1. Avoid terrain collision checks for particles that are far from terrain.
2. Reduce `TERRAIN_PARTICLE_COLLISION_ITERATIONS` from 8, or make it adaptive.
3. Add a broadphase / cache for terrain SDF presence by grid cell or chunk.
4. Avoid doing both `repair_particles()` terrain collision and G2P terrain collision every substep unless needed.
5. Consider separating water simulation enablement from visual particle rendering/debug particles so `--no-particles` does not have to imply water behavior.

## Profiling caveat

The fine-grained G2P breakdown currently uses per-particle `Instant` timing. This is useful for diagnosis under `--perf`, but it adds measurement overhead. After optimization, remove it or gate it behind a more explicit deep-profiling flag.
