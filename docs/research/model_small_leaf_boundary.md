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

## Correction and bounded proof

Two interfaces were considered: constrain/reimplement division in the separate
reference adapter, or observe vertices at the actual coverage-loop seam. The
first cannot promise identical compiler context and risks changing supporting
materials again. The second is implemented with a specialized projection
observer: normal consumers use a no-op; native mode 1 writes the existing fifth
slab from the actual per-fan XY/depth locals. One designated invocation per
instance writes all triangles, even if its center is occupied; it never changes
that center hit. Offscreen triangles publish empty topology. Mode 2 now only
exports the center/ray references. The explicit push field carries the evidence
slab address; there is no new production allocation or readback.

The original coverage predicate, f64 planner, 0.0001px epsilon, CPU-derived depth
oracle, projection/frame/source checks and center RGBA preservation are unchanged.
The original per-fan division, precise XY, depth dot/saturation and supporting
triangle ordering remain. This is a correction to diagnostic geometry identity,
not a production missing-pixel repair or a new numerical tolerance.

- `fixed-small-green/`, `fixed64-green/`: both uninstrumented native regressions
  pass. The 64px complete final/center tiles also match the original v3 capture
  byte-for-byte (`observer-fixed64/rgba-comparison.log`).
- Small-case separate-run captures had identical masks/depth but different RGB,
  including center samples; they are **not** color-equivalence proof. A temporary
  same-dispatch comparison invoked the original coverage loop with the identical
  mesh/pose/camera/lighting. At both captured poses **every final RGB/depth lane
  and supporting triangle matches** (`preservation-comparison.txt`,
  `preservation-{small,fixed64}-2/pixels.bin`). The earlier two comparison attempts
  failed shader compilation (private helper visibility), not a numerical test.
- `negative-missing-pixel/` deletes a real center pixel and the unchanged center
  preservation check rejects it. `negative-coverage-only/` deletes center-empty
  covered (6,9); the unchanged coverage assertion rejects it with
  `GPU omitted conservative coverage: instance=0 pixel=6,9`. Both patches are
  archived; neither is in source. v3's (12,26) negative control is retained.
- Focused Rust: `cargo test butterfly_mesh` (18 pass), `cargo test model_pixel`
  (24 pass, one existing ignored). The small-case test crosses the actual evidence
  decoder and planner, including nearby shifted geometry, both windings, real
  covered neighbors and wrong projection/depth/source rejection. The old 64px
  test remains intact. Slang projection/coverage executable passes both captured
  cases and neighboring shifts/windings. Failed intermediate compilation logs
  are retained, followed by `test-*-2.log` successes.
- `cargo check`, `cargo fmt --check` and Release hidden/mute smoke pass. No
  generated files changed. All temporary shader probes and CPU pose generator
  were archived and removed. Broader deterministic phases and final live gates
  follow separately; this is not final acceptance.

## Deterministic pose coverage

`node scripts/validate-model-coverage-poses.mjs` (same enclosing GPU lock and
Release/hidden/mute policy) passes **906 unscreened cases in 15 batches**:
both exact captures and ±1/4/16-ULP translations on each axis, plus full rotations
around three axes at 16 phases × 8/16/64px × 0.25/1/4 sizes. The runner checks the
entire ordered batch sequence, nonzero independent numerical checks, and actual
completion of the final 10-instance batch. It does not change the live runner,
simulation cadence, saved settings or normal play. Every phase is checked;
there is no retry/filtering of troublesome poses. Runtime wall-clock timing
cannot select the sample set. `poses-1/`, `pose-check.log`, `pose-fmt.log` and
`test-pose-sweep.log` retain the passing explicit native and fast pure guardrails.

The deterministic sweep now also **requires nonzero checked samples for every
instance**, not just a nonzero batch total or repeated last batch. All 906 cases
pass this stronger gate (`poses-nonempty/`). Focused Rust (19 butterfly tests),
fmt/check and hidden smoke pass again (`nonempty-*.log`, `nonempty-smoke/`).

## Final worker gates (not controller acceptance)

All GPU commands used an enclosing, non-nested
`flock --close /tmp/re-flora-summer-gpu.lock env -u WAYLAND_DISPLAY CARGO_BUILD_JOBS=4`.
Every invocation used the worker's own target and existing shared sccache.

- **Unchanged strict live leaf:** `node scripts/validate-leaf-model.mjs --seconds
  20`, three consecutive independent processes on the final implementation:
  `strict-final-{1,2,3}/`, PIDs 861883/861982/862785, all 5120×2880, complete
  8/16/64px and 0.25/4 size sweep, 22/23/22 nonzero checks, no ERROR/VUID/panic.
  First B64 hits: 1587/1581/1587. `strict-final-summary.json` records identities.
  The preceding `strict-leaf-{1,2,3}/` sequence also passed; none was discarded.
- **Both fixed regressions:** the final fixture implementation passes in
  `final-fixed-small/` (24 hits) and `final-fixed64/` (103 hits). The latter's full
  final and center RGBA/depth still exactly equal the original v3 captured bytes
  (`rgba-comparison.log`), not merely its mask or four material witnesses.
- **Strict butterfly:** `python3 scripts/validate_butterfly_mesh.py --seconds 20`
  passes 45 checks across 21 models, 8/22/64px and all shadow/transmission stages
  (`butterfly-complete/`). The preceding unchanged 12-second attempt had no
  numerical/runtime errors but ended immediately after submitting transmission=1,
  before its completed readback/image capture; the runner correctly failed its
  missing-final-stage assertion (`butterfly/`). The documented longer duration
  completes that stage without changing any oracle or fixture setting.
- **Ordinary consumers:** `node scripts/validate-apple-model.mjs --seconds 12
  --stage-one` passes orthographic A/B, 8/16/37/128/512 views, mixed-resolution
  rotating leaves/butterflies, attached/fallen apples, 17 real fruit drops and
  native resize requests (`model-stage-one/`). As before, this protects consumers
  and lifetimes, not an apple-specific numerical color/depth oracle.
- **Release smoke:** `cargo run --release -- --hidden --mute --auto-exit 0.5`
  passes with successful exit/failures=0 (`final-smoke/`, `nonempty-smoke/`).
  Same-worktree latest-log helpers were used and logs inspected.
- `cargo fmt --check`, `CARGO_BUILD_JOBS=4 cargo check`, focused Rust tests
  (19 butterfly, 24 model-pixel + one existing ignored), and the explicit Slang
  executables `model_pixel_projection_evidence`, `leaf_particle_pose_contract`,
  `leaf_flutter_contract` pass. Slang command: `slangc shader/tests/<name>.slang
  -std 2025 -I shader/slang -target executable -o target/improve-delivery/v4/<name>`,
  then execute it. Logs: `final-*` and `nonempty-*`.

GUI/camera hashes match the initial files. Generated files and original live
strict runners are unchanged. All capture/legacy/negative probes are removed
from tracked source; raw attempts and patches remain under v4, v3 evidence is
retained. No unchanged full Cargo/DDGI matrix, performance benchmark, visible
launch, other worktree mutation, merge, push, release or cleanup was performed.
The controller owns independent review and integrated acceptance. Single-driver
native evidence and ordinary consumer coverage are not universal bit-exact
cross-driver or performance claims.
