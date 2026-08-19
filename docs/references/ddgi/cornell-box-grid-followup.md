# Cornell-box DDGI grid follow-up

> Historical experiment record. The dedicated Cornell-box runtime scene and its CLI flag have
> since been removed; this note remains as provenance for the accepted DDGI visibility formula.

Experiment date: 2026-08-19

This follow-up supersedes the receiver-bias root-cause conclusion in
[`cornell-box-moment-visibility-grid-research.md`](cornell-box-moment-visibility-grid-research.md).
That first pass measured the Moment Visibility debug view. The user then
reported that the production Final view retained an obvious grid at both ends
of the bias slider. A fixed-camera Final A/B reproduced that observation:
changing receiver bias from `0` to `0.02` retained `98.69%` of the measured wall
grid energy. Receiver bias changes the wall color and Moment Visibility, but it
does not remove the production artifact.

## Root cause

The Final artifact had two separable contributors:

1. The squared same-side weight `max(0, normal dot probeDirection)^2`
   suppresses near-tangent probes aggressively. Neighboring probes therefore
   trade dominance at the projected cage scale even when Moment visibility is
   removed.
2. Per-probe Monte Carlo variance remains visible through the spatial basis.
   More epochs reduce that residual, but do not remove the structural pattern
   produced by the squared weight.

The following fixed `gd-ptn2` Cornell captures used the same 1920x1080 Final
wall ROI and the same band-pass statistic. Values are relative to the original
hard-weight, 64-epoch control; lower is better.

| Candidate | Retained Final grid energy | Decision |
| --- | ---: | --- |
| receiver bias `0.02`, otherwise unchanged | `98.69%` | rejected; wrong primary control |
| hard weight, 128 epochs, history `0.99` | `99.11%` | rejected as a structural fix |
| RTXGI-style wrap weight, 64 epochs | `63.74%` | quality pass, correctness fail |
| linear same-side weight, 128 epochs | `72.08%` | safe but weaker |
| square-root same-side weight, 64 epochs | `63.74%` | quality and correctness pass |
| square-root same-side weight, 128 epochs | `58.20%` | accepted |
| no surface-side weight, 64 epochs | `56.56%` | diagnostic only; leak-prone |

Widening the visibility filter from exponent `50` to `8` regressed the Final
metric. A `0.05` Chebyshev floor produced no measurable Final improvement.
Smoothstep position interpolation made the cells stronger. A three-dimensional
Halton rotation sequence was also slightly worse than the existing deterministic
uniform SO(3) rotations at the same 64-epoch budget. All four experiments were
removed.

## Accepted query change

The terrain query now uses:

```text
surface_side_weight = sqrt(max(0, normal dot direction_to_probe))
```

This is a project-calibrated conditioning curve, not a claim that it is the
paper's universal constant. Unlike wrap weighting, it remains exactly zero for
probes behind the receiver plane. It gives near-tangent probes enough weight to
avoid one probe dominating an entire cage, while preserving the existing
same-side rejection contract.

The rejected wrap candidate demonstrated why that distinction matters. It
reduced the Cornell grid, but portal Final-vs-Exact luminance-error p99 rose to
`0.02835` at spacing 32 and `0.01701` at spacing 16, both above the committed
`0.01` bound.

The accepted square-root candidate passed the same release capture gates:

| Case | Spacing | Final-vs-Exact p99 | Bound |
| --- | ---: | ---: | ---: |
| sealed | 32 | `0` | `0.00001` |
| sealed | 16 | `0` | `0.00001` |
| portal | 32 | `0.003857` | `0.01` |
| portal | 16 | `0.008058` | `0.01` |
| thin walls | 32 | `0.397023` | `0.40` |
| thin walls | 16 | `0.368565` | `0.375` |

The production-policy captures all terminated at epoch 127. Portal overestimate
p99 values were `0.000617` and `0.000139` at spacing 32 and 16 respectively.
Thus the accepted smoothing did not reproduce the wrap candidate's
behind-surface light admission.

## Temporal budget

The maximum convergence budget moves from 64 to 128 complete epochs and the
default history retention from `0.98` to `0.99`. At 128 epochs, `0.99` stays in
equal-weight running-average behavior for the first 100 epochs and approaches
the quality of the tested 256-epoch/`0.98` field with half its total work.

This doubles the worst-case background work per static field, but does not
increase rays per probe, atlas memory, batch size, or the cost of one rendered
frame's DDGI batch. The last complete Active field remains visible while the
new field converges. A 256-epoch candidate improved the Cornell statistic by
only another `7.4%` relative to the accepted 128-epoch/`0.99` result and was
rejected as a poor work-quality tradeoff.

The convergence label still means either threshold completion or exhaustion of
the finite sample budget. It must not be interpreted as a mathematical fixed
point when the terminal reason is `SampleBudget`.
