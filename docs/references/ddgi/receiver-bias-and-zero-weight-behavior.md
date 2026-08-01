# DDGI receiver bias, zero-weight cages, and voxel self-occlusion

Research date: 2026-08-02

## Question

After making Re: Flora's terrain DDGI query constant per voxel, the irregular
intra-voxel triangles disappeared, but some complete voxels under the tree are
still black. The main branch has an older ray-origin helper that starts from the
voxel center, reaches the voxel-cube surface along the stored voxel normal, and
then moves another fixed distance along that normal. This note asks:

1. which positions and biases the DDGI papers actually define;
2. what the papers and the reference RTXGI implementation do when probe weights
   approach or reach zero;
3. whether the older Re: Flora offset is a justified direction for the remaining
   whole-black voxel failure.

## Short answer

The older offset is a strong **diagnostic and likely part of the fix**, but it is
not the DDGI paper's probe-ray origin rule.

The papers distinguish two operations:

- DDGI update rays originate at the probe center. The probe itself may later be
  relocated if it is inside or too close to static geometry.
- A surface consuming DDGI supplies a surface point and normal. A world-space
  self-shadow bias moves the point used for the probe-to-surface visibility
  query away from the unstable depth boundary.

Re: Flora adds a third, voxel-specific operation: reconstructing a stable
surface point from the voxel center and its one stored normal. Its exact voxel
visibility test then needs an origin that is actually in empty voxel space. A
quarter-voxel offset beyond the source cube can still land in a neighboring
occupied voxel on a stair-stepped or tightly connected voxel surface. Because
all eight hard-visibility segments share that origin and the query fails closed,
one bad origin can reject the whole cage and produce one completely black
voxel. The main branch's larger `0.005` offset (1.28 voxel widths at 256 voxels
per world unit) can jump beyond that self-occluding shell, which explains the
earlier empirical result.

The safe direction is therefore to separate:

1. the canonical per-voxel shading/query anchor;
2. the biased point used by the paper-style moment visibility query; and
3. the empty-space origin used by Re: Flora's additional exact voxel segment
   test.

First A/B test the main-branch offset on (3) only. If it removes the black voxels,
replace the magic distance with a bounded, occupancy-aware "advance to the
first empty voxel along the stored normal" rule, and fail closed when no nearby
empty origin exists. Do not add a positive visibility floor or sky fallback:
those hide the black result by deliberately trusting occluded probes and can
restore the wall/roof leaks that exact visibility was introduced to prevent.

## Four positions that must not be conflated

| Position | Defined by | Purpose | Normal/view bias? |
| --- | --- | --- | --- |
| Nominal or relocated probe center | DDGI grid and probe relocation | Origin of the spherical rays that update a probe | No surface bias; update rays begin at the actual probe position |
| Probe-ray surface hit | Scene intersection | Surfel shaded to update probe irradiance and distance | Its normal is used when recursively sampling DDGI and when tracing ordinary lighting/shadow rays |
| Canonical voxel receiver anchor | Re: Flora-specific | Gives every pixel of the same voxel one stable DDGI receiver | Center-to-cube-surface displacement along the one stored voxel normal |
| Biased visibility-query point | DDGI surface query | Avoids evaluating filtered distance moments exactly at the surface discontinuity | Yes; 2019 describes normal plus a directional term, and 2021 makes this an explicit world-space normal/view vector |

