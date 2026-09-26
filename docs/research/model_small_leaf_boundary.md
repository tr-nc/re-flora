# Small leaf projection counterexample (scope v4)

Base: `1ca15e592be2994ae8bec51c22c4c2b5cb2bc741`. This does not replace the
[v3 64px regression or its material correction](model_coverage_boundary.md).

## Reproduction

```sh
flock --close /tmp/re-flora-summer-gpu.lock env -u WAYLAND_DISPLAY \
  CARGO_BUILD_JOBS=4 node scripts/validate-model-coverage-boundary.mjs leaf-small-boundary
```

Before correction this exits 1: `GPU omitted conservative coverage:
instance=0 pixel=5,10`. The renderer-only fixture freezes one captured variant 0,
16px, size 0.0009765625 (the original live 0.25 size), quaternion and position.
The original review camera and geometry remain. The old default 64px fixture
still runs, including its supporting-material assertions. Neither changes saved
settings or the live strict runner.

New evidence is under `target/improve-delivery/v4/`. `controller-red.log` retains
the supplied failure. `capture-1` was incomplete because validating every frame
slowed the live sweep; `capture-2` passed. Recording actual published inputs and
batch replay (`scan-1/2`) did not reproduce. A temporary CPU generator then used
the real ParticleSystem at 120 Hz mechanics / 240 Hz diagnostic publication to
retain the intervening phases, with no simulation changes. Its stream includes
exactly the controller's frame-180 position (1.161259,1.3918549,1.4950721).
Scanning nearby variant-0 phases reproduced (5,10) at position
(1.1692841,1.3915664,1.4927582), then minimization to one instance remained red.
`red-capture/` contains raw camera, instances, geometry and all GPU slabs;
`fixture-red-3/` is the checked, uninstrumented deterministic regression.
`fixture-red/`, `fixture-red-2/` and `observer-small/` accidentally ran the old
64px fixture because the script did not forward its case argument; the latter
two correctly rejected the unexpected resolution. Those attempts are retained,
not counted as small-case evidence. The runner now forwards the case and output.

## Ranked hypotheses and discriminating evidence

1. Exported vertices differ from the actual coverage producer under a different
   compiler arithmetic context: **confirmed** by observing the actual coverage
   loop, not another projection call. Triangle 30 producer XY:
   (5.9973740578,11.0648431778), (5.9863524437,11.0205078125),
   (6.2672176361,10.6040945053). The separate reference export gives
   (5.9973263741,11.0648698807), (5.9863033295,11.0205249786),
   (6.2672500610,10.6040716171). Homogeneous clips and framing bounds are
   byte-identical (`clip-comparison.txt`); post-division coordinates are not.
   Merely adding `precise` to the diagnostic XY local did not fix this
   (`precise-probe/`), and is not shipped.
2. f64 vs f32 edge arithmetic on identical vertices: **disproved for this case**.
   On the exported triangle the critical signed margin is +0.0000073388302
   (f64) and +0.0000073313713 (f32), both covered (`arithmetic.txt`).
3. Size-switch frame/reference mismatch: fixed single-pose replay remains red;
   matching raw clips, bounds, camera and source identity exclude this cause.

Temporary captures/probes and generator patches are archived under `probes/`,
not production code. A correct diagnostic must observe the actual producer
arithmetic rather than assume a shared helper called elsewhere is bit-identical.
Do not change coverage epsilon, supporting-depth/material arithmetic, or trust
the producer's coverage decision as the oracle. Controller acceptance is pending.
