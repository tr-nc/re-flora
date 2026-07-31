# Improving Probes in Dynamic Diffuse Global Illumination — reading notes

## Bibliographic and rights record

- Author: Dominik Roháček
- Supervisor (not a coauthor): Tomáš Iser
- Venue: *Proceedings of CESCG 2022: The 26th Central European Seminar on Computer Graphics* (non-peer-reviewed), 2022
- Primary source: <https://cescg.org/cescg_submission/improving-probes-in-dynamic-diffuse-global-illumination/>
- Local source: [official CESCG PDF](rohacek-2022-improving-probes-ddgi.pdf)
- Author spelling corroboration: [Charles University repository record](https://dspace.cuni.cz/handle/20.500.11956/171782?locale-attribute=en)
- License: no explicit reuse license was found in the CESCG publication page or PDF as of 2026-08-01

Because adaptation permission is unclear, this file is an original technical summary rather than a full-text conversion. The paper is a seven-page CESCG technical report; a longer Charles University diploma thesis with the same title also exists, but it is not the PDF archived here.

## One-paragraph contribution

Roháček implements DDGI in an RTX renderer and concentrates on bad probe positions. The proposed pipeline detects probes likely trapped in geometry, moves them through a simple geometry-unaware cardinal-axis spiral, corrects spatial weights for the offsets, and rejects directional depth samples that reach beyond the region a probe cage can legitimately represent. The result targets localized, highly visible errors in architectural scenes, accepting a small added GPU cost.

## Section and page map

| Topic | Paper location | Why it matters |
| --- | --- | --- |
| DDGI recap | [§2.1, PDF pp. 1–2](rohacek-2022-improving-probes-ddgi.pdf#page=1) | Summarizes backface, moment visibility, and trilinear weights. |
| Failure model | [§3, PDF p. 3](rohacek-2022-improving-probes-ddgi.pdf#page=3) | Explains why uniform grids place probes inside or too near surfaces. |
| Dead-probe detection and movement | [§3.1–3.2, PDF p. 3](rohacek-2022-improving-probes-ddgi.pdf#page=3) | Reuses probe rays to detect and relocate invalid samples. |
| Offset-aware filtering | [§3.3, PDF p. 4](rohacek-2022-improving-probes-ddgi.pdf#page=4) | Shows why nominal trilinear weights fail after moving probes. |
| Depth-sample rejection | [§3.4, PDF pp. 4–5](rohacek-2022-improving-probes-ddgi.pdf#page=4) | Directly addresses roof/wall leaks from coarse filtered depth. |
| Results | [§4, PDF pp. 5–6](rohacek-2022-improving-probes-ddgi.pdf#page=5) | Gives error maps and measured pass costs. |
| Limitations and future work | [§5, PDF pp. 6–7](rohacek-2022-improving-probes-ddgi.pdf#page=6) | Discusses cascades, ray budgeting, and dynamic hysteresis. |

<a id="section-2022-failure"></a>
## Failure model — §3, PDF p. 3

A uniform grid frequently puts probes inside geometry or very close to large planar surfaces, especially in coordinate-aligned architectural content. A buried probe cannot provide useful irradiance. A probe almost touching a surface may devote nearly half of its small directional map to that nearby patch, wasting angular coverage and making filtering sensitive to local depth extremes.

The paper's placement goal is not merely “outside geometry.” It aims to keep every useful probe at least a user-defined distance from its nearest surface, while keeping the offset below half a cage side so the sample remains associated with its original cell topology.

<a id="figure-2022-4"></a>
### Figure 4 — near-surface angular waste

[PDF page 3](rohacek-2022-improving-probes-ddgi.pdf#page=3) illustrates how a probe almost touching geometry spends many directions on a small surface patch. This can produce a valid-but-poor sample even when no probe is technically inside the wall.

<a id="section-2022-dead"></a>
## Dead-probe detection — §3.1, PDF p. 3

The implementation reuses existing update rays instead of introducing a new geometry query. Backface first hits are marked with negative distance. A later pass counts rays that hit geometry closer than a user threshold; once more than half of the probe's rays satisfy the dead criteria, the probe is considered unusable.

The classification is temporally filtered with hysteresis. That prevents a probe from moving immediately because of one noisy frame, but it also means a test must run long enough for classification and movement to settle.

This heuristic assumes a high backface fraction is evidence of being enclosed. Non-manifold, intersecting, or one-sided production geometry makes a single-ray test insufficient, which is why the algorithm uses a majority rather than one backface.

<a id="section-2022-moving"></a>
## Geometry-unaware movement — §3.2, PDF p. 3

Each dead frame advances an integer counter. Modulo six chooses one of the ±X, ±Y, or ±Z directions; integer division determines distance from the nominal probe position. Together those candidates form a simple cardinal-axis spiral. The maximum displacement is a user percentage of the cage side and must stay below 50%.

Advantages:

- no SDF, neighborhood search, or additional geometry structure;
- easy to update when geometry changes;
- deterministic bounded candidates.

Risks:

- the candidate path is not informed by the nearest safe surface;
- movement may take multiple frames;
- cardinal candidates can correlate with an axis-aligned scene and grid;
- changing positions invalidates nominal-grid trilinear weights.

<a id="figure-2022-5"></a>
### Figure 5 — moved dead probes

[PDF page 4](rohacek-2022-improving-probes-ddgi.pdf#page=4) compares a room before and after four buried probes are moved. It also visualizes dead versus usable probes, making it a useful debug-view model.

<a id="section-2022-filtering"></a>
## Offset-aware filtering — §3.3, PDF p. 4

Ordinary trilinear interpolation assumes the eight samples remain at the cage corners. After relocation, applying nominal weights can violate two invariants:

- the eight normalized weights should sum to one;
- every individual weight should remain in [0, 1].

Breaking the sum produces visible cage boundaries. Leaving the interval permits oversaturation or subtraction. The proposed correction expresses both the fragment and each relocated probe in normalized cage coordinates, adjusts the axial interpolation position by the probe offset, computes a linear weight per axis, and multiplies the three axial weights.

The equations in the paper are compact and depend on its corner-coordinate convention. Before implementing them, reproduce the one-dimensional case from Figure 6 and verify endpoint weights, non-negativity, and a unit sum numerically. Do not transplant the formula without matching the paper's definition of `probeCoordNorm` and signed offset.

<a id="figure-2022-6"></a>
### Figure 6 — interpolation invariants

[PDF page 4](rohacek-2022-improving-probes-ddgi.pdf#page=4) plots two one-dimensional probe weights and their sum before and after offset-aware correction. It is a compact unit-test oracle for relocated probes.

<a id="section-2022-depth-rejection"></a>
## Depth-sample rejection — §3.4, PDF pp. 4–5

The most directly relevant leak in this paper comes from the low-resolution directional depth field. Depth rays are convolved into nearby octahedral texels with a sharpened lobe. A long hit from a direction nearly parallel to a roof can become a local extreme in adjacent texels. A later visibility query can then incorrectly treat the far side as visible and light through the roof.

The proposed guard rejects depth samples longer than the main diagonal of the probe cage, because a probe in that cage should not need a farther hit for interpolation within the cage. When probes may move, the threshold is enlarged by the maximum possible offset so legitimate queries to a relocated probe are not discarded.

This is a **support bound**, not a generic distance clamp. Its validity depends on the consumer sampling only within the cage represented by those probes. If a renderer reuses the same visibility field outside that support, clips or cascades volumes differently, or selects nonlocal fallback probes, the bound must be re-derived.

<a id="figure-2022-7"></a>
### Figures 7–8 — roof leak mechanism

[PDF page 5](rohacek-2022-improving-probes-ddgi.pdf#page=5) shows a sunlit roof leaking before rejection and the corrected image. The accompanying diagram depicts a depth texel whose filtered directional support includes a distant surface beyond the local cage.

<a id="section-2022-results"></a>
## Results — §4, PDF pp. 5–6

The error maps compare four configurations: original DDGI, probes moved to surfaces, probes kept at least 15 cm from surfaces, and offset-aware filtering. The paper's goal is to spread error rather than leave a concentrated hotspot; the final configuration removes the most visually obvious island in the shown scene.

The reported timing scene uses an 11×10×15 probe grid and 64 rays per probe per frame. The table reports:

| Pass | Time |
| --- | ---: |
| Ray cast | 2.71 ms |
| Irradiance and depth update | 0.02 ms |
| Dead-probe detection | 0.05 ms |
| Probe offset | 0.10 ms |
| Indirect-light sampling | 0.48 ms |
| Total | 3.36 ms |

The two added passes total about 0.15 ms in that specific renderer and scene. These numbers are context, not portable performance evidence for Re: Flora.

<a id="figure-2022-9"></a>
### Figure 9 — error-map ablation

[PDF page 6](rohacek-2022-improving-probes-ddgi.pdf#page=6) is the paper's main acceptance comparison. Note that each subfigure uses its own scale, so compare spatial concentration as well as absolute values.

## Correctness-first experiments for Re: Flora

1. **Near-parallel depth outlier view.** For each probe-to-wall query, show the four directional samples, decoded distances, bilinear weights, and selected surface distance. Look for one long sample dominating across a texel boundary.
2. **Cage-support clamp.** Temporarily reject stored hit distances beyond the local cell diagonal, adjusted for any probe offset. If the grid pattern vanishes, the failure is likely directional support contamination rather than irradiance SH.
3. **Invalid-probe visualization.** Color probes by frontface/backface ratio and minimum hit distance. A high-density grid can place more probes in thin walls or very near planes even though spacing is smaller.
4. **Position-aware weights.** If Re: Flora ever relocates probes, assert non-negative weights and a unit sum over a dense set of points in every affected cell.
5. **Axis correlation.** Rotate the test room relative to the probe grid. If the pattern rotates with the grid instead of the geometry, inspect octahedral sampling, cardinal directions, and cell interpolation.
6. **Separate hard visibility from filtered moments.** The Roháček leak arises from filtered low-resolution depth. Re: Flora's directional visibility may have a different representation, so reproduce the causal intermediate value before importing the proposed clamp.

## Limits to carry forward

- The paper is explicitly marked non-peer-reviewed.
- Dead-probe detection and movement are heuristics, not a proof of valid placement.
- The movement search is axis-aligned and geometry-unaware.
- The depth-rejection bound assumes local cage queries.
- The paper identifies probe cascades, adaptive ray budgets, and variance-driven hysteresis as future work rather than validated components.
