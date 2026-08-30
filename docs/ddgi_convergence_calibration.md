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

Three evidence seams were compared for this acceptance wire:

- Extending the Python source parser to every `std::io`, print, file, alias, helper, or FFI output
  path was rejected. It would be an incomplete Rust effect analyzer with unbounded false-negative
  and false-positive cases.
- Replacing logs with a private artifact writer could become a deep module if convergence evidence
  needs a first-class binary artifact. It is not the current seam: path binding, atomic write,
  flush/error semantics, and capture provenance would change the acceptance wire in a larger slice.
- The selected seam keeps the private canonical macro/log producer as a local structure tripwire
  and makes independently parsed console plus process-bound run log the runtime authority. This
  preserves the existing wire while rejecting malformed, extra, or stream-inconsistent evidence.

The runtime keeps convergence facts, exact evidence-line construction, and log emission inside a
private child module. Batch completion carries only an opaque, non-debuggable pending capability;
after Tracer's final fallible batch observation succeeds, its last batch-block statement consumes
that capability. Parent runtime code can prepare but cannot inspect or format the evidence, and
Tracer cannot reconstruct its count or terminal identity. Child-module Rust tests prove the exact
one-line Published and ordered two-line Converged results. The Python source tripwire dynamically
reads all `src/**/*.rs`; its global raw-identifier inventory permits only the runtime definition and
Tracer's canonical final direct commit call. Its child-module checks are deliberately limited to
the private capability, closed macro/log inventory, same-receiver consuming commit, and canonical
commit-last position.
Rustc owns the opaque types' non-`Debug`/non-`Display` and non-`Copy`/non-`Clone`/non-`Default`
proof through production-configuration, owner-local compile-time negative trait assertions. The
latter traits seal the pending capability against duplication or default fabrication. Each
assertion is an unconfigured module-root item adjacent to its owned struct, so an ordinary non-test
build enforces the seal. A source-level
trait parser was rejected because imports and aliases require Rust name resolution and generic
wrappers create false positives; representation marker fields would enlarge the production
interface, while language negative impls remain unstable. The source tripwire therefore checks
only the exact direct-item assertion placement and payload, including absolute paths to the macro
crate and `core::fmt` traits. Owner-local traits or modules therefore cannot shadow the rustc-owned
proof, and the tripwire does not duplicate Rust trait semantics.
Within the private child module's deliberately controlled Rust source grammar, the same tripwire
requires absolute `::log` paths, one canonical child-private log gate and sink, and the fixed
owner-local formatting/vector macros. It does not claim global sink or marker uniqueness and does
not inspect arbitrary parent `std::io`, print, file, alias, helper, or FFI output. Those effects are
outside the source tripwire and are rejected at the dual-stream runtime acceptance seam. This is a
local structure guard, not a general Rust name resolver, effect analyzer, or control-flow proof.
The private emitter checks that its dedicated target is enabled at Debug level before constructing
the evidence-line vector, so ordinary production logging does not pay that allocation cost.

The convergence summarizer independently parses both the capture console and its preserved,
process-bound `.run.log`, then requires semantic equality of the policy, ordered validation curve,
and terminal identity. Every physical line containing the convergence marker must be exactly one
canonical validation or terminal record. Before selecting the capture identity, every validation
record in each stream passes a cross-language wire mirror. The
`[validation_wire]` table in `config/ddgi_convergence_acceptance.toml` is its single field/type
owner: the Python parser derives all `u64`/`u32`/`f32` bounds and the decimal rounding cell from
that table, while the runtime formatter test compiles each mapped getter or fact against the
declared Rust type and checks the production formatter's decimal precision. This keeps the mirror
synchronized with `DdgiFieldKey`, `DdgiAtlasValidationStats`, the capsule's consecutive count, and
`DDGI_CONVERGENCE_POLICY` without a second Python type or formatting registry.

The mirror requires nonzero field serial/radiance revision/spacing and rejects Converged epoch
zero. Every numeric field must fit its Rust type; delta and threshold floats must round to a finite,
nonnegative `f32`. Validated atlas records require zero non-finite/negative counts, positive complete
8x8/10x10 coverage, and the runtime policy's thresholds and required consecutive count. Threshold
tokens must exactly equal the production `f32` policy rendered with the wire's eight decimal places;
nearby decimal values are not treated as equivalent. The initialization policy uses Rust's shortest
round-tripping `f32` display instead, so the parser compares its reconstructed `f32` exactly with the
top-level contract rather than applying the evidence format or a numeric tolerance. Across records,
field serials are globally unique and strictly increasing, identities are contiguous, and each
identity's epoch, consecutive-below count, and Converging/Converged state follow the production
classification state machine. A displayed delta is ambiguous only when its configured eight-place
decimal rounding cell crosses the actual Rust `f32` threshold; clearly above or below cells have one
possible streak outcome. The first process-bound validation record is the source-free initial
publication and therefore requires streak zero regardless of its deltas. Every later identity is
source-backed, so its epoch-zero streak follows the same threshold classification with previous
streak zero: clearly below requires one, clearly above requires zero, and only a true rounding-cell
crossing permits either. These checks apply to legal historical identities before capture selection,
not only to the selected curve. Terminal integer bounds are inherited by its mandatory exact match
to the final already-validated field identity. `max_rgb_value` is not in the evidence wire;
production atlas validation checks that private fact before the capsule can be constructed.
Consequently old-identity duplicates, raw stdout/stderr injection, direct run-log injection, and
synchronized duplicate records fail closed even when the source tripwire cannot see how bytes were
produced. Console-only or run-log-only evidence cannot qualify. It validates:

- one authoritative typed runtime-policy record per process, checked against the shared acceptance
  contract with no runner-owned epoch-count copy;
- legal, contiguous historical and captured identity sequences with unique ordered field serials;
- full valid/stored atlas coverage and production-possible convergence classification for every
  epoch;
- finite, nonnegative delta and policy values representable as production `f32` fields;
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
