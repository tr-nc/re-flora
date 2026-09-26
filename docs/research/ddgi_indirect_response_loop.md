# DDGI indirect-response repair and measurement loop

## Baseline and measurement contract

The requested baseline is `4b8951aa`, on `agent/tree-edit-stall`; nothing here is merged into `main`.
Its executable is retained as `target/ddgi-response-loop/bin/baseline-4b8951aa`.
The measurement baseline adds instrumentation and `3f54cb80`, which permits a numerically valid,
complete black atlas. The original unconditional brightness assertion otherwise aborts an
intentionally unlit scene. That prerequisite changes no tracing, filtering, scheduling, or history
policy. Both comparison arms use it. Its Rust regression failed before the correction.

`indirect-response` measures **linear indirect irradiance**, not screenshot brightness or publication
counters. An opt-in GPU consumer calls the production `sampleDdgiTerrainSmoothEnvironment` at a
fixed floor receiver, `(0.65, 100/256 + 0.001, 1.2)`, normal `+Y`. Direct light, material multiplication,
exposure, and tone mapping are not added. Sun and sky lighting are zero. The ordinary scene-owner
isolation excludes automatic climbing edits. The sampler verifies request serial, position, normal,
and finite/nonnegative output. It records local probe support and the bound complete field's
identity separately from the consumer uniforms' possibly newer terrain/radiance revisions.
There is no dispatch or readback unless the fixture requests one; its small buffers and pipeline
are allocated at normal initialization.

The fixture waits for initial convergence, adds a real point light, executes 40 real terrain edits
at least 100 ms apart **without waiting for DDGI**, waits for convergence, removes the light, and
repeats 40 edits and settling. Edits alternate closing/reopening the skylight, returning to the same
geometry for each reference. A light is at `(0.65, 0.7, 1.2)`, RGB `(1, 0.4, 0.15)`, power `0.05`.
`indirect-response-static` is the matched fixed-geometry control: the same receiver/light timeline
and timed windows, with no terrain mutations. Its observations during those windows are also
eligible static reference samples.

The analyzer requires:

- All expected ordered phases, a completion marker, valid ordered readbacks, and local DDGI support.
- Forty actually advancing geometry edits per transition, or unchanged geometry in static control.
- A measurable on-minus-baseline signal; a black output cannot pass on advancing revisions alone.
- Last three **distinct complete fields** for each reference, with range below 10% of the signal.
- At least a 10% response during each editing window, not only after editing stops.
- Off-reference residual at most 5% of the on-minus-baseline signal.

The 10%/5% tolerances are explicit contracts for this controlled fixture, not universal game
brightness/latency budgets. First crossings of 10/50/90% are reported; a first crossing does not
prove sustained convergence. Three field samples are a guard against obviously drifting references,
not a ground-truth renderer. This tests one receiver and source transition, not every surface,
terrain-only illumination change, hardware configuration, or long gesture.

An early fixture stopped after eight epochs; it was rejected because its references were still
moving. The final fixture waits for the production convergence decision and the analyzer independently
checks reference stability and removal residual. It does not force additional renderer iterations
to make a failed decay look successful.

## Test validation before changing the response policy

- Ten deterministic Python tests reject black-but-advancing output, frozen lit output on removal,
  response only after editing, missing local support, invalid numbers/missing completion, repeated
  reference identities, unchanged edit revisions, and unstable references; a valid response and
  static control pass.
- Real Release static control, spacing 32: **passes**. First 10% on/off crossings are about 171 ms;
  first 90% crossings are 514/512 ms. The on reference is 0.06056, off residual 4.02%.
- Temporary GPU negative control: replace only the diagnostic shader's returned RGB with zero,
  preserving real field identities/support and the rest of the renderer. The native run exits
  cleanly, but the analyzer rejects it for no measurable indirect increase. The mutation was
  removed immediately; it is not a shipped flag or alternate production path. Executable/report:
  `bin/zero-query-mutant`, `negative-gpu-zero/` under the evidence root.
- The unmodified response policy fails both sustained-edit densities. This is a real negative
  control for publication-without-response, independent of synthetic analyzer tests.

All evidence is under `target/ddgi-response-loop/`, including binaries, exact per-invocation logs,
raw observations, reports, GUI/binary hashes, and the initial baseline manifest. Reports, rather
than timing numbers transcribed here, are the detailed record. Runs serialize on the shared GPU
lock, restore GUI/camera bytes, and use hidden/muted Release on the RTX 3060 Ti. GPU-lock contention
from other work caused interrupted preliminary attempts; those are not comparison evidence.

## Baseline measurements

Final instrumented baseline executable: `bin/baseline-instrumented`.

| Run | On 10% | On 90% | Off 10% | Off 90% | Final off residual | Result |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `baseline32-run1` | 6.245 s | 11.560 s | 6.241 s | 12.237 s | 9.67% | fails both during-edit responses and removal residual |
| `baseline16-run1` | 14.905 s | 58.823 s | 14.901 s | 59.947 s | 8.04% | fails both during-edit responses and removal residual |

References are stable under the stated fixture criterion, native exits are zero, and there are no
ERROR/panic/VUID messages. During editing the receiver remains **exactly unchanged**, even while
complete fields with newer identities publish. At spacing 32 the edited on-reference (0.06142)
is within about 1.5% of the independent static control, so it is not a different brightness target
invented to manufacture the failure.

## Reproduce

```sh
python3 -m unittest scripts.tests.test_check_ddgi_indirect_response
python3 scripts/check_ddgi_indirect_response.py target/response-static --static-control
python3 scripts/check_ddgi_indirect_response.py target/response32
python3 scripts/check_ddgi_indirect_response.py target/response16 --spacing 16
```

Use `--binary <release-executable>` to run a preserved arm without rebuilding. The default native
auto-exit deadline is 300 seconds to allow dense whole-volume reference convergence;
`--timeout-seconds` changes only the deadline. An incomplete run fails rather than relaxing the
response contract. Assets/configuration come from the current worktree.
