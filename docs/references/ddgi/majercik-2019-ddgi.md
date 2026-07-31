# Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields — reading notes

## Bibliographic and rights record

- Authors: Zander Majercik, Jean-Philippe Guertin, Derek Nowrouzezahrai, Morgan McGuire
- Venue: *Journal of Computer Graphics Techniques (JCGT)*, volume 8, number 2, pages 1–30, 2019
- Published: 2019-06-05
- Primary source: <https://jcgt.org/published/0008/02/01/>
- Local source: [publisher PDF](majercik-2019-ddgi.pdf)
- License: CC BY-ND 3.0, stated on [PDF page 30](majercik-2019-ddgi.pdf#page=30)

Because the license prohibits distributing adaptations, this file is an original technical summary rather than a full-text conversion.

## One-paragraph model

The paper turns an irradiance-probe grid into a dynamic irradiance field with visibility. Every probe stores a low-angular-resolution irradiance map plus directional first and second moments of hit distance. Each frame, rays update those fields from dynamic scene geometry and lighting. A surface query gathers the eight probes around the shading point, then combines spatial interpolation, surface orientation, directional depth moments, and a world-space bias to reject probes that should not contribute. Multiple diffuse bounces emerge over successive frames because probe-hit surfels are shaded using the previous probe state.

## Section and page map

| Topic | Paper location | Why it matters |
| --- | --- | --- |
| Motivation and contributions | [§1, PDF pp. 2–4](majercik-2019-ddgi.pdf#page=2) | Frames light leaking as missing visibility, not merely insufficient irradiance resolution. |
| Probe representation | [§3, PDF pp. 6–7](majercik-2019-ddgi.pdf#page=6) | Defines octahedral irradiance and depth-moment storage. |
| Probe updates | [§4, PDF pp. 8–11](majercik-2019-ddgi.pdf#page=8) | Gives grid placement, ray scheduling, surfel shading, filtering, and hysteresis. |
| Diffuse query | [§5.2, PDF pp. 12–15](majercik-2019-ddgi.pdf#page=12) | Defines the visibility-aware eight-probe interpolant. |
| Density and quality | [§6.1, PDF pp. 16–19](majercik-2019-ddgi.pdf#page=16) | Separates spatial probe density from angular probe resolution. |
| Assumptions and limitations | [§7, PDF pp. 25–26](majercik-2019-ddgi.pdf#page=25) | States the mutual-visibility assumption behind interpolation. |

<a id="section-2019-representation"></a>
## Probe representation — §3, PDF pp. 6–7

The reference layout stores three directional fields per probe:

- diffuse irradiance in an 8×8 octahedral map, using `GL_R11G11B10F` in the described configuration;
- mean hit distance in a 16×16 octahedral map;
- mean squared hit distance in the same 16×16 map, with both moments packed in `GL_RG16F`.

Probe tiles are packed into 2D atlases. One-pixel gutters duplicate edge data so hardware bilinear filtering does not cross octahedral or probe-tile boundaries. The angular resolutions are intentionally small: spatial interpolation and moment filtering, rather than a detailed per-probe environment map, provide the final smooth field.

<a id="figure-2019-3"></a>
### Figure 3 — atlas and octahedral layout

[PDF page 7](majercik-2019-ddgi.pdf#page=7) shows the irradiance and depth atlases, gutter texels, and alignment padding. It is the quickest visual reference for the storage model.

<a id="section-2019-update"></a>
## Dynamic update — §4, PDF pp. 8–11

The frame update has three conceptual passes:

1. Trace a set of spherical rays from every active probe and retain surface hit position and normal.
2. Shade those hit surfels with the same direct-plus-indirect shader used for visible surfaces. The indirect part reads the previous frame's probes.
3. Convolve ray radiance into irradiance texels and ray distance into first/second-moment texels, then blend with the old values using hysteresis.

Important details:

- The sample directions follow a per-frame, randomly rotated Fibonacci sphere pattern.
- The paper's reference results update every probe with the same number of rays, favoring a simple conservative baseline over update scheduling.
- Irradiance uses a clamped-cosine filter; visibility moments use a sharper cosine-power filter.
- An update contribution below the selected cosine-power threshold (0.001 in the paper) is ignored.
- The reported hysteresis range is 0.85–0.98. It amortizes higher-order bounces but introduces lag after lighting or visibility changes.
- Probe-hit surfels are shaded recursively from the previous probe state, so multi-bounce diffuse illumination converges across frames instead of within one frame.

### Grid placement guidance — §4.1, PDF pp. 8–9

The paper uses an axis-aligned uniform 3D grid and associates each shading point with the eight vertices of its containing cell. It recommends at least one complete eight-probe cage inside each room-like region. For human-scale scenes, the authors found roughly 1–2 m spacing sufficient in their tests. That is empirical guidance, not a guarantee: the query still assumes surrounding probes capture comparable incident light when mutually visible.

<a id="section-2019-query"></a>
## Visibility-aware diffuse query — §5.2, PDF pp. 12–15

At a surface position, the algorithm finds the surrounding eight-probe cage. It samples each probe's irradiance in the surface-normal direction and builds a weight from five ideas:

1. **Backface weight.** A probe below the surface tangent plane fades out as the normal-to-probe dot product approaches zero. This rejects probes clearly behind the receiver but does not, by itself, prove line of sight.
2. **Low-intensity perceptual weight.** Very low irradiance values receive an additional monotonic reduction because small leaks are especially visible in dark regions.
3. **Moment visibility.** Mean and squared hit distance yield a variance estimate. A variance-shadow-map-style Chebyshev test estimates whether the biased shading point is visible from the probe.
4. **World-space query bias.** The visibility query position is moved away from the geometric discontinuity using the surface normal and view/probe direction. This avoids sampling the visibility function exactly at its unstable boundary.
5. **Trilinear weight.** The ordinary cell-coordinate weight remains the spatial interpolation basis. The other factors modulate it before normalization.

Weights are bounded with epsilon guards before normalization. That matters when all eight candidates are nearly rejected; a mathematically reasonable visibility test can still produce unstable normalized contributions if the fallback behavior is not defined.

<a id="figure-2019-6"></a>
### Figure 6 — geometry of a query

[PDF page 13](majercik-2019-ddgi.pdf#page=13) labels the shading point, normal, probe direction, and directional mean distance. Use it when checking whether an implementation samples the depth field in the probe-to-surface direction and applies bias in the intended space.

<a id="figure-2019-7"></a>
### Figure 7 — leak-removal ablation

[PDF page 14](majercik-2019-ddgi.pdf#page=14) progressively adds backface rejection, moment visibility, and normal bias in a dark closed room. The figure is useful as an acceptance-test template: isolate every factor instead of judging only the fully combined result.

<a id="figure-2019-8"></a>
### Figure 8 — low-resolution ray visibility versus moments

[PDF page 16](majercik-2019-ddgi.pdf#page=16) contrasts classic probes, a low-resolution direct visibility lookup, and variance-based visibility. It demonstrates why filtering sparse directional depth as moments can be smoother than treating a coarse direction as a hard binary ray test.

<a id="section-2019-density"></a>
## Density, angular resolution, and visible grid structure — §6.1, PDF pp. 16–19

The paper's experiments separate two resolutions:

- **Spatial density:** how many probes cover the scene.
- **Angular resolution:** how many texels each probe uses for irradiance and visibility.

The reported images show spatial density having the larger effect. Low angular resolution can still approximate smooth diffuse lighting well, whereas too few probes leave subtle leaks and incorrect indirect shadows. This distinction is important when diagnosing a grid-like artifact: increasing density changes both the physical sample locations and the spatial interpolation cells; increasing per-probe angular resolution changes only the directional approximation.

The paper's core assumption, stated in §7, is that incident light at the surface resembles incident light at surrounding probes **when the surface and probe are mutually visible**. Error grows as density falls, but increasing density is not automatically monotonic if the visibility term, normalization, or per-probe signal is discontinuous. A denser grid can expose those discontinuities at a smaller, more obvious cell scale.

<a id="figure-2019-9"></a>
### Figures 9–11 — density and quality

[PDF pages 17–18](majercik-2019-ddgi.pdf#page=17) compare probe count, per-probe resolution, and indirect-shadow convergence. These figures support testing spatial density and angular resolution independently.

## Practical diagnostic checklist for Re: Flora

When a regular pattern appears after changing probe spacing, isolate the query factors in this order:

1. Render raw irradiance from each candidate probe with visibility forced to one. If the blocks remain, the discontinuity is in probe lighting/update data.
2. Hold probe irradiance constant and visualize only normalized visibility weights. If the blocks appear, the selection/interpolation path is creating cell boundaries.
3. Visualize the eight unnormalized weights and their sum. A tiny sum followed by normalization can amplify one probe into a hard region.
4. Split trilinear, backface, and directional-visibility terms into separate debug views, following the Figure 7 ablation pattern.
5. Compare two axes independently: change spatial spacing without changing ray count/angular resolution, then change directional resolution without changing spacing.
6. Check the direction convention against Figure 6 and verify that the stored distance represents the probe-to-surface ray, not its inverse.
7. Test a room that has at least one full probe cage entirely inside it; otherwise the original placement assumption is knowingly violated.

## Limits to carry forward

- Temporal hysteresis trades stability for response time; abrupt changes can appear to flow through the field.
- Probe visibility is a filtered statistical estimate, not exact segment visibility.
- Low density violates the local-similarity assumption even if visibility is perfect.
- A normal/view bias can suppress self-shadow error, but too much bias can move the query across thin geometry.
- The 2019 claim that a naïve uniform grid works without manual placement was refined by the 2021 production paper, which adds probe relocation and states.
