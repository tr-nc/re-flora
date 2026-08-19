# Cornell-box DDGI probe-grid artifact

> Historical experiment record. The dedicated Cornell-box runtime scene and its CLI flag have
> since been removed; commands below document the original reproduction and are no longer runnable.

> **Superseded diagnosis (2026-08-19):** the receiver-bias conclusion below was
> based on a Moment Visibility debug crop and did not predict the production
> Final view. A fixed-camera Final A/B retained `98.69%` of the grid energy at
> the maximum bias. The corrected diagnosis, rejected alternatives, accepted
> same-side weighting curve, and leak gates are recorded in
> [`cornell-box-grid-followup.md`](cornell-box-grid-followup.md). This file is
> retained as the first-pass research record, not current implementation advice.

Research date: 2026-08-18

Scope: diagnosis, experiment design, and the accepted Git/GUI experiment

## Decision

The regular pattern on the Cornell box's white wall is **not a color-bit-depth
artifact and is not an unavoidable property of DDGI**. It is a stable,
probe-aligned error introduced by Re: Flora's **moment-visibility weighting**.
The strongest current explanation is that the receiver is queried too close to
the distance distribution's unstable surface boundary. The current, very small
fixed normal bias leaves neighboring probes with systematically different
Chebyshev visibility, and interpolation exposes those differences as probe
cells.

