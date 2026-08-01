# DDGI Transport Acceptance

`scripts/check_ddgi_transport_acceptance.sh` is the top-level hidden release-mode acceptance
runner for the transport specification. It runs the stage-specific transport captures first, then
the existing portal/walls correctness runner, runtime terrain-edit runner, and the radiance/density
lifecycle runner when that script is present.

## Evidence

Every stage capture writes three adjacent artifacts below
`target/ddgi-transport-acceptance/<run-id>/`:

- `<case>-spacing<spacing>-<stage>-<order>.rfirr` is the v4 pre-albedo capture;
- the matching `.analysis.json` records capture identity, ROI measurements, and convergence deltas;
- the matching `.console.log` records every full-atlas validation reached on the way to the requested
  stage. The `[DDGI] full-atlas validated` records are the complete per-iteration convergence curve,
  including absolute and relative delta.

The required matrix covers spacing 32 and 16, sealed S0/S1/S2/converged, donor S0/S1, dogleg S1/S2,
and donor S1 in both forward and reverse batch order. `NonConverged` remains a valid diagnostic
capture state, but analyzer `--correctness` rejects it.

## Threshold provenance

The convergence limits are exactly the current centralized `DDGI_CONVERGENCE_POLICY` values:

- maximum absolute RGB delta: `0.0025`;
- maximum relative RGB delta: `0.02`;
- convergence requires the runtime policy's two consecutive passing iterations.

These limits are not widened by the runner. The existing portal/walls exact-reference limits also
remain owned by `scripts/check_ddgi_correctness.sh`: walls p99 is `0.15` at spacing 32 and `0.133` at
spacing 16.

Donor and dogleg signal thresholds must be calibrated after the exact voxel visibility hard gate is
merged. The runner intentionally has no broad defaults and fails before building unless all five
values are supplied:

```text
DDGI_DONOR_MAX_S0_RED_ADVANTAGE
DDGI_DONOR_MIN_S1_RED_ADVANTAGE
DDGI_DONOR_MIN_S1_LUMINANCE_MEAN
DDGI_DOGLEG_MAX_S1_LUMINANCE_MEAN
DDGI_DOGLEG_MIN_S2_LUMINANCE_GAIN
```

Calibration must use the committed deterministic camera, authored palette, ROIs printed by the test
scene, both required spacings, and the analyzer JSON emitted for the exact requested field identity.
Choose limits from the tighter of the two spacings with a documented numerical margin; do not tune
around a walls/roof leak or a `NonConverged` result. `--dry-run` prints every capture and analyzer
command without requiring provisional values.

## Remaining direct-sun evidence seam

The `.rfirr` payload records pre-albedo environment irradiance and exact direct-sun visibility. It
does not record the visible renderer's final direct-light RGB, so it can prove that the receiver is
shadowed but cannot prove that independent direct sun stays lit elsewhere while DDGI is fail-closed.

The minimal follow-up is a v5 optional third float4 plane containing visible-surface direct-light RGB
and the terrain hit mask, written before environment/direct composition. The analyzer can then require
strict-zero environment RGB inside the invalidated domain while requiring positive direct-light
luminance in a separate sunlit ROI. Until that seam exists, the runner reports
`direct-sun-framebuffer=UNPROVEN` and does not claim this evidence.
