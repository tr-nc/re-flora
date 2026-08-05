# DDGI probe-layer seam research

Status: diagnosis only; no renderer change is proposed or implemented by this note

## Question and short answer

The saved-terrain reproduction is not enough to call the symptom an unavoidable DDGI
defect. DDGI does have documented limitations that make this kind of scene a strong
stress case: a sparse probe grid represents low-frequency irradiance, visibility is a
statistical approximation, and a small bright source can be badly represented between
probes. Those limitations are expected error bounds, not a license for a persistent
internal grid band.

The current evidence makes the strong sun an amplifier, not the root cause. The useful
working classification is:

- **generic DDGI limitation:** the roof opening creates high-frequency direct illumination
  while the probe field is sparse and low angular resolution; a coarse field can therefore
  miss or smear the beam and can show probe-scale error;
- **project-specific defect or bad parameterization:** an abrupt band that follows probe
  layers, survives after the hard projected-light edge is excluded, or appears when the
  interpolated probe values should be equal points to the transport, probe state, receiver
  coordinate, or query-weight implementation; and
- **not established yet:** the existing screenshots do not identify which of the probe
  irradiance atlas, visibility weights, relocation/support test, or direct-sun injection is
  responsible. The old direct-terrain-shadow/VSM receiver hypothesis must not be used as the
  explanation for the `exact-irradiance` image.

This is consistent with the user's observation: the exact-irradiance crop is the clearest
surface for the bands because it displays the DDGI probe-field result without albedo and
without the final direct VSM term.

## Primary-source findings

### What DDGI promises and what it does not

The original DDGI paper defines a surface query over the eight probes of the containing
cage. It combines trilinear position weights, backface/orientation rejection, a biased
visibility test from distance moments, and irradiance sampled in the surface-normal
direction. The method assumes that the incident light at the surface is similar to the
incident light represented by nearby probes when they are mutually visible. The paper
explicitly reports increased error at lower probe density and treats the representation as
low-resolution diffuse irradiance, not a high-frequency direct-light solution.