This conclusion is narrower than saying that DDGI has no inherent error. DDGI
is a sparse, low-frequency diffuse-light representation: insufficient probe
density can lose spatial detail and produce leakage. That limitation remains.
However, a regular wall checker that tracks probe spacing is not a required
outcome of the method. The original paper explicitly uses trilinear and soft
visibility weights to avoid cage transitions, and reports robustness to grid
rotation and translation while the shaded region remains covered
([Majercik et al. 2019, §§5.2 and 6.4, Figs. 6–7 and 14, PDF pp. 13–15 and
21–22](majercik-2019-ddgi.pdf#page=13)). NVIDIA describes the expected output as
low-frequency diffuse irradiance, not as a piecewise probe-cell field
([RTXGI Algorithms, lines 27–35](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Algorithms.md#L27-L35)).

The implementation experiment therefore uses a **one-terrain-voxel normal-only
receiver bias** (`0.00390625` world), rather than another density increase or a
precision change. It is the smallest tested value that clears the Cornell
metric. The previous `0.000977` value remains the Git/GUI control; the new value
is the branch's experiment default, not a claim that all future geometry can
drop leak regression coverage.

## Reproduction contract

The controlled captures used:

- the only saved camera snapshot, [`gd-ptn2`](../../../config/camera_snapshots.toml#L1),
  whose explicit pose overrides the Cornell test scene's default camera
  ([startup order](../../../src/app/core/mod.rs#L1130));
- the deterministic Cornell-box scene;
- a hidden release build at 2880 × 1620;
- 64 rays per probe and the final field at update epoch 63;
- the same saved GUI/sky state for every comparison;
- spacing 32 (`17³ = 4,913` probes) and spacing 16
  (`33³ = 35,937` probes).

Representative command:

```bash
cargo run --release -- --hidden --mute \
  --cornell-box-scene \
  --environment-probe-spacing-voxels 32 \
  --ddgi-debug-view final \
  --screenshot gd-ptn2 \
  target/cornell-ddgi-grid-research/spacing32-final.png \
  --screenshot-delay 6 --auto-exit 9
```

The uncommitted evidence directory contains `final`, `moment-visibility`,
`exact-visibility`, `visibility-error`, `exact-irradiance`,
`unoccluded-irradiance`, `weight-sum`, `dominant-probe`, `probe-state`, and
`relocation` captures. The output logs record convergence before the accepted
captures. These artifacts intentionally remain under ignored `target/` rather
than becoming runtime fixtures.

## Observed local facts

| Isolation | Observation | What it says |
| --- | --- | --- |
| Spacing-32 Final | The white wall has a regular cell grid. | Establishes the symptom under the saved view. |
| Moment Visibility | The wall has a strong regular grid. | Locates the pattern in the consumer visibility term. |
| Visibility Error (`abs(moment - exact)`) | The same grid is stronger and spatially precise. | Moment visibility, rather than the reference hard visibility, creates the cell-dependent error. |
| Exact Irradiance | The wall is visually smooth. | The irradiance data and the shared nominal trilinear/surface-side basis can produce a smooth wall when moment visibility is removed. |
| Unoccluded Irradiance | The wall is visually smooth. | Irradiance encoding/sampling alone does not contain the visible checker. |
| Weight Sum / Dominant Probe | The expected probe lattice/cage partition is visible. | The visibility error is projected through the eight-probe basis at exactly the observed spatial scale. |
| Probe State / Relocation | Visible wall cages are valid; relocation does not explain the pattern. | Invalid-probe holes and relocation discontinuities are not supported as the immediate cause. |
| Spacing 16 | Cells become finer and weaker but remain; visibility-error frequency approximately doubles. | Density reduces and spatially shrinks the symptom but does not remove its mechanism. |

The fixed-camera result has reached the system's final temporal sample budget.
Independent default-bias repeats have normalized-image RMSE `0.0000422`,
whereas the default-to-bias A/B RMSE is `0.01618`. The pattern is therefore
fixed spatial bias, not unresolved per-frame Monte Carlo noise. More temporal
accumulation can reduce estimator variance, but does not specifically remove a
systematic query-boundary error and can converge to a stable biased result.

One temporary, GUI-only A/B changed only the receiver visibility bias from
`0.000977` to `0.02` world units and then restored the configuration
byte-for-byte. At spacing 32, the moment-visibility wall grid nearly vanished:
the normalized wall ROI's high-pass standard deviation at sigma 32 changed from
`0.006608` to `0.0000986`, a 98.5% reduction. This strongly identifies the
surface/moment boundary as the cause family. It does **not** establish that
`0.02` is safe near portals or thin walls.

A follow-up sweep bounded the useful transition in terrain-voxel units:

| Normal-only bias | Wall high-pass standard deviation | Gate |
| --- | ---: | --- |
| `0.000977` world, about 0.25 voxel | `0.00660821` | RED |
| `0.001953125` world, 0.5 voxel | `0.00361189` | RED |
| `0.00390625` world, 1 voxel | `0.000423077` | GREEN |
| `0.0078125` world, 2 voxels | `0.000245527` | GREEN |
| `0.02` world, about 5.12 voxels | `0.0000985887` | GREEN |

The one-voxel candidate also made the normal Final capture visibly smooth. In
matched current-build spacing-32 correctness captures, it did not weaken the
existing occluder gates:

- the sealed case remained exactly black (`luminance max = 0`);
- the portal Moment-vs-Exact luminance error p99 was `0.001518`, below the
  committed `0.01` limit, and overestimate p99 was `0.000171`;
- the walls p99 was effectively unchanged (`0.397584` at the saved default,
  `0.397634` at one voxel), below the committed `0.40` limit, while mean error
  changed from `0.076073` to `0.075950`.

The implementation pass then repeated the committed sealed, portal, and walls
matrix at spacing 32 and 16. All six quality gates passed with coherent release
captures. At spacing 16 the sealed result remained black, portal
Moment-vs-Exact error p99 was `0.000523` against the `0.01` limit, and the walls
case remained below its `0.375` limit. The spacing-32 walls p99 was `0.397408`
against the `0.40` limit. A spacing-16 Cornell Moment capture also cleared the
scaled wall high-pass gate (`0.000257 < 0.001`).

The first full-matrix invocation reported three infrastructure failures: two
spacing-16 exact views needed about 25 seconds but the script allowed 24, and
one set changed render extent while the display state changed. Re-capturing at
a fixed 800 x 450 windowed extent made the missing comparisons compatible and
green. The walls-spacing-32 repeat payloads were not bit-exact even though both
were converged at epoch 63; the analyzer's quality and compatibility gates
passed. Treat that temporal/capture-repeat variance as a separate existing
test-harness follow-up, not as evidence that a larger bias is free.

Runtime A/B does not require a second flag. The existing
`DDGI Receiver Visibility Bias (world)` float control accepts exact numeric
input, persists through the GUI Save action, and participates in the DDGI
radiance snapshot identity. Enter `0.000977` for the control or `0.00390625` for
the one-voxel experiment, then wait for the replacement field to report
Converged. Git commit `deb8c8cd` is the source-level control point.

The dogleg/roof and edit-publication cases were not rerun in this implementation
pass. They remain required before treating the experiment value as a universal
production constant. Every earlier temporary config edit was restored before
the experiment default was changed intentionally.

## Literature facts

These are facts from primary sources, separated from the current-code findings
and the root-cause inference below.

### What DDGI is expected to approximate

DDGI assumes that a surface can use incident light from nearby, mutually visible
probes. The approximation error grows as probe density decreases, and the 2019
evaluation found probe density more important than angular resolution for its
scenes ([Majercik et al. 2019, §§6.1 and 7, PDF pp. 16–19 and
23](majercik-2019-ddgi.pdf#page=16)). RTXGI likewise calls the result inherently
low frequency and recommends composing it with other techniques for missing
high-frequency detail
([RTXGI Algorithms, lines 27–35](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Algorithms.md#L27-L35)).

That explains why denser probes can reduce error, but not why this wall error is
locked to cage boundaries. The baseline query is an eight-probe gather with
smooth orientation, Chebyshev visibility, receiver bias, and trilinear spatial
weights ([Majercik et al. 2019, §5.2, PDF pp. 13–15](majercik-2019-ddgi.pdf#page=13)).

### The receiver bias is part of the algorithm, not a cosmetic workaround

The 2021 production paper says distance-estimate variance is highest around the
mean—at the surface—and moves the visibility sample to a lower-variance point.
Its Equation 2 is:

```text
(0.2 * normal + 0.8 * direction-to-camera)
* (0.75 * minimum axial probe spacing)
* TunableShadowBias
```

The paper's default `TunableShadowBias` is `0.3`; it also says noisier estimates
from lower ray counts generally need more bias. Excess bias can move the sample
past an occluder and leak light, so this remains a measured tradeoff
([Majercik et al. 2021, §4.1 and Fig. 3, PDF pp. 8–9](majercik-2021-scaling-ddgi.pdf#page=8)).
RTXGI implements the normal/view bias and adds it to the surface position before
the cage and distance query
([RTXGI `Irradiance.hlsl`, lines 24–31 and 61–87](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/Irradiance.hlsl#L24-L87)).

Re: Flora deliberately wants a view-independent terrain-voxel result, so the
camera term should not be copied blindly. The paper does establish two useful
requirements: bias should be considered relative to probe spacing, and it must
be selected together with the distance estimator's variance and leak risk.

### Visibility filtering and graceful weights are tunable conditioning

RTXGI filters first and second hit-distance moments using a cosine weight raised
to a configurable distance exponent, then temporally blends those moments with
hysteresis
([RTXGI `ProbeBlendingCS.hlsl`, lines 419–502 and 563–568](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/ProbeBlendingCS.hlsl#L419-L568)).
At query time, the reference implementation also uses per-axis trilinear floors,
wrap shading with a positive offset, cubed Chebyshev visibility with a `0.05`
floor, a tiny total-weight floor, and a continuous small-weight crush
([RTXGI `Irradiance.hlsl`, lines 115–171](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/Irradiance.hlsl#L115-L171)).

Those floors are robustness/leak tradeoffs, not universally correct constants.
RTXGI explicitly floors visibility to avoid an all-zero fallback
([RTXGI `Irradiance.hlsl`, lines 143–160](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/Irradiance.hlsl#L143-L160));
separately, the 2021 paper demonstrates that moving a visibility query too far
from the surface can leak through an occluder
([Majercik et al. 2021, Fig. 3, PDF p. 9](majercik-2021-scaling-ddgi.pdf#page=9)).

### Precision and relocation failure signatures

The 2019 paper reports visible artifacts with 8-bit integer probe texels that
vanish at 16-bit floating point; 11-bit floating point was visually close to
16-bit in its tests ([Majercik et al. 2019, §6.3, PDF pp. 20–21](majercik-2019-ddgi.pdf#page=20)).

Relocation can create a different probe-cell artifact if interpolation no longer
obeys weights in `[0,1]` summing to one. Roháček specifically reports visible
cage boundaries when the sum invariant is violated, and also reports depth
leaks from nearly parallel, overly distant samples in a low-resolution distance
field
([Roháček 2022, §§3.3–3.4, PDF pp. 4–5](rohacek-2022-improving-probes-ddgi.pdf#page=4)).
These are useful alternative hypotheses, but they do not match the current
isolation evidence as well as receiver/moment conditioning.

## Current-code facts

The following statements describe this repository at the research commit; they
are not literature claims.

- The atlases are `RGBA32F` irradiance and `RG32F` visibility
  ([resource formats](../../../src/ddgi/resources.rs#L20)). A bit-depth upgrade
  cannot fix the current symptom because the stored data is already full
  32-bit float.
- Irradiance uses an 8 × 8 interior and visibility uses 16 × 16
  ([atlas constants](../../../src/ddgi/atlas.rs#L4)), the same production
  angular layout reported by the 2021 paper
  ([Majercik et al. 2021, §6, PDF p. 19](majercik-2021-scaling-ddgi.pdf#page=19)).
- Each probe traces 64 rays
  ([shared shader configuration](../../../shader/slang/ddgi_config.slang#L1)).
  Every update epoch applies a deterministic SO(3) rotation, and successive
  fields temporally retain valid irradiance and visibility history
  ([history](../../../src/ddgi/resources.rs#L249),
  [rotation](../../../src/ddgi/resources.rs#L283)).
- The visibility filter uses a narrow cosine-power exponent of `50`, shortens
  backface distances to 20%, rejects non-sky distances beyond cage support, and
  temporally blends the first two moments
  ([visibility filter](../../../shader/slang/ddgi_visibility_filter.slang#L6)).
- The query returns cubed Chebyshev probability with no positive floor
  ([Chebyshev query](../../../shader/slang/ddgi_query.slang#L233)).
- Terrain adds the configurable bias only along its stable surface normal. The
  configured value is `0.000977` world units
  ([query](../../../shader/slang/ddgi_query.slang#L420),
  [saved setting](../../../config/gui.toml#L1035)). With 256 terrain voxels per
  world unit, that is about 0.25 voxel: only `0.78%` of spacing 32 and `1.56%`
  of spacing 16.
- Production terrain uses nominal trilinear weights, a hard squared
  surface-side term, and moment visibility; it does not multiply by the debug
  exact-visibility reference
  ([weight construction](../../../shader/slang/ddgi_query.slang#L375),
  [terrain query](../../../shader/slang/ddgi_query.slang#L931)). The receiver is
  fixed per voxel while only the nominal position basis follows the exact hit
  ([tracer](../../../shader/slang/tracer.slang#L467)).
- The exact and error debug modes intentionally reuse the same probe data and
  contribution machinery, allowing the capture set to isolate moment
  visibility without changing the scene
  ([debug modes](../../../shader/slang/tracer.slang#L316)).
- Earlier project evidence already recorded this failure class: a narrow or
  abrupt directional-visibility field revealed the probe lattice; the then-used
  exponent `8`, two-voxel bias, `0.05` floor, and omission of small-weight crush
  produced a smooth wall in that older pipeline
  ([historical experiment](../../local_environment_probe_plan.md#L681)). This is
  local regression history, not proof that those old constants remain correct
  for the current renderer.

## Root-cause inference and ranked hypotheses

This section is inference from the literature, current code, and controlled
captures.

### 1. Undersized receiver bias at the moment boundary — strongly supported

The wall receiver sits almost exactly on the surface represented by each
probe's filtered distance distribution. With only a quarter-voxel normal bias,
small systematic differences in neighboring probes' means and variances put
the receiver on different sides of `distance > mean`. Cubing the Chebyshev
probability and allowing it to reach zero gives those differences high contrast.
The normalized eight-probe gather then paints the contrast at cage scale.

This explains all observed facts at once: exact/unoccluded irradiance is smooth,
moment error follows the lattice, spacing 16 shrinks the cells, converged frames
remain stable, and changing only bias reduces the measured pattern by 98.5%.

### 2. Exponent 50 under-conditions a 64-ray visibility field — plausible amplifier

A high cosine exponent gives each visibility texel effective support from fewer
near-aligned rays. Temporal rotation improves sample coverage, but it does not
remove a deterministic surface-boundary error after the moments converge. The
older local pipeline's exponent-8 observation and RTXGI's configurable
`probeDistanceExponent` make this a principled second A/B, after bias. It is not
yet isolated by the new Cornell capture set.

### 3. Hard surface-side weights and missing positive floors — possible amplifier

The current hard hemisphere term and zero visibility floor can make one probe's
weight disappear abruptly. However, Exact Irradiance uses the shared hard
surface-side basis and is smooth, so this basis is not sufficient to create the
symptom. RTXGI-style wrap weighting or a visibility floor may reduce contrast,
but they trade rejection for leakage and should not precede the bias test.

### 4. Relocation/interpolation — not supported for this occurrence

Relocation-aware interpolation can create cage boundaries in principle, but
production terrain's smooth path uses nominal trilinear weights, the visible
cages report valid probe state, and Exact Irradiance is smooth. Keep relocation
diagnostics as a regression gate, not as the leading fix.

### 5. Irradiance angular encoding, atlas precision, or inherent DDGI density — rejected as the primary cause

Both 32F atlases rule out low-bit-depth quantization. Exact and Unoccluded
Irradiance rule out an irradiance-atlas checker. Density changes the scale and
amplitude but preserves the same moment-visibility mechanism. The inherent
low-frequency limitation remains relevant to fine indirect-shadow detail, not
to this regular cell pattern.

## Recommended discriminating experiments

### 1. Turn the Cornell wall into a quantitative regression gate

Keep `gd-ptn2`, resolution, sky state, rays, spacing, and epoch 63 fixed. Capture
at least Final, Moment Visibility, Exact Visibility, Visibility Error, Exact
Irradiance, and Probe State. For the white-wall ROI, record:

- high-pass standard deviation at a fixed sigma;
- Moment-vs-Exact visibility RMSE;
- two independent same-setting repeat RMSEs;
- cell-frequency energy at the known projected probe spacing.

A candidate only passes if changed-setting error exceeds repeat variance and
both Moment Visibility and Final improve. The historical saved-seam analyzer
targets a different horizontal-seam ROI and is not a red-capable gate for this
Cornell symptom.

### 2. Sweep receiver bias in voxel and dimensionless units first

Express the normal-only terrain bias as
`k * min(probe_spacing_world)` and test, for example,
`k = {0.03125, 0.05, 0.10, 0.15, 0.20}` at both spacing 32 and spacing 16.
Also compare fixed 0.5-, 1-, and 2-voxel offsets, because Re: Flora's occluders
and canonical receiver are voxel-defined. Include the current value and the
`0.02` world A/B as controls. Choose the smallest rule that passes the wall
metric at both densities; do not copy the paper's mixed normal/view scalar as a
normal-only constant.

Every bias candidate must also pass sealed-room, portal, roof/skylight,
one-voxel and two-voxel wall, and dogleg-occluder tests. Compare Moment against
the exact debug reference or the terrain path-tracing reference at the same
receivers. Reject any value that improves the wall by tunneling light through
those occluders. Repeat after a terrain edit so the result also covers
relocation and refreshed moments.

### 3. If no bias is both smooth and leak-safe, widen the visibility filter

With the accepted bias held fixed, A/B exponents such as `8`, `16`, `32`, and
`50`. Hold ray directions, 64-ray budget, history, epoch count, atlas resolution,
and scene fixed. Measure both the Cornell visibility-error metric and the leak
gates. A wider filter should lower per-probe directional variance; it may also
blur occluder boundaries, so it needs the same correctness tests.

### 4. Test query-weight conditioning one factor at a time

Only after the first two experiments:

1. compare hard surface-side weighting with RTXGI-style wrap weighting;
2. compare no visibility floor with a small floor;
3. compare the current normalization with RTXGI's continuous low-weight curve.

Do not accept the already-smooth nominal-wrap debug capture as proof: that debug
reference also omits production moment visibility, so it did not isolate wrap
weighting. A positive visibility floor is especially risky in Moment-only
production because it deliberately admits a probe judged occluded. If a floor
is required, gate it behind exact/hybrid trust in the test build before
considering the performance/correctness tradeoff.

### 5. Defer expensive density/precision/ray-count changes

Repeat ray-count and visibility-atlas-resolution A/B only if bias and filter
width cannot satisfy both smoothness and leaks. Do not change atlas format: 32F
already exceeds the precision the literature found visually sufficient. Do not
use another 8× probe-count increase as the primary fix; current evidence shows
that it pays substantial time and memory to make the same mechanism smaller.

An optional final confirmation is to translate or rotate the probe grid while
holding the room and camera fixed. The wall pattern following the grid would be
additional causal evidence; a pattern fixed in screen or voxel space would
reopen post-process or terrain-quantization hypotheses.

## Acceptance rule

The current evidence is sufficient to proceed with a narrow implementation
experiment, but not to choose a production constant. Accept a renderer change
only when one parameter family simultaneously:

1. materially reduces the fixed-camera Moment-vs-Exact wall error at spacing 32
   and 16;
2. stays stable across repeated converged captures;
3. preserves thin-wall, portal, roof, and edit-time leak correctness; and
4. does not hide the problem solely with post-process dithering or higher probe
   density.

On present evidence, the preferred order is **spacing-scaled receiver bias,
then visibility-filter width, then weight conditioning**. Precision changes,
relocation rewrites, and additional probe density are not justified as first
responses.
