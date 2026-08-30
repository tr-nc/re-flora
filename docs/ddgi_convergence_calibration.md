# DDGI Temporal Convergence Calibration

This document records the stopping policy and representative release evidence for the temporal DDGI
lifecycle. Generated run artifacts remain authoritative after any shader, scene, spacing, history,
or scheduler change.

## Policy

| Parameter | Value |
|---|---:|
| maximum absolute RGB delta | `0.0025` |
| maximum relative RGB delta | `0.02` |
| relative floor | `0.05` |
| minimum complete epochs | `8` |
| consecutive passing epochs | `2` |
| maximum complete epochs | `128` (`e0` through `e127`) |

The checked-in machine contract is
`config/ddgi_convergence_acceptance.toml`. Rust locks the runtime policy to that contract, while the
Python summarizer independently rejects a process whose logged policy drifts from it. This prevents
the producer and every acceptance process from moving together to a shorter budget unnoticed.

The maximum is a finite temporal sampling budget and sleep backstop. `Threshold` means both deltas
passed for the required consecutive epochs after the minimum age. `SampleBudget` means the latest
finite nonnegative field was retained and put to sleep at e127 even though its maximum texel delta did
not pass. The lifecycle state is `Converged` in both cases; the reason must be inspected when making
a quality claim.

## Historical 64-epoch baseline

The representative pre-change spacing-32 release captures from
`target/ddgi-temporal-final-full-rerun/20260816T171425Z-213621/` completed the full field and slept
at the finite sample budget:

| Scene | Final epoch | Reason | Max absolute delta | Max relative delta |
|---|---:|---|---:|---:|
| sealed | 63 | SampleBudget | 0.00553083 | 0.02123354 |
| portal | 63 | SampleBudget | 0.00553083 | 0.02791479 |
| donor | 63 | SampleBudget | 0.00544786 | 0.02720159 |
| dogleg | 63 | SampleBudget | 0.00493240 | 0.03324598 |

These results are finite and nonnegative but do not satisfy the maximum-delta thresholds. They
motivated the current 128-epoch budget; they are historical baseline evidence, not a calibration of
the new terminal epoch. A dense spacing-16 donor run likewise completed at e63 with max absolute delta
`0.00804699` and max relative delta `0.03064647`.

The old deterministic fixed-ray, zero-history S5/S6 calibration is not applicable to rotated
temporal sampling and has been removed from the current policy. Historical artifacts remain usable
only to audit the superseded implementation.

## Reproduction

Run:

```bash
scripts/check_ddgi_transport_acceptance.sh
```

The runtime keeps convergence facts, exact evidence-line construction, and log emission inside a
private child module. Batch completion carries only an opaque, non-debuggable pending capability;
after Tracer's final fallible batch observation succeeds, its last batch-block statement consumes
that capability. Parent runtime code can prepare but cannot inspect or format the evidence, and
Tracer cannot reconstruct its count or terminal identity. Child-module Rust tests prove the exact
one-line Published and ordered two-line Converged results. The Python source tripwire reads only
`src/ddgi/runtime.rs` and `src/tracer/mod.rs`; it is deliberately limited to the private capability,
single child log sink, same-receiver consuming commit, and canonical commit-last position.
The private emitter checks that its dedicated target is enabled at Debug level before constructing
the evidence-line vector, so ordinary production logging does not pay that allocation cost.

The convergence summarizer independently parses both the capture console and its preserved,
process-bound `.run.log`, then requires byte-semantic equality of the policy, ordered validation
curve, and terminal identity. Console-only evidence cannot qualify. It validates:

- one authoritative typed runtime-policy record per process, checked against the shared acceptance
  contract with no runner-owned epoch-count copy;
- one contiguous epoch sequence with unique ordered field serials for the capture's exact
  geometry/radiance/spacing identity;
- full valid/stored atlas coverage for every epoch;
- finite, nonnegative RGB values;
- the exact policy constants above;
- exactly one dedicated typed terminal record whose identity and epoch match the final full-atlas
  validation and capture, including the final field serial;
- terminal reason matching either the first valid threshold stop or e127 sample-budget stop.

## Known limitation and next experiment

The present delta is measured after temporal blending. A high history-retention value can reduce
post-blend delta without proving that new raw samples have low variance. The next calibration should
record pre-blend sample variability separately, then decide whether threshold sleep can be trusted
independently of the 128-epoch budget. Per-probe variability and adaptive sleep remain out of scope
for this implementation. The Cornell follow-up records the measured 64/128/256
quality tradeoff for the current policy
([follow-up](references/ddgi/cornell-box-grid-followup.md)).