Sources: [Majercik et al., 2019, §5.2 and §6.1](https://jcgt.org/published/0008/02/01/paper-lowres.pdf#page=12),
[§7 limitations](https://jcgt.org/published/0008/02/01/paper-lowres.pdf#page=25).

The production DDGI paper retains the eight-probe query and adds relocation, probe states,
backface handling, and convergence heuristics. It documents that bias is a correctness
trade-off near surfaces, that relocation changes the spatial sample distribution, and that
small bright sources can leave indirect-light ghosting or require rapid convergence.

Source: [Majercik et al., 2021, §§2, 4–6, 8](https://jcgt.org/published/0010/02/01/paper-lowres.pdf).

NVIDIA's maintained RTXGI-DDGI algorithm description states the same design boundary in
implementation terms: probe irradiance and distance statistics plus an occlusion test reduce
light leaks, but the result is low-frequency global illumination and does not reproduce
high-frequency radiometric or geometric detail. Its query shader uses eight probes with
trilinear/backface weighting and Chebyshev visibility; the official source is a useful
reference for expected, continuous interpolation versus explicit visibility gates.

Sources: [RTXGI-DDGI Algorithms.md](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/main/docs/Algorithms.md),
[official Irradiance.hlsl](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/main/rtxgi-sdk/shaders/ddgi/Irradiance.hlsl),
[official ProbeBlendingCS.hlsl](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/main/rtxgi-sdk/shaders/ddgi/ProbeBlendingCS.hlsl).

The 2021 production-resampling paper is particularly relevant to the strong-sun setup. It
describes sparse-probe bias and probe-grid artifacts, and gives a failure case in which a
strongly sunlit planar wall exposes light-leak error. That is evidence that the test scene is
well chosen to amplify a known approximation; it is not evidence that every regular band is
correct behavior or that a project cannot have a bug.

Source: [Majercik et al., 2021, *Dynamic Diffuse Global Illumination Resampling*, §§1, 2, and failure cases](https://arxiv.org/abs/2108.05263).

Roháček's probe-improvement paper gives two additional mechanisms that can create cage-scale
artifacts: relocation can invalidate nominal trilinear assumptions at cage boundaries, and
coarse octahedral depth can classify a long, nearly parallel roof hit incorrectly. It is a
non-peer-reviewed implementation study, so it is supporting evidence rather than the
authority for a root-cause claim.

Source: [Roháček, 2022, §§3.3–3.4](https://cescg.org/wp-content/uploads/2022/04/Rohacek-Improving-Probes-in-Dynamic-Diffuse-Global-Illumination.pdf).

## What the current Re: Flora path actually does

The following is source-grounded observation, not a new hypothesis:

1. A probe traces 256 Fibonacci rays. For a front-facing terrain hit, the transport path
   computes a direct-sun term proportional to `sun_luminance * cosine * visibility`, then adds
   the previous transport field and multiplies by the voxel albedo. The direct-sun visibility
   origin and the DDGI transport receiver are currently derived from
   `result.center_position`, not the camera ray's exact hit. See
   [`ddgi_probe_trace.slang`](../../../shader/slang/ddgi_probe_trace.slang:80-123).
2. Each valid probe is filtered independently into an 8x8 octahedral irradiance tile from
   the 256 ray records. There is no cross-probe smoothing in that filter. See
   [`ddgi_irradiance_filter.slang`](../../../shader/slang/ddgi_irradiance_filter.slang:43-86).
3. The `exact-irradiance` debug view calls `sampleDdgiExactTerrainReference`, gathers the
   eight probes, forces full hard visibility for that debug query, and returns the resulting
   irradiance directly. The final `directLight`/VSM term is not added in the debug branch.
   See [`tracer.slang`](../../../shader/slang/tracer.slang:371) for the debug query and
   [`tracer.slang`](../../../shader/slang/tracer.slang:510) for the final/debug branch.
4. The query chooses the containing cell with `floor(worldPosition * gridScale)`, applies
   relocation-aware position weight and a squared surface-side weight, rejects invalid or
   out-of-support probes, optionally applies moment and/or exact voxel visibility, and then
   normalizes only the surviving weighted irradiance. A small surviving-weight sum can thus
   make one probe dominate a region. See [`ddgi_query.slang`](../../../shader/slang/ddgi_query.slang:270-348,../../../shader/slang/ddgi_query.slang:384-420).

These facts explain why the exact crop is diagnostically valuable and also why it must be
read carefully. `--ddgi-consumer-visibility none` changes the normal consumer query, but
`exact-irradiance` deliberately calls `getDdgiFullVisibilityProbeContribution`; therefore a
visibility-none run of that debug view is **not** an all-visibility-disabled experiment.
Normal-view parity and exact-view evidence answer different questions.

## Matched hidden-release isolation results

The following runs use the saved terrain, the single `snapshot` camera, the existing lighting
fixture, `--no-god-rays`, and a 7-second screenshot delay. They were run after the DDGI field
reported convergence; no terrain, camera, material, or lighting input was changed. The new
debug views are diagnostic-only and do not enter the Final path:

- `exact-irradiance` uses the existing exact reference query and forces full hard visibility.
- `unoccluded-irradiance` keeps the normal valid/relocated/support and surface-side gates but
  sets both consumer visibility terms to one.
- `equal-weight-irradiance` keeps the trustworthy/support gates but averages the eight probe
  irradiances without position or surface-side weights.
- `raw-cage-irradiance` averages the eight valid atlas tiles in the nominal cell, bypassing
  query weights, support, and visibility altogether.

The captured crops are under `target/ddgi-seam-repro/experiments/`:

| View | Crop | Log | Edge-excluded metric |
| --- | --- | --- | --- |
| exact irradiance | `baseline/exact-irradiance-crop.png` | `re-flora-20260806-014819.173-296921.log` (matched `exact-none` run) | RED, 24 internal bands, ratio 1.961422 |
| unoccluded irradiance | `unoccluded32/crop.png` | `re-flora-20260806-015224.226-299529.log` | RED, 24 internal bands, ratio 1.961422 |
| equal-weight irradiance | `equalweight32/crop.png` | `re-flora-20260806-015750.868-302798.log` | GREEN, 1 internal band, ratio 0.067848 |
| raw cage irradiance | `rawcage32/crop.png` | `re-flora-20260806-020646.703-308245.log` | GREEN, 1 internal band, ratio 0.089716 |

The exact view with `--ddgi-consumer-visibility none` is byte-identical to the full exact view
(crop RMSE `0`), as expected from its deliberate full-visibility reference call. The normal
`full` and `none` captures also have the same row-gradient peaks, so the consumer visibility
switch is not the source of the reported bands.

The reproducible metric is:

```bash
python3 scripts/analyze_saved_ddgi_seam.py \
    target/ddgi-seam-repro/experiments/baseline/exact-irradiance-crop.png
```

It finds the largest projected-light/geometry edge, excludes a 50-pixel neighborhood around
that edge, and counts separated internal horizontal gradient peaks. This is intentionally not
an edge-only score. The analyzer and its two synthetic PNG tests are in
[`scripts/analyze_saved_ddgi_seam.py`](../../../scripts/analyze_saved_ddgi_seam.py) and
[`scripts/tests/test_analyze_saved_ddgi_seam.py`](../../../scripts/tests/test_analyze_saved_ddgi_seam.py).

### Evidence classification

The `exact` and `unoccluded` images retain the same 24 internal bands, so moment/hard visibility
attenuation is not required to produce them. Removing the eight-probe position and surface-side
weights makes the internal-band metric GREEN, while removing all query gates with the raw cage
view remains GREEN. This localizes the symptom to the current spatial query weighting/gating
interaction with the relocated sparse field, rather than to a final VSM edge or an already-banded
per-probe atlas tile. It does **not** yet prove that the intended repair is a generic DDGI change:
relocation-aware weighting and support rejection are project-specific parameterization, and the
strong sun still amplifies the underlying sparse-field error. A next renderer change should first
instrument the eight contributions at pixels immediately above, inside, and below one measured
band, then test a continuity-preserving weight contract before changing transport or materials.

## Working diagnosis

The current symptom is best described as **a project-specific DDGI result that is exposed by
a generic sparse-field limitation**, not as a universal DDGI bug.

The strong sun raises the contrast of any difference between adjacent probes. A roof opening
also creates a sharp visibility transition, so probes just above and below a layer can receive
very different direct radiance. If the eight-probe query then removes some candidates through
hard visibility, surface-side weighting, relocation-aware support, or an invalid state, the
normalized result can show a probe-layer band. This can be a legitimate approximation error at
too-low density, but a repeatable band away from the true beam edge is a defect if it survives
an isolation that should make the interpolation continuous.

The old direct-terrain-shadow receiver change cannot explain the exact-irradiance symptom: that
debug branch returns the DDGI query result before adding final direct VSM lighting. It may affect
the normal final image, but it is not a diagnosis of the probe-field bands.

## Isolation plan before choosing a fix

Run these as matched hidden release captures from the saved-terrain fixture; do not alter the
accepted camera, terrain, or normal lighting while collecting the baseline.

1. **Raw probe irradiance versus query weights.** Add a diagnostic capture that displays the
   per-probe irradiance with visibility forced to one. If the bands remain, inspect probe trace,
   direct-sun injection, ray classification, atlas filtering, and probe publication. If they
   disappear, hold irradiance constant and display only normalized visibility weights and their
   unnormalized sum.
2. **Consumer visibility A/B.** Compare normal `full`, `moment-only`, `exact-only`, and `none`
   modes. Separately compare exact-irradiance with a debug path that does not force full
   visibility. Record the eight probe indices, validity, actual/nominal positions, base weight,
   hard visibility, moment visibility, irradiance, and final weight at pixels above, inside, and
   below one band.
3. **Spacing A/B.** Repeat at probe spacing 32, 16, and 8 voxels without changing ray count.
   A band whose world-space width follows the cell size is an under-resolved field or an
   interpolation/visibility discontinuity; a fixed-world seam is stronger evidence of a
   coordinate or atlas bug. Keep the principal beam metric in every capture.
4. **Angular/atlas A/B.** Keep spacing fixed and vary per-probe angular resolution or inspect
   octahedral gutter boundaries. A change only with angular resolution points to ray/atlas
   filtering, not cell interpolation.
5. **Continuity test.** With a synthetic constant irradiance atlas and visibility forced to one,
   sample the query across each cell face. The normalized result should not jump. Repeat with
   relocation enabled, then with one candidate rejected, to identify whether the discontinuity
   is in relocation-aware weights, support-distance rejection, or visibility normalization.
6. **Direct-sun transport A/B.** Suppress only the DDGI probe direct-sun injection while leaving
   final direct lighting and geometry unchanged. If the bands vanish, inspect the transport
   hit receiver/origin, direct visibility, and the number of rays that see the opening. Do not
   infer this from changing the final VSM receiver.
7. **Convergence/state check.** Capture after the field reports converged, repeat from a clean
   startup, and compare with all valid probes kept updating. This separates a persistent spatial
   seam from publication, sleeping/relocation, or temporal convergence artifacts.

## Fix decision rules

- If raw irradiance is banded, fix probe transport/update or increase field resolution only
  after proving that the chosen direct-sun receiver and ray classification are correct. Do not
  hide it with albedo changes or post-blur.
- If raw irradiance is smooth but normalized weights jump, fix the query's cage/relocation/
  support and visibility-weight contract. Preserve the intended flat voxel material and make
  any hard visibility gate explicit.
- If both are smooth but the screenshot is banded, inspect the debug sampling coordinate,
  cache/publication identity, tone mapping, and screenshot path before touching DDGI math.
- If the band scales cleanly with spacing and disappears at an affordable density, classify it
  as a quality/resolution limit and document the chosen operating bound rather than calling the
  method broken.

The acceptance bar for a later fix is symptom-specific: the internal bands must disappear in
the exact-irradiance and normal views while the elongated principal beam remains; repeated
captures must agree; visibility-none parity must be interpreted in the correct debug path; and
DDGI debug atlas/probe-state views must not change accidentally when only a consumer-side fix is
intended.
