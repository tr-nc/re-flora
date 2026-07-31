# Scaling Probe-Based Real-Time Dynamic Global Illumination for Production — reading notes

## Bibliographic and rights record

- Authors: Zander Majercik, Adam Marrs, Josef Spjut, Morgan McGuire
- Venue: *Journal of Computer Graphics Techniques (JCGT)*, volume 10, number 2, pages 1–29, 2021
- Published: 2021-05-03
- Primary source: <https://jcgt.org/published/0010/02/01/>
- Local source: [publisher PDF](majercik-2021-scaling-ddgi.pdf)
- License: CC BY-ND 3.0, stated on [PDF page 29](majercik-2021-scaling-ddgi.pdf#page=29)

Because the license prohibits distributing adaptations, this file is an original technical summary rather than a full-text conversion.

## What this paper adds

This paper starts from the 2019 DDGI representation and records production changes developed while integrating it into RTXGI, Unity, Unreal Engine 4, and commercial engines. The most relevant correctness changes are a single world-space self-shadow bias, explicit handling of probe rays that hit backfaces, static-geometry probe relocation, state-driven probe activation, and robust blending across multiple moving or nested volumes. It also adds convergence heuristics and probe-update optimizations.

## Section and page map

| Topic | Paper location | Why it matters |
| --- | --- | --- |
| Complete algorithm overview | [§2, PDF pp. 3–5](majercik-2021-scaling-ddgi.pdf#page=3) | Compact reference for update and eight-probe query. |
| Self-shadow bias and backfaces | [§4.1, PDF pp. 8–9](majercik-2021-scaling-ddgi.pdf#page=8) | Direct treatment of visibility leaks. |
| Temporal encoding and convergence | [§4.2–4.3, PDF pp. 9–11](majercik-2021-scaling-ddgi.pdf#page=9) | Explains lag, flicker, and rapid response to large changes. |
| Probe relocation | [§5, PDF pp. 12–16](majercik-2021-scaling-ddgi.pdf#page=12) | Moves probes out of static walls while preserving grid indexing. |
| Probe states | [§6, PDF pp. 16–18](majercik-2021-scaling-ddgi.pdf#page=16) | Distinguishes invalid, sleeping, and contributing probes. |
| Tracking windows and volumes | [§7.3–7.4, PDF pp. 21–24](majercik-2021-scaling-ddgi.pdf#page=21) | Handles large worlds and transitions without popping. |
| Limitations | [§8.1, PDF pp. 25–26](majercik-2021-scaling-ddgi.pdf#page=25) | Records remaining ghosting and ray-budget tradeoffs. |

<a id="section-2021-query"></a>
## Baseline query and update — §2, PDF pp. 3–5

The surface query remains an eight-probe gather. For each probe, the implementation multiplies:

- trilinear position weight;
- backface/orientation weight;
- visibility probability, evaluated at a biased surface point.

It samples irradiance from the probe in the surface-normal direction and normalizes the weighted sum. The update traces a randomly rotated Fibonacci sphere from every active probe, shades the hits with the regular deferred path, filters radiance into irradiance, and filters distance plus squared distance into the visibility field.

The paper's production target uses 8×8 irradiance and 16×16 visibility maps. These sizes are selected not only for quality and storage, but also to map efficiently to GPU thread-group sizes.

<a id="section-2021-self-shadow"></a>
## Self-shadow bias and backface handling — §4.1, PDF pp. 8–9

The previous collection of statistical bias controls is replaced by one world-space vector:

```text
bias = (0.2 * surface_normal + 0.8 * direction_to_camera)
       * (0.75 * minimum_axial_probe_spacing)
       * tunable_shadow_bias
```

The stated default for `tunable_shadow_bias` is 0.3. The factor is intentionally related to probe spacing, so changing spacing changes the absolute bias unless the implementation decouples them.

This is a correctness-quality tradeoff rather than a free robustness knob:

- Too little bias evaluates the depth distribution at its highest-variance boundary and can self-shadow.
- Too much bias can push the query past occluding geometry and create light leaks.
- Lower ray counts increase moment variance and may require a larger bias, but that can be unsafe near thin walls.

For probe-update rays whose first hit is a backface, the paper writes zero irradiance and reduces the recorded distance by 80% (to 20% of the original). The goal is to make probes inside or behind geometry strongly shadowed without forcing the mean distance to exactly zero, which would behave poorly under Chebyshev weighting and normalization.

<a id="figure-2021-3"></a>
### Figure 3 — excessive bias causes a leak

[PDF page 9](majercik-2021-scaling-ddgi.pdf#page=9) shows a wall leaking when self-shadow bias is too high and the corrected result with a smaller value. This is a useful counterexample to treating bias as a universal fix.

<a id="section-2021-convergence"></a>
## Perceptual encoding and convergence — §4.2–4.3, PDF pp. 9–11

The paper stores irradiance through an exponential encoding with exponent 5.0. Updates interpolate in that encoded space; sampling partially decodes before trilinear blending and returns to linear irradiance afterward. The intent is faster perceived light-to-dark response and suppression of low-frequency firefly flicker.

It also adjusts irradiance hysteresis when a texel changes substantially:

- above 25% change, subtract 0.15 from hysteresis;
- above 80% change, set hysteresis to zero for that update.

Scene-level events may temporarily lower hysteresis for several frames. Visibility hysteresis is kept higher whenever possible because aggressive depth-moment changes produce instability. The paper also warns that TAA adds its own temporal lag; changing probe hysteresis without coordinating TAA can hide the intended convergence improvement.

For a stable static test scene, temporal convergence should be ruled out before diagnosing a final grid pattern. Capture after all probes and downstream temporal filters have settled, and repeat the same capture to separate persistent spatial structure from transient update order.

<a id="section-2021-relocation"></a>
## Probe-position adjustment — §5, PDF pp. 12–16

Visibility can suppress a probe buried in geometry, but an ignored probe wastes one of the eight spatial samples and can leave acute corners poorly represented. The paper therefore relocates probes around **static** geometry during initialization:

1. If more than 25% of traced directions see backfaces and a close backface exists, move through the closest backface.
2. Otherwise, if the probe is too close to front-facing geometry and has useful free space, move it away.
3. Keep the offset within half of the minimum axial probe spacing so the original grid-indexing topology remains valid.
4. Run at most five optimizer iterations to avoid oscillation through tangent surfaces.

Dynamic geometry does not drive relocation because moving probes every time an object crosses the grid is less stable than letting the visibility and convergence rules handle it.

Relocation can also create **double coverage**: probes formerly inside a wall move to the same visible side as the other four probes of a cage. This raises update cost and changes the spatial sample distribution even if the image improves. Any implementation that moves probes must make interpolation aware of actual positions or prove that its constrained offsets preserve the chosen weights.

<a id="figure-2021-5"></a>
### Figure 5 — acute-corner leak and relocation

[PDF pages 13–14](majercik-2021-scaling-ddgi.pdf#page=13) show probes trapped near a ceiling corner and the result after moving them out of the surfaces.

<a id="figure-2021-6"></a>
### Figure 6 — double coverage

[PDF page 14](majercik-2021-scaling-ddgi.pdf#page=14) visualizes an eight-probe cage whose relocated probes all cover the same surface side. It is a reminder that relocation changes more than validity.

<a id="section-2021-states"></a>
## Probe states — §6, PDF pp. 16–18

The state machine separates probes by whether they can or must contribute:

- **Off:** remains inside static geometry; never trace or update.
- **Sleeping:** valid but currently too far from relevant surfaces to contribute.
- **Newly Awake / Newly Vigilant:** needs rapid initialization after becoming useful.
- **Awake / Vigilant:** updates normally; vigilant probes are needed continuously for static-surface propagation.

Camera visibility alone is insufficient for sleeping decisions. An off-screen probe may still propagate indirect light toward visible surfaces through later bounces. Dynamic-object bounds are conservatively expanded by one cell plus self-shadow bias to wake nearby sleeping probes.

For correctness testing, first run with all valid probes vigilant. State transitions are an optimization layer and should be reintroduced only after the static spatial result is correct.

<a id="section-2021-volumes"></a>
## Tracking windows and multiple volumes — §7.3–7.4, PDF pp. 21–24

Large scenes use a camera-following circular grid and nested probe volumes of decreasing density. When the camera crosses a cell threshold, the furthest plane of probes leapfrogs to the front and is reinitialized. Static volume blending fades over the outermost cell. A tracking volume tightens and camera-centers that fade so newly spawned probes do not suddenly dominate a surface.

Nested volumes are sampled from densest to sparsest because the dense field should best approximate local lighting. Contributions accumulate until total volume weight reaches one. This ordering and the transition function matter: two individually smooth grids can still form a seam if their normalized cross-volume weights jump.

<a id="figure-2021-9"></a>
### Figure 9 — octahedral borders and compute indexing

[PDF page 20](majercik-2021-scaling-ddgi.pdf#page=20) shows border-copy rules and thread-block alignment for 8×8 irradiance and 16×16 visibility tiles. Incorrect gutters can look exactly like directional grid artifacts, so atlas seams should be ruled out independently of spatial probe interpolation.

<a id="figure-2021-13"></a>
### Figure 13 — moving-volume blend

[PDF page 23](majercik-2021-scaling-ddgi.pdf#page=23) contrasts abrupt static blending with camera-aware blending when a probe plane leapfrogs.

## Correctness-first checklist for a spacing regression

1. **Freeze probe states.** Keep every valid probe updating so no spatial pattern comes from Awake/Sleeping transitions.
2. **Freeze relocation.** Compare the exact uniform lattice against relocated positions; record actual, not nominal, probe-to-surface vectors.
3. **Express bias in world units.** Log the absolute self-shadow bias before and after changing spacing. A spacing-relative formula silently changes the occlusion query when density changes.
4. **Inspect backface history.** Check whether probes near the wall repeatedly switch between normal and shortened backface depth samples.
5. **Visualize the unnormalized sum.** Chebyshev visibility near zero plus normalized eight-probe weights can turn a small statistical difference into a dominant cell.
6. **Verify octahedral gutters.** Sample across every tile edge in a synthetic directional field before blaming spatial interpolation.
7. **Converge without TAA.** A stable capture should not depend on temporal anti-aliasing hiding probe changes.
8. **Test visibility and irradiance separately.** Constant irradiance with live visibility isolates query structure; live irradiance with visibility forced to one isolates the field update.

## Limits to carry forward

- The production changes are empirically robust, but many constants are heuristic.
- Bias depends on geometry scale, probe spacing, and ray-count variance.
- Relocation is constrained and cannot rescue every probe; it can also duplicate coverage.
- Temporal heuristics reduce lag but do not eliminate ghosting from small bright sources.
- Cascades and moving windows require an additional spatial blending proof beyond the single-grid query.
