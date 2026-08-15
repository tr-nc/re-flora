# DDGI Transport Acceptance

`scripts/check_ddgi_transport_acceptance.sh` is the top-level hidden release-mode acceptance
runner for the transport specification. It runs the stage-specific transport captures and
convergence summarizer first, then the portal/walls correctness runner, runtime terrain-edit runner,
radiance/density lifecycle runner, and committed sky-normalization evidence checker. Every
subordinate check is mandatory; a missing checker or runner is an acceptance failure.

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
legitimately differ between independent processes; the dedicated stale-active terrain-refresh
scenario adds `--compare-direct-light` and requires all three planes to match.

The required matrix covers spacing 32 and 16, sealed S0/S1/S2/converged,
portal S1 plus converged, donor S0/S1/converged, dogleg S1/S2/converged, and donor S1 in both
forward and reverse batch order. The eight complete convergence curves are summarized separately in
`docs/ddgi_convergence_calibration.md`. `NonConverged` remains a valid diagnostic capture state, but
analyzer `--correctness` rejects it.

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

## Sky-normalization presentation parity

`scripts/check_ddgi_sky_normalization_evidence.py` validates the committed machine-readable evidence
in `docs/evidence/ddgi_sky_normalization.json`. The evidence compares release-mode portal captures
from the adjacent pre-transport commits immediately before and after the `E/pi` normalization at
spacing 32 and 16, with identical authored camera, time, and voxel variance. Both hit masks match;
the observed maximum RGB channel error is `3.58e-7` and the maximum luminance error is `1.18e-7`,
below the committed `1e-6` limits.

## Stale-active terrain-refresh and independent direct-sun evidence

Capture v5's third float4 plane is the actual raster terrain `directLighting` term after albedo,
cosine, and VSM/leaf/cloud shadowing, but before it is added to the DDGI environment term. It never
samples a DDGI atlas. The in-flight terrain-edit acceptance capture requires all of the following at
both spacings while the latest terrain staging field is still rebuilding:

- the capture identity remains the older resident active token and geometry revision;
- environment irradiance p99 is at least `0.10`, finite, nonnegative, and repeated captures are
  bit-exact;
- logs prove only one staging update exists at a time and an obsolete candidate is discarded before
  the latest queued edit starts;
- sunlit ROI mean direct-light luminance at least `0.15` (observed `0.168975`, 11.2% margin);
- shadowed ROI maximum direct-light luminance exactly `0`;
- direct-light and environment terrain-hit masks match;
- all direct-light channels are finite and nonnegative.

The spacing 32 and 16 direct-light payloads were themselves bit-exact and contained 5,277 sunlit and
24,455 shadowed samples. The top-level runner therefore reports
`direct-sun-framebuffer=PROVEN seam=v5-direct-light-plane`.

## Dynamic radiance lifecycle evidence

The radiance lifecycle runner captures four v5 frames in one process at both required spacings:
the terminal baseline, the first rendered frame after R2, the first rendered frame after R4, and the
final R4 field. Identity sidecars prove `capture_frame = mutation_frame + 1`, that R2 remains the
immutable in-flight snapshot when R4 arrives, and that R3 is coalesced while the final active field
uses R4 from R2. On both spacings the old DDGI irradiance payload, world XYZ, and terrain-hit mask are
bit-exact during the immediate direct-light changes. The sunlit ROI changes from `0.168975` to
`0.303398` at R2 and `0.073722` at R4; both absolute deltas exceed the committed `0.02` gate.
