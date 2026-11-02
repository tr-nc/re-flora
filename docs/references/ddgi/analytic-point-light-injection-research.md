# Analytic point lights in a shared DDGI field

Research snapshot: 2026-11-03. Code inspected at `c4a8f953` (rendering unchanged from `28184813`). **Research and proposed experiments, not an implemented feature or measured optimization.**

[Visual report](../../reports/ddgi-point-lights/index.html) · [Earlier area-emitter research](direct-light-in-fields-research.md)

## Conclusion

**Explicitly evaluate lights at probes, then project their directional contributions into the existing irradiance representation.** An ideal point light need not become emissive geometry, and the representation need not become spherical harmonics (SH). For this project's current small active light list, deterministic probe–light connections are the clearest first experiment; importance sampling is a subsequent scaling choice.

There is a particularly direct first-party precedent: Unity's `LightProbes.bakedProbes` example computes direction and attenuation from every probe to every point light and calls `SphericalHarmonicsL2.AddDirectionalLight`. It explicitly says that at a probe the point light is equivalent to a directional light. The example can run in the editor or at runtime. **It does not perform shadow or transmission queries**, and its attenuation is a Unity-specific approximation, not a physically exact inverse-square implementation. [S2, S3]

This establishes feasibility, not losslessness: summing light contributions removes light enumeration from grass shading, but probe interpolation cannot preserve arbitrary near-source falloff or sharp visibility discontinuities. A fixed receiver lookup does not make field updates independent of light count.

## 1. Why random DDGI rays cannot find an ideal point light

PBRT describes point lights as delta-position sources. `SampleLi` explicitly connects the shading point to the light, returns intensity divided by squared distance, and uses a delta sampling convention. `PDF_Li` returns zero because another continuous directional sampling process has zero probability of hitting an infinitesimal emitter. [S1]

Therefore:

- More ordinary probe rays do not solve ideal-point-light capture.
- Sampling a wall illuminated by the point light captures the wall's reflected light, **not the point light's direct contribution at the probe**.
- Analytic injection is a light-sampling estimator, not permission to mix arbitrary directed rays into the existing uniform-sphere estimator.
- Making a point light into a sphere creates a finite emitter with different near-field behavior and penumbrae. It is an artistic/modeling choice, not an exact representation conversion. [S1; derivation]

## 2. What is actually stored?

For a probe position `p`, light position `l_i`, and a normal direction `n` represented by an irradiance texel:

```text
r_i = |l_i - p|
w_i = (l_i - p) / r_i
C_i(p) = T(p, l_i) * I_i / r_i²
E_direct(p, n) = Σ_i C_i(p) * max(0, n · w_i)
```

Here `I_i` is physical radiant intensity, and `T` is opaque visibility or RGB transmittance under the chosen transport approximation. Range windows and near-field clamps, when used, are part of the authored light model. A straight connection with attenuation through glass is not a general refractive-caustics solver. [S1; derivation]

A point source is singular in incoming direction, but its cosine-convolved diffuse response is an ordinary clamped-cosine lobe. We can encode that lobe as SH coefficients or directly evaluate it for each direction in an octahedral irradiance map. The Unity API is an existence proof for SH injection, not a requirement to change this project's representation. [S2, S3; derivation]

### Suitable integration shape for this renderer

1. For each updated, valid probe, form each relevant probe-to-light connection.
2. Evaluate direction, the authoritative attenuation, and opaque/RGB transport **once per connection**, not once per irradiance texel.
3. Reuse that result to accumulate the cosine lobe across the existing octahedral normal directions.
4. Publish a revision-consistent direct field; grass queries its interpolated response instead of looping over those lights.

Probe origins are volume points, not surface hits. Reusing a receiver helper must not invent a surface normal/bias that changes visibility for each output texel. Embedded/relocated probes need the same valid-position ownership as other DDGI updates.

### Units and sampling measures must remain explicit

`shader/slang/ddgi_irradiance_filter.slang` uses `4/N * Σ L(w) max(0,n·w)` and explicitly documents its stored quantity as **E/π**. For physical intensity, the direct formula above must be divided by π to enter that representation. A delta light contribution is a separate discrete term, not another uniform-sphere sample multiplied by `4/N`.

