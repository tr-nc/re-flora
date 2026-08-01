# DDGI Convergence Calibration

This document independently records how the committed DDGI convergence policy is qualified. It is
intentionally separate from the broader transport acceptance narrative so the policy provenance can
be reviewed and regenerated without changing functional donor, dogleg, visibility, or lifecycle
gates.

## Policy

- maximum absolute RGB delta: `0.0025`;
- maximum relative RGB delta: `0.02`;
- required consecutive passing full-volume iterations: `2`;
- hard maximum feedback iteration: `8`.

The policy is qualified by complete convergence curves for `sealed`, `portal`, `donor`, and
`dogleg`, each at probe spacing 32 and 16. A curve contains every full-atlas validation from S0 to
its terminal converged iteration. Every validation must cover all valid 8x8 interior texels and all
corresponding 10x10 stored texels, contain no non-finite or negative RGB values, and report the exact
committed policy above.

## Reproduction and machine-readable evidence

Run:

```bash
scripts/check_ddgi_transport_acceptance.sh
```

Alongside the per-stage `.rfirr`, `.analysis.json`, and `.console.log` files, the runner writes
`target/ddgi-transport-acceptance/<run-id>/convergence-calibration.json`. The JSON records the policy,
the complete eight-curve matrix, every per-iteration absolute/relative delta, final threshold
margins, consecutive-pass count, hard-max headroom, and source artifact names. It is emitted only if
all curves independently validate.

The observed calibration table below is filled from a full release acceptance run on the commit
that introduced this evidence. Regenerate the JSON after any transport, scene, spacing, or shader
change; the generated run artifact remains authoritative.

| Case | Spacing | Final iteration | Max absolute delta | Absolute margin | Max relative delta | Relative margin | Hard-max headroom |
|---|---:|---:|---:|---:|---:|---:|---:|
| sealed | 32 | S5 | 0.00042211 | 0.00207789 | 0.00350526 | 0.01649474 | 3 |
| portal | 32 | S5 | 0.00042211 | 0.00207789 | 0.00350526 | 0.01649474 | 3 |
| donor | 32 | S6 | 0.00016218 | 0.00233782 | 0.00136243 | 0.01863757 | 2 |
| dogleg | 32 | S5 | 0.00014585 | 0.00235415 | 0.00282001 | 0.01717999 | 3 |
| sealed | 16 | S6 | 0.00019798 | 0.00230202 | 0.00221997 | 0.01778003 | 2 |
| portal | 16 | S6 | 0.00019798 | 0.00230202 | 0.00221997 | 0.01778003 | 2 |
| donor | 16 | S6 | 0.00022581 | 0.00227419 | 0.00228004 | 0.01771996 | 2 |
| dogleg | 16 | S5 | 0.00020987 | 0.00229013 | 0.00410989 | 0.01589011 | 3 |

All eight curves reached two consecutive passing iterations at least two iterations before the S8
hard maximum. The tightest observed absolute margin was `0.00207789`; the tightest relative margin
was `0.01589011`. The source artifact for this table is
`target/ddgi-transport-acceptance/20260801T130318Z-1006985/convergence-calibration.json`.