The 2019 update rays explicitly share the probe center as their origin
([2019 §4.2, PDF p. 9](majercik-2019-ddgi.pdf#page=9)). The 2021 algorithm overview
likewise treats probe position adjustment as initialization, then traces update
rays from active probes; the surface self-shadow bias appears in the separate
eight-probe query
([2021 §§2.1–2.3, PDF pp. 4–5](majercik-2021-scaling-ddgi.pdf#page=4)).
NVIDIA's reference ray-generation shader makes the distinction executable: it
sets `ray.Origin` to the relocated probe world position, while a later probe-hit
surfel computes `surfaceBias` only for its recursive irradiance query
([RTXGI `ProbeTraceRGS.hlsl`, lines 59–75 and 159–188](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/samples/test-harness/shaders/ddgi/ProbeTraceRGS.hlsl#L59-L188)).

## Surface query bias in the papers

### 2019: bias is part of the visibility-aware surface interpolant

For a shading point, the 2019 query gathers its eight-probe cage and combines:

- a soft backface/orientation weight;
- suppression of very low irradiance;
- a mean/variance Chebyshev visibility estimate;
- a world-space offset of the shading point, proportional to its normal and
  direction toward the probes; and
- ordinary trilinear position weights.

Figure 6 labels the shading point, normal, probe direction, and stored mean
distance. Figure 7 then adds backface rejection, visibility, and normal bias as
separate ablations
([2019 §5.2 and Figs. 6–7, PDF pp. 13–15](majercik-2019-ddgi.pdf#page=13)).
The bias exists to move the visibility lookup away from the filtered
shadowed/unshadowed boundary. It is not described as changing the probe-ray
origin or as searching voxel occupancy. The exact 2019 direction wording is
not fully consistent: Figure 6's caption names the surface normal and
camera-view vector, while the adjacent weighting bullet names the surface
normal and direction to the probes. The 2021 paper removes that ambiguity with
the explicit normal/camera Equation 2 below.

The paper says the weighting terms use conservative epsilon bounds to avoid
numerical problems during normalization when per-probe weights approach zero.
It does not specify a sky fallback or an exact algorithm for a cage whose eight
probes are all rejected
([2019 §5.2, PDF p. 15](majercik-2019-ddgi.pdf#page=15)).

### 2021: one tunable world-space self-shadow bias

The production paper replaces several statistical knobs with Equation 2:

```text
(0.2 * surface normal + 0.8 * direction to camera)
* (0.75 * minimum axial probe spacing)
* tunable shadow bias
```

The default tunable value reported by the paper is `0.3`. This vector is added
to the initial surface sample point specifically for the visibility test
([2021 §4.1, PDF p. 8](majercik-2021-scaling-ddgi.pdf#page=8)). It is deliberately
a quality tradeoff: lower ray counts and noisier moments may need more bias, but
Figure 3 shows that excessive bias moves a query past an occluder and creates a
light leak
([2021 Fig. 3, PDF p. 9](majercik-2021-scaling-ddgi.pdf#page=9)).

For update rays that hit backfaces, the same section records zero irradiance and
20% of the original hit distance. This makes a probe inside or behind geometry
strongly shadowed without forcing its filtered distance to exactly zero. That is
probe-data conditioning; it is not a receiver-origin offset
([2021 §4.1, PDF pp. 8–9](majercik-2021-scaling-ddgi.pdf#page=8)).

NVIDIA's reference SDK expresses the surface bias as a normal component plus a
view component and adds it to the surface world position before selecting the
base cage and evaluating visibility
([RTXGI `Irradiance.hlsl`, lines 24–31 and 61–87](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/Irradiance.hlsl#L24-L87)).

For Re: Flora, copying the camera component would make lighting of a voxel vary
with view direction and would violate the chosen per-voxel invariant. Keeping a
single stored voxel normal is compatible with DDGI, but Re: Flora must calibrate
its own camera-independent bias and prove it against thin voxel walls.

## Probe rejection, relocation, and classification

### Visibility and orientation rejection

Both Majercik papers define the consumer as an eight-probe weighted gather. The
baseline factors are trilinear position, a surface-side/backface term, and
moment visibility at the biased surface point
([2019 §5.2, PDF pp. 13–15](majercik-2019-ddgi.pdf#page=13);
[2021 §2.2, PDF p. 4](majercik-2021-scaling-ddgi.pdf#page=4)). Roháček's
implementation instead clamps a simple normal-to-probe dot product at zero and
reports that the original smooth backface offset could bleed through geometry
in his scenes
([Roháček §2.1, PDF p. 2](rohacek-2022-improving-probes-ddgi.pdf#page=2)).
This is further evidence that "increase every bias" is not a general solution.

Re: Flora is stricter than the papers: it multiplies moment visibility by an
exact binary voxel segment test. That test is useful for voxel thin-wall
correctness, but it changes the all-rejected behavior discussed below.

### Relocation and state classification address bad probes, not bad receivers

The 2021 optimizer runs at initialization against static geometry. If more than
25% of rays see backfaces and a close backface is present, it tries to move the
probe through the closest backface; otherwise it may move away from close
frontfaces. Offsets stay within half the minimum probe spacing, and optimization
is capped at five iterations to avoid oscillation. Probes still trapped in static
geometry become `Off`; useful surface-adjacent probes become `Vigilant`, and
far probes may sleep until needed
([2021 §§5–6, PDF pp. 12–18](majercik-2021-scaling-ddgi.pdf#page=12)). The
authors explicitly avoid relocating around dynamic geometry because stability
is preferable to a lower but unstable average error
([2021 §5, PDF pp. 12–14](majercik-2021-scaling-ddgi.pdf#page=12)).

Roháček also reuses update rays. Backface hits are encoded with negative
distance, a temporally filtered dead-probe test requires more than half of the
rays to satisfy its near-geometry criteria, and a bounded cardinal-axis spiral
searches for a better location. Maximum displacement remains below 50% of a
cage side
([Roháček §§3.1–3.2, PDF p. 3](rohacek-2022-improving-probes-ddgi.pdf#page=3)).
After relocation, his §3.3 corrects trilinear weights so each weight remains in
`[0, 1]` and the sum remains one; violating these invariants creates visible cage
boundaries
([Roháček §3.3, PDF p. 4](rohacek-2022-improving-probes-ddgi.pdf#page=4)).

These methods should eventually repair cages whose probes are genuinely buried
or unusable. They cannot repair a valid probe that Re: Flora falsely rejects
because the receiver's exact-visibility segment starts inside an occupied voxel.

## What happens when all nearby probes have zero or near-zero weight?

There is no single answer shared by the papers and RTXGI:

1. **2019 paper:** epsilon-bounds the weighting terms to keep normalization
   numerically stable as weights approach zero, but does not define an
   all-rejected fallback
   ([2019 §5.2, PDF p. 15](majercik-2019-ddgi.pdf#page=15)).
2. **2021 paper:** warns that forcing a backface distance to zero would drive a
   Chebyshev weight toward zero and that normalization could then raise its
   relative contribution. It solves the probe-data case with a shortened
   distance and zero irradiance, plus relocation/classification; it does not
   prescribe global-sky fallback for an occluded cage
   ([2021 §4.1, PDF pp. 8–9](majercik-2021-scaling-ddgi.pdf#page=8)).
3. **Roháček:** explicitly identifies cages with most or all probes submerged in
   geometry as a source of dark scene sections and treats this as a placement
   failure to solve with dead-probe detection and movement, not by injecting
   unoccluded light
   ([Roháček §5.1, PDF p. 6](rohacek-2022-improving-probes-ddgi.pdf#page=6)).
4. **RTXGI reference shader:** prevents most active-probe weights from ever
   reaching zero. It floors each trilinear axis at `0.001`, uses wrap shading
   with a positive `0.2` term, floors Chebyshev visibility at `0.05`, and floors
   the remaining weight at `1e-6`. It returns black only when no active probe
   contributes at all
   ([RTXGI `Irradiance.hlsl`, lines 103–171 and 186–203](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/Irradiance.hlsl#L103-L203)).

RTXGI's positive floors are an intentional graceful fallback for a filtered,
statistical visibility field. Re: Flora's exact occupancy segment is an
additional authoritative visibility gate. Applying the same `0.05` floor after
an exact occlusion result would no longer be exact: it would guarantee that
light crosses a segment known to be blocked. That conflicts with the current
fail-closed thin-wall requirement.

## Mapping to the current Re: Flora implementation

At the time of this note, the feature branch behaves as follows:

- `voxelSurfacePositionAlongNormal()` constructs one stable anchor per voxel by
  intersecting the stored normal direction with the voxel cube.
- Terrain rendering and probe-hit transport both query DDGI from that canonical
  anchor, so all pixels of one voxel now choose the same cage and weights.
- `getDdgiProbeContribution()` then adds `normal * 0.25 voxel` for both the
  moment lookup and the exact segment origin. Surface-side weights are strictly
  clamped to zero, and a candidate contributes only when both moment and exact
  visibility survive.
- `ddgiVoxelSegmentVisibility()` immediately rejects an occupied start cell.
- `finalizeDdgiQueryResult()` returns zero irradiance when no weighted probe
  remains; it intentionally does not resurrect the cage through global sky.

On `main`, `nextTracingPosition()` starts at the same cube-surface point and adds
`normal * 0.005`. At 256 voxels per world unit, that fixed addition is `1.28`
voxel widths. It is used as the origin of the old stochastic secondary terrain
ray. It is therefore evidence about a working **voxel ray origin**, not evidence
that DDGI itself prescribes a `0.005` receiver bias.

The leading hypothesis for the remaining tree symptom is:

```text
smooth voxel normal
  -> cube-surface anchor exits the source voxel
  -> only 0.25 voxel of further travel
  -> point still falls in a neighboring occupied voxel / digital surface shell
  -> exact visibility rejects the common start for every candidate probe
  -> normalized irradiance has no contributors
  -> the whole voxel is black
```

This is an inference from the papers plus the current shader contracts. It must
be confirmed by capturing, for every black receiver voxel, the exact start voxel,
its occupancy, the first empty distance along the stored normal, and each of the
eight candidates' hard and moment visibility values.

## Recommendation matrix

| Candidate | Which position changes? | Expected diagnostic value | Correctness risk | Recommendation |
| --- | --- | --- | --- | --- |
| Reproduce `main`'s extra `0.005` only for exact voxel visibility | Exact segment origin only | High: directly tests whether the black cage is receiver self-occlusion | Can jump through a legitimate one-voxel occluder or narrow branch | **Selected terrain default after the controlled A/B/C below**; retain all three modes for regression |
| Advance from the canonical anchor until the first empty voxel along the stored normal, with a small epsilon and a strict maximum distance | Exact segment origin only | High after the A/B confirms the hypothesis | A wrong normal could search inward; an unbounded search could tunnel through geometry | **Deferred**: the project rejected this unproven occupancy search for the current iteration because of its complexity and runtime cost |
| Increase the bias used by both moment and exact visibility | Paper query point and exact origin together | Medium; may reduce self-shadow | 2021 Fig. 3 demonstrates that excessive moment-query bias leaks through occluders | Do **not** couple these knobs; test exact origin separately |
| Restore the 2021 camera/view component | Paper visibility point and possibly cage selection | Low for this symptom | Makes a voxel's GI view-dependent and breaks the agreed one-color-per-voxel contract | Do not use for terrain voxels; keep the stored voxel normal |
| Floor hard visibility or use a sky/nearest-probe fallback when the cage is empty | Final contribution weights | Masks black immediately | Reintroduces known wall/roof leaks by trusting an exactly occluded segment | Reject for the correctness-first path |
| Relocate/classify probes after terrain initialization and later after edits | Probe positions and states | High only for genuinely buried/invalid probes | Relocation can double-cover surfaces and requires offset-aware interpolation | Keep as the paper-aligned solution for true invalid probes; diagnose receiver self-occlusion first |
| Reject overlong filtered depth samples using cage support | Stored moment update/filter | Useful for far-depth contamination and roof leaks | Does not address an occupied exact-segment start | Retain as a separate moment-visibility safeguard; see [Roháček §3.4, PDF pp. 4–5](rohacek-2022-improving-probes-ddgi.pdf#page=4) |

## Proposed correctness-first sequence

1. In the stable `blacky` view, add/read a receiver-origin diagnostic: occupied
   start flag, distance to first empty voxel along the stored normal, and an
   eight-bit hard-visibility mask. Do not change final shading yet.
2. A/B only the exact visibility origin between the current quarter-voxel
   addition and the main-branch `0.005` addition. Leave the canonical receiver,
   cage selection, trilinear weight, surface-side weight, moment direction, and
   irradiance sample unchanged.
3. If the `0.005` variant turns the whole-black voxels into stable lit voxels,
   require the existing sealed-room, portal, and thin-wall tests to remain
   fail-closed. A visual improvement without those tests is not acceptance.
4. Keep the bounded empty-space origin resolver deferred. Revisit it only if a
   later contact-surface regression proves that the fixed offset is tunneling;
   do not add its occupancy search speculatively.
5. Re-run probe classification/relocation separately for any remaining black
   cages. Those survivors are more likely to be genuine bad-probe placement or
   moment-data failures than receiver self-intersection.

## Controlled terrain-origin experiment (2026-08-02)

The stable `blacky` snapshot compared three terrain-only exact-segment origins.
Every mode kept the canonical surface anchor, normal, cage, trilinear and
surface-side weights, moment query, probe field, raster consumers, and transport
source unchanged:

- A, `surface-quarter`: surface plus 0.25 voxel (the pre-experiment default);
- B, `center-fixed`: voxel center plus `0.005` world units;
- C, `surface-fixed`: surface plus `0.005` world units (the `main` form).

| Spacing | Mode | Environment-zero pixels | Combined-zero pixels | Exact hard-visibility-zero pixels |
| --- | --- | ---: | ---: | ---: |
| 32 | A, surface-quarter | 19,240 | 13,442 | 18,924 |
| 32 | B, center-fixed | 15,603 | 10,949 | 15,272 |
| 32 | C, surface-fixed | 10,154 | 7,518 | 9,929 |
| 16 | A, surface-quarter | 14,198 | 8,981 | 13,827 |
| 16 | B, center-fixed | 11,099 | 6,904 | 10,871 |
| 16 | C, surface-fixed | 7,102 | 4,907 | 6,781 |

C removes 47.2% of A's environment-zero pixels at spacing 32 and 50.0% at
spacing 16. B's smaller improvement shows that reaching the canonical surface
before applying the fixed offset matters. B and C kept the mixed-zero receiver
voxel count at zero, so neither restored the earlier within-voxel triangular
lighting split.

Running C through `scripts/check_ddgi_correctness.sh` passed sealed, portal, and
one/two/diagonal thin-wall cases at both spacings. The sealed room stayed exactly
black; the wall final-to-exact luminance-error P99 values were `0.02989` at
spacing 32 and `0.00626` at spacing 16, below the committed `0.15` and `0.133`
limits.

An additional cross-mode comparison avoided accepting a self-consistent C
reference blindly. Sealed exact visibility was bit-identical between A and C,
and portal mean error was at most `0.000051`. Walls changed more substantially:
mean exact-visibility deltas were `0.03289` and `0.03581`. Strong positive
changes localized overwhelmingly to the visible one-voxel, two-voxel, and
diagonal wall receivers themselves (24,389 of 27,491 pixels at spacing 32 and
31,211 of 34,089 at spacing 16); only 14 spacing-16 pixels localized to the back
wall. This supports receiver self-occlusion relief rather than observed
far-side leakage in the committed scenarios.

The evidence selects C as the terrain default while retaining explicit A/B/C
CLI modes. It does not erase the theoretical risk: `0.005` world units is 1.28
terrain voxels. Future regressions at touching surfaces should be tested against
`surface-quarter` before changing the moment-query bias or weakening the
fail-closed gate.

## Evidence limits

- The papers operate on continuous triangle surfaces and filtered statistical
  visibility. They do not define how to exit a binary voxel surface for an exact
  DDA segment test; the bounded first-empty rule is a Re: Flora design inference.
- The main-branch `0.005` value is empirical and world-scale-specific. Its prior
  success supports the diagnosis but does not prove it is safe near all thin
  voxel occluders.
- Roháček's paper is a non-peer-reviewed seven-page CESCG report. Its
  dead-probe and depth-rejection mechanisms are useful corroboration, not a
  universal proof.
- RTXGI's positive weight floors show a production choice for graceful
  filtering. They should not be transplanted after Re: Flora's authoritative
  exact-occupancy gate without explicitly accepting renewed leakage.