However, the current local-light helper returns an authored intensity × attenuation × cosine term; grass adds it to the environment response directly, and `ddgiTransportHitRadiance` also multiplies local lighting by albedo without an explicit `1/π`. **Do not mechanically divide current `light.intensity` by π and assume brightness will match.** Establish whether that input already includes the diffuse normalization, define the conversion once, and verify against a one-light, unoccluded reference. This is a unit-contract issue, not justification for an arbitrary exposure correction.

If environment ray directions become nonuniform, their estimator needs the actual directional PDF/MIS weights. Discrete light selection additionally needs its own selection PMF. Directional PDFs and discrete light PMFs are not interchangeable. [S1, S4]

### Do not inject light distances into DDGI visibility moments

The existing visibility field describes scene geometry sampled around a probe. A light's distance, or a shadow ray conditioned on selecting that light, is not an ordinary scene-distance sample. Keep direct-light contribution estimation separate from geometry-distance moment estimation unless the latter has its own correctly weighted redesign. Otherwise direct injection can corrupt interpolation visibility rather than improve it. [Project integration inference]

## 3. Candidate comparison

| Approach | How a point light enters | Strength | Main limit | Recommendation |
| --- | --- | --- | --- | --- |
| Deterministic probe-side injection | Explicit connection to every relevant light, then cosine projection | No missed-light sampling noise at the probe; easiest brightness/visibility reference | Update work scales with relevant light count; interpolation remains approximate | **First experiment with the existing small list** |
| Sampled probe-side injection | Choose `K` lights with known PMF and accumulate weighted connections | Bounded connection budget; compatible with many-light samplers | Variance, temporal convergence, sampler maintenance; shadows still required | Second stage if deterministic updates cost too much |
| Point → finite emissive geometry | Ordinary probe rays hit a newly sized source | Reuses emission transport | Changes light model; tiny bright geometry still has high variance | Only if finite source shape is intentionally desired |
| Cache visibility but evaluate attenuation/direction at each grass sample | Keep per-light information and apply exact receiver geometry | Better near-field direction/falloff | Grass still enumerates lights; no longer a fixed-size summed field | Explicit hybrid only, not the requested end goal |

**Deterministic** does not mean spatially exact. It means the chosen point-light model is evaluated without stochastic light selection at probe positions.

### Many-light scaling: where ReGIR fits

For independent light draws with selection probability `q_i(p) > 0` wherever a contribution is nonzero:

```text
E_hat(p, n) = (1/K) * Σ_samples [C_i(p) * max(0,n·w_i) / q_i(p)]
```

This is the ordinary discrete-light estimator before spatial interpolation/temporal approximation. Reservoir resampling requires its own normalization and validity rules; simply dividing a reused reservoir by the original proposal probability is not sufficient. [S4, S5]

PBRT offers uniform, power-weighted, and spatial BVH light selection. A global power distribution need not suit a spatially distributed scene. NVIDIA's RTXDI documentation describes ReGIR as a world-space light-sampling structure and explicitly names shading surfaces for RTXGI probes as a use case. Its sampling function **does not evaluate shadowing**. [S4, S5]

Important distinction: the documented RTXGI use case shades **probe-ray hit surfaces**. Applying the same family of samplers to **probe-center direct-field injection** is our proposed extension, not a claim that RTXDI already implements this cache. ReGIR stores candidate light samples, not final irradiance. It can help update a field; it is not itself the field grass should simply read.

IS-DDGI (2023) is related work on MIS-based DDGI ray allocation and temporal reuse. Its author abstract supports importance-based ray strategies, but does not establish a ready-made analytic-point injection design. The paper PDF could not be extracted in this research, so no equation-level claim or speedup prediction is based on it. [S6]

## 4. Code-specific findings

These are observations of the inspected revision, not a maintained feature inventory.

- `src/lighting/mod.rs`: GPU capacity is **8**. `PointLight.source_radius` is documented as a near-field attenuation clamp. Nonzero `source_half_extents` identifies an aggregated voxel emitter's finite emitting envelope.
- `src/lighting/voxel_emissive.rs`: aggregate voxel emitters populate that envelope. **The Rust name `PointLight` does not imply every selected stone lantern is an ideal delta light.** Preserve finite-emitter semantics when comparing the existing scene.
- `shader/slang/local_lighting.slang`: `evaluateUnshadowedPointLight` uses a finite-support range window and `max(distance², source_radius²)`. Reuse this model deliberately; Unity's example attenuation is not a drop-in replacement.
- `shader/slang/ddgi_irradiance_filter.slang`: the existing octahedral irradiance map already supports normal-dependent evaluation. There is no representational need to introduce SH solely for point lights.
- `shader/slang/ddgi_probe_trace.slang`: `ddgiTransportHitRadiance` adds explicit local lighting to sampled history before multiplying by albedo, and separately adds `surfaceEmission`.
- `shader/slang/flora_lighting_cache.comp.slang`: grass obtains the DDGI/environment response, then adds explicit local lighting. This is the receiver-side work targeted for replacement.
- `src/tracer/local_light_live_publication.rs`: light source, registry, and live revisions already exist. Any cached direct channel must coordinate its publication with light and geometry revisions rather than silently mixing new lights and old visibility.

