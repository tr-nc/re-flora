# DDGI Transport Acceptance

`scripts/check_ddgi_transport_acceptance.sh` is the top-level hidden release-mode acceptance
runner for the transport specification. It runs the stage-specific transport captures first, then
the existing portal/walls correctness runner, runtime terrain-edit runner, and the radiance/density
lifecycle runner when that script is present.

## Evidence

Every stage capture writes three adjacent artifacts below
`target/ddgi-transport-acceptance/<run-id>/`:

- `<case>-spacing<spacing>-<stage>-<order>.rfirr` is the v5 capture: pre-albedo
  environment irradiance, world position plus exact sun visibility, and the raster terrain's
  independent direct-light RGB;
- the matching `.analysis.json` records capture identity, ROI measurements, and convergence deltas;
- the matching `.console.log` records every full-atlas validation reached on the way to the requested
  stage. The `[DDGI] full-atlas validated` records are the complete per-iteration convergence curve,
  including absolute and relative delta.

Analyzer comparisons report environment/world and direct-light bit exactness separately. General
DDGI determinism checks require the environment/world planes because temporal raster shadows may
legitimately differ between independent processes; the dedicated fail-closed scenario adds
`--compare-direct-light` and requires all three planes to match.

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

The exact-voxel hard-gate calibration used the committed deterministic camera, authored palette,
printed world-space ROIs, and exact requested field identities. The limits are committed in the
runner, so a normal correctness run cannot weaken them through environment variables:

| Signal | spacing 32 | spacing 16 | committed gate | margin from tighter observation |
|---|---:|---:|---:|---:|
| donor S0 red channel share | 0.036368 | 0.037327 | at most 0.05 | +0.012673 |
| donor S1 minus S0 red-share gain | 0.091683 | 0.083320 | at least 0.065 | 22.0% |
| donor S1 minus S0 luminance gain | 0.069264 | 0.057747 | at least 0.045 | 22.1% |
| dogleg S1 receiver luminance mean | 0.000013900 | 0.000010577 | at most 0.00002 | +43.9% |
| dogleg S2 minus S1 luminance gain | 0.000085190 | 0.000092319 | at least 0.00007 | 17.8% |

The donor gate uses red *share gain*, rather than absolute red-minus-blue advantage. The authored sky
is intentionally blue, so absolute blue remains larger at S1; the stable stage-boundary evidence is
that the red donor raises red's share from about 3.7% to 12–13% while also raising total irradiance.
This distinguishes transported donor energy from the unchanged sky seed without changing the game's
authored sky.

## Independent direct-sun evidence

Capture v5's third float4 plane is the actual raster terrain `directLighting` term after albedo,
cosine, and VSM/leaf/cloud shadowing, but before it is added to the DDGI environment term. It never
samples a DDGI atlas. The in-flight terrain-edit acceptance capture requires all of the following at
both spacings while the DDGI environment plane is strict zero:

- sunlit ROI mean direct-light luminance at least `0.15` (observed `0.168975`, 11.2% margin);
- shadowed ROI maximum direct-light luminance exactly `0`;
- direct-light and environment terrain-hit masks match;
- all direct-light channels are finite and nonnegative;
- repeated captures are bit-exact.

The spacing 32 and 16 direct-light payloads were themselves bit-exact and contained 5,277 sunlit and
24,455 shadowed samples. The top-level runner therefore reports
`direct-sun-framebuffer=PROVEN seam=v5-direct-light-plane`.