### Double counting has two separate locations

**At grass:** if its field now contains lamp direct illumination, the same lights must stop contributing through the old per-grass loop.

**Inside DDGI feedback:** blindly adding direct injection to the atlas used by `ddgiTransportHitRadiance` means the hit surface receives explicit local light **and** a history lookup now containing that same direct light. That duplicates the same contribution before another bounce. Removing only the grass loop does not fix this.

Two coherent ownership options:

1. **Separate logical direct channel, shared probe grid.** Keep the existing transport history's role; at final receiver shading combine it with a separately identified direct response. DDGI hit shading continues to use explicit local lighting and must not recursively read the direct channel as additional indirect light. This also permits fast direct invalidation without discarding useful bounce history. Logical separation may require an extra atlas/channel, but not a second probe-placement or scene-tracing system.
2. **A total-light transport field.** Make recursive hit shading use the total field instead of also adding explicit lighting for those sources. This changes bounce accuracy and convergence and must be evaluated as a broader algorithm change, not a blind atlas addition.

For a first analytic-light experiment, prefer **option 1's explicit ownership**, settling physical storage only after confirming the update/publication requirements. The earlier finding that area emission can already enter DDGI remains valid: it did not imply that all additional analytic input can share recursive history without accounting changes.

For finite voxel emitters, there is a further overlap between hit-surface emission and explicit area-light sampling. Use source/path partitioning or a valid combined estimator (e.g. MIS); do not sum two full estimators of the same emitter contribution. An initial isolated analytic-light fixture avoids that overlap and makes normalization testable. [S1, S7; project inference]

## 5. Quality limits that sampling cannot eliminate

### Near-field position dependence

A field sums contributions at probe locations. Interpolating that sum cannot reconstruct exact inverse-square falloff or light direction near every individual source. Increasing directional map resolution alone does not fix spatial undersampling.

Illustrative analytic counterexample, **not a Re: Flora measurement**: two probes separated horizontally by `h`, an upward-facing receiver at their midpoint, and an ideal point light `0.1h` above the receiver. With no occluders/clamps, each probe's upward irradiance is `I * 0.1 / (0.25 + 0.01)^(3/2) / h² ≈ 0.754 I/h²`, while the receiver's exact value is `100 I/h²`. Linear interpolation of the two identical probe values misses the narrow peak by about **133×**. This demonstrates the limitation, not the expected error of the project's eight-probe query.

Use sufficient local spatial density, an intentionally finite source, or explicitly accept a selective direct-shading fallback. A fallback that loops over nearby lamps trades away strict fixed-cost receiver shading; make that trade visible.

### Probe visibility is not receiver visibility

Even a perfect probe-to-light shadow query establishes visibility **at the probe**. A grass sample behind a lamp base or thin wall may see a different result. DDGI visibility weighting between receiver and probe helps avoid borrowing across geometry; it does not recreate every receiver-to-light shadow boundary. [S8; project inference]

Glass is especially important in the measured scene: replacing its existing RGB straight-transport approximation with binary visibility changes the effect, not merely its cost. Retain the transport model for comparable A/B results, and test lights/occluders before and behind the endpoint.

### Dynamic changes

Direct lighting usually needs faster response than slowly converged bounce lighting. A moving light changes attenuation and direction; a wall edit changes connections even if the light is unchanged. A point-light direct sample should be recomputed/replaced, not added permanently each frame. Define invalidation, old/new affected bounds, valid revision publication, and a maximum acceptable stale interval. Standard history blending alone can leave ghost illumination after a light turns off. [Design requirements; not implemented]

## 6. Proposed experiment and acceptance evidence

**Stage A: establish representation and correctness before optimizing its update.**

- Isolated analytic point light, empty space, fixed exposure and known material; compare exact per-receiver shading and injected field at probe positions and between probes. Test distance, normal direction, source-radius clamp and range boundary. Resolve units first.
- Add an opaque wall, thin wall, lamp base, moving shadow boundary and colored glass. Record spatial error, not only average brightness.
- Toggle/move/change the light and edit geometry. Observe first response and time to stable state, including switch-off ghosting.
- Preserve sun's existing direct-shadow path. Switch only the intended local-light contribution with a Debug A/B checkbox (unchecked original, checked field experiment). Keep both modes at runtime.
- Separately test actual finite voxel lanterns with their original shape/transport and explicit emission-path ownership. Do not call collapsing them to points a quality-preserving speedup.

**Stage B: measure cost after visual review.**

Let `G` be grass samples, `B` updated probes, `L` relevant lights, and `M` irradiance directions. Current direct connections scale with `G × L` (and finite-source taps). Deterministic injection can move expensive connections to `B × L`; cosine projection adds roughly `B × L × M` cheap arithmetic. Probe history and scene-ray work remain. An implementation that retraces visibility for every texel wastes the main sharing opportunity.

Measure grass query, direct-field update, existing DDGI transport/update, and total GPU frame time separately in Release mode. First compare the same effective 1/8 lights; testing 32/128 requires explicit capacity/selection work and proof that all requested sources participate. Then consider fixed-budget light sampling and measure both cost and convergence/variance. No speedup is claimed by this research.

## Sources and evidence limits

- **S1 — PBRT v4, Point Lights (§12.3):** [official text and implementation](https://www.pbr-book.org/4ed/Light_Sources/Point_Lights). `SampleLi`, `PDF_Li`, inverse-square intensity, delta distributions. Directly read.
- **S2 — Unity 2020.3, `LightProbes.bakedProbes`:** [official runtime/editor example](https://docs.unity3d.com/2020.3/Documentation/ScriptReference/LightProbes-bakedProbes.html). `AddPointLight` computes attenuation/direction and calls `AddDirectionalLight`. Directly read; no shadowing in the shown code.
- **S3 — Unity, `SphericalHarmonicsL2.AddDirectionalLight`:** [official API](https://docs.unity3d.com/2019.4/Documentation/ScriptReference/Rendering.SphericalHarmonicsL2.AddDirectionalLight.html). Direction points from probe to light and should be normalized. Directly read. SH truncation does not promise exact spatial or angular reproduction.
- **S4 — PBRT v4, Light Sampling (§12.6):** [official text/code](https://www.pbr-book.org/4ed/Light_Sources/Light_Sampling). Uniform, power-weighted and BVH sampling, selection PMFs. Directly read relevant sections.
- **S5 — NVIDIA RTXDI integration guide:** [official repository](https://github.com/NVIDIA-RTX/RTXDI/blob/main/Doc/Integration.md#regir). World-space sampling/ReGIR, RTXGI probe-hit use case, separate shadow evaluation. Directly read. Repository `main` is mutable; this is a dated research snapshot, not a pinned dependency.
- **S6 — Liu et al., *Importance-Based Ray Strategies for Dynamic Diffuse Global Illumination* (2023):** [author page](https://allenliuzihao.github.io/IS-DDGI/) · [DOI](https://doi.org/10.1145/3585500). Author abstract read; PDF extraction failed and publisher retrieval returned HTTP 406. Used only as a related-work pointer, not evidence of an implemented direct-point field.
- **S7 — PBRT v4, A Better Path Tracer:** [official text](https://www.pbr-book.org/4ed/Light_Transport_I_Surface_Reflection/A_Better_Path_Tracer). Distinct light/BSDF sampling and MIS accounting. Referenced for avoiding duplicate emitter estimators, not a ready-made DDGI integration.
- **S8 — Majercik et al., DDGI (2019):** [publisher paper](https://jcgt.org/published/0008/02/01/paper-lowres.pdf), especially visibility-aware interpolation and §5.1. [Existing source notes](majercik-2019-ddgi.md). Area-emitter support is established by the earlier research; this note's analytic-light injection is a separate proposal.

The Stanford irradiance-environment-map page and Sloan SH PDF were also sought, but could not be reliably retrieved in this session; no detailed claim relies on those fetches. A search result for an unrelated 2005 importance-sampling paper was discarded after opening it. Search summaries were not treated as primary evidence.

Only original notes, presentation HTML and official links are retained in the repository; no paper originals or full-text conversions are added. No rendering/configuration changes or new image/audio assets accompany this research.
