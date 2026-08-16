# Re: Flora DDGI stages, secondary rays, and modern update strategies

Research date: 2026-08-16

## Short answer

`S0`, `S1`, and `S2` are Re: Flora transport-stage labels. In this repository, `S` is best read as
**Stage**: the code calls the state enum `DdgiFieldStage`, while the paper does not define an `S0` or
`S1` notation. It is not a sample count and it is unrelated to a random-number seed.

- **S0 / `SeedSky`** is a complete, current-geometry field containing authored-sky radiance from
  probe-ray misses and current-geometry visibility. Terrain hits contribute no reflected radiance.
- **S1 / `SingleBounce`** shades terrain hits with current direct sun plus irradiance queried from
  immutable S0. It is the first field that a new staging volume may publish.
- **S2 and later / `Feedback`** shade hits from the previous complete field. Each complete update can
  propagate diffuse energy through one more surface interaction while retaining lower-order light.

`SeedSky` means a numerical **initial condition** for the recursive transport, like seeding an
iterative solver with its first known energy. It does not mean PRNG seed, and Re: Flora is not
rendering the visible sky image before the terrain. It is manufacturing the local sky-light field
that S1 will query at terrain hits.

The production DDGI path does **not** spawn a diffuse secondary ray at a probe-ray hit. It traces a
probe ray and, for direct sun, one hit-origin shadow ray; recursive diffuse transport comes from
querying the previous complete probe field. A separate GUI-controlled, terrain-only path-tracing
reference does spawn a bounded diffuse path (default 2, maximum 8 bounces), but that reference
bypasses DDGI and is not the production indirect-lighting algorithm.

Consequently, production DDGI can show `sky/sun -> red wall -> ground` color bleeding. S1 is enough
when the red wall is directly lit by sky or sun. A path requiring two ordered diffuse surfaces, such
as `sky/sun -> red wall A -> wall B -> ground`, needs S2 or a later feedback field. The committed
donor and dogleg acceptance cases measure both boundaries.

S2+ is recursive in **transport order**, but it is not yet the paper/RTXGI's full **temporal update
form**. Current feedback deliberately uses fixed directions, zero hysteresis, and deterministic
full-volume source-to-destination iterations. Random rotation, history blending, sleeping, and
adaptive scheduling remain deferred ([current transport policy][transport-current-policy]).

## What exactly are S0 and S1?

The names are project terminology, enforced by [`DdgiFieldStage` and its iteration
invariants][stage-enum]: `SeedSky` must be iteration 0, `SingleBounce` must be iteration 1, and
feedback/terminal states must be iteration 2 or later. The abbreviated `S0`, `S1`, and `S2` labels
used in logs and specifications are therefore **stage/transport-order indices**, not terminology
imported from Majercik et al. 2019.

| Re: Flora label | Complete field contains | Immutable source | Publication role |
|---|---|---|---|
| S0 `SeedSky` | authored sky on ray misses; zero radiance on terrain hits; current visibility | none | private bootstrap source |
| S1 `SingleBounce` | current direct-sun diffuse response plus sky-lit diffuse response | complete S0 | first publishable replacement field |
| S2+ `Feedback` | lower-order light plus another order of recursively propagated diffuse light | previous complete field | publishable until convergence |

S0 is not a generally displayable “ambient-only” approximation. Its zero-radiance terrain hits are
deliberate because it exists to provide a clean incident sky field for hit shading. Completing S0
makes it the private source for S1; only complete, validated S1 is promoted as the first new Active
field ([bootstrap publication][bootstrap-publication]).

## Why sky seeding precedes single bounce

At an S1 probe-ray terrain hit, the shader needs the incident diffuse irradiance at the hit so it can
compute reflected radiance. Re: Flora does not allow a new geometry revision to sample the old
Active field during bootstrap, because that field's irradiance and visibility belong to the old
geometry. S0 supplies a source that matches the new geometry revision.

The source also has to be complete and immutable. Probe updates are split into batches over several
render frames. If an S1 batch both read and overwrote the same atlas, later batches could see data
written by earlier batches. Bounce depth and output would then depend on batch order. The separate
S0 source makes every S1 probe read the same transport order.

Without S0, an implementation would have to choose a different contract:

1. Start from black, which omits locally occluded sky irradiance at S1 terrain hits.
2. Warm-start from old Active, which is faster but can carry stale light and visibility through a
   geometry edit.
3. Read and write one partially updated field, which introduces update-order dependence unless a
   previous-frame snapshot or equivalent barrier is retained.
4. Evaluate the sky integral independently at every hit, which replaces the S0 cost with another
   approximation or additional hit-origin rays.

Thus, two complete passes are not a mathematical requirement of DDGI. They are the cost of Re:
Flora's strict geometry-revision isolation, immutable source/destination rule, and atomic
publication boundary. The old Active field remains visible during this work; the tradeoff is update
latency, not a mandatory scene-wide indirect-light blackout.

This also explains the earlier 64-ray timing: roughly 40 ms for one full iteration versus a median
89.5 ms from rebuild start through S1 promotion. The latter contains one complete S0, one complete
S1, and lifecycle/validation/promotion overhead; it is not the time for a single probe pass.

## Production rays versus path-tracing secondary rays

### Production DDGI

For every probe, the production trace shader emits a fixed spherical-Fibonacci set of probe rays.
A miss records authored sky. A front-face terrain hit from S1 onward computes

`albedo * (current direct-sun irradiance + previous-complete-field irradiance)`

([probe trace and hit shading][probe-trace]). Direct sun visibility is an exact shadow ray against
the same terrain revision. There is no diffuse hemisphere/ray-tree launch from the hit; the design
contract says this explicitly ([transport specification][transport-spec-secondary]).

This still represents approximate multi-bounce diffuse transport. A probe ray that sees a red wall
stores radiance already multiplied by the wall's red albedo. Ground pixels then interpolate that
probe field, so they can receive red indirect light. On the next complete feedback iteration, a
different hit can query that red-containing field and propagate it around another corner. The
acceptance evidence shows the red donor raising red's channel share at S1, while the two-segment
dogleg remains nearly dark at S1 and gains energy at S2 ([transport acceptance][transport-acceptance]).

This is not full path tracing:

- transport is low-frequency diffuse irradiance reconstructed from probes;
- there is no explicit branching diffuse ray tree at each hit;
- there is no glossy/specular secondary transport in the DDGI field;
- probe spacing, moment visibility, and interpolation can blur detail or leak through geometry;
- the current fixed ray directions do not add new angular samples on later feedback iterations.

The complete-volume policy matters here: every feedback destination is withheld until all valid
probes have been updated from one immutable source. Current recursion is therefore deterministic
Jacobi-style field iteration, not an in-place partial history update and not a temporally blended
Monte Carlo estimator.

### Terrain path-tracing reference

When `Path Tracing Reference (Terrain)` is enabled, the terrain shader bypasses the DDGI query. From
the visible primary terrain hit it samples one diffuse direction per bounce, marches to the next
terrain hit, traces a direct-sun shadow ray there, multiplies path throughput by hit albedo, and
terminates on a sky miss or after the configured limit ([terrain reference shader][terrain-reference]).
The GUI defaults to 2 bounces and clamps the control to 0-8 ([GUI configuration][path-tracing-gui]).

So the precise answer to “does Re: Flora support secondary rays?” is:

- **yes in the terrain-only path-tracing reference**, as a bounded stochastic validation path;
- **no in production DDGI in the path-tracing sense**; production DDGI substitutes previous-field
  lookup for an explicit diffuse secondary path.

## Majercik et al. 2019 versus Re: Flora

The paper describes one recurring update loop: trace rays from active probes, shade hit surfels with
direct and indirect illumination, then blend new radiance and distance into probe atlases
([Majercik et al. 2019, section 4][paper-pdf]). Hit shading reads **previous-frame** probe data. This
both amortizes higher diffuse bounces across frames and smooths temporal discontinuities. The paper
reports irradiance hysteresis values from 0.85 to 0.98 (section 4.4), and section 5.3 describes
higher bounces as recursion across frames.

The reference configuration updates every probe every frame, but this is a scheduling choice. The
paper neither defines S0/S1 nor mandates waiting for two complete updates before display. It notes
that gathering multiple bounces before display is an optional adaptation for fixed-view use cases.

Re: Flora agrees with the paper on the core recursive approximation: shade a ray hit using a
previous complete irradiance field. It differs in update and publication policy:

| Property | Majercik 2019 recurring form | Current Re: Flora |
|---|---|---|
| Bootstrap notation | no prescribed S0/S1 barrier | explicit private S0 then publishable S1 |
| History | previous-frame atlas blended with new samples | immutable complete source, distinct complete destination |
| Temporal blend | hysteresis | no irradiance-history lerp in the filter |
| Ray directions | randomly rotated each frame | same fixed Fibonacci directions each iteration |
| Partial visibility | displayed field evolves continuously | old Active remains visible until complete staging promotion |
| Geometry revision | initialization/invalidation left to integration | strict revision ownership and atomic replacement |

Current Re: Flora therefore favors deterministic stage boundaries and edit correctness over fast
temporal convergence. With 64 fixed rays per probe, feedback improves transport order but does not
average a fresh set of angular samples; paper/RTXGI-style rotation plus hysteresis would improve
sampling convergence, at the cost of temporal state and invalidation complexity.

That distinction is the answer to whether Re: Flora has already “synced with the paper”: the
recursive radiance term is present at S2+, but random temporal sampling, hysteresis, and adaptive
probe scheduling are not.

## What current engines commonly do

There is no single engine-standard GI update form. The closest comparison is RTXGI DDGI; Lumen,
Unity APV, and Godot's voxel/SDF systems solve related problems with different representations.
The common real-time pattern is to amortize work through persistent spatial/temporal state rather
than trace a complete multi-bounce path tree for every shaded pixel every frame.

The repository already contains a longer source audit of this question in [DDGI dynamic-scene
update strategies][industry-update-strategies]. That note predates the current S2+ implementation,
so this note supersedes its implementation-status/decision sections; its primary-source audit and
evidence boundary still apply. The paper/RTXGI form is well supported as normal practice **inside
the documented DDGI lineage**, but the public sources do not justify a market-wide claim that “most
games” use one exact algorithm. The table below is a compact architectural comparison, not an
engine-popularity census.

| System | Officially documented form | Relevance to this decision |
|---|---|---|
| NVIDIA RTXGI DDGI | Probe hits combine direct lighting with recursively sampled probe irradiance. A volume computes a new random ray rotation when updated; updating with newly traced data every frame is common, but lower-frequency or asynchronous streaming is allowed. Probe variability can be used to pause converged updates. | Closest direct precedent for previous-field recursion, temporal sampling, and scheduled probe updates. It does not prescribe Re: Flora's two-pass geometry publication barrier. |
| Unreal Engine Lumen | Fully dynamic diffuse GI with “infinite” diffuse bounces, built from multiple trace methods plus Surface/Radiance caches. Epic exposes cache update-speed controls and notes that global lighting changes can take seconds to propagate. | Strong evidence that production real-time GI normally accepts cached, delayed propagation. It is not DDGI and cannot be copied as an S0/S1 algorithm. |
| Unity 6 APV | Automatically placed adaptive probes contain **baked** indirect lighting, support per-pixel sampling, streaming, blending baked Lighting Scenarios, and runtime sky-occlusion updates. Geometry changes can prevent sharing/blending scenarios. | Useful probe-storage and invalidation comparison, but not evidence for a dynamic recursive DDGI update loop. |
| Godot SDFGI | Semi-real-time cascaded GI. One bounce is the default; `Bounce Feedback` enables further propagation. Quality is accumulated over configurable convergence frames, while dynamic occluders/emissive surfaces are limited. | A close conceptual example of feedback across frames and its convergence/feedback risks, but its SDF/cascade representation differs from DDGI. |
| Godot VoxelGI | Static geometry is voxel-baked; lighting can update at runtime. It offers one or two bounces, propagation controls, and dynamic contributors with higher cost. | Demonstrates another common bounded-bounce cached representation, not a previous-frame DDGI atlas. |

Primary-source details are in the [RTXGI integration guide][rtxgi-integration], [RTXGI volume
reference][rtxgi-volume], [Lumen overview][lumen-overview], [Lumen update documentation][lumen-update],
[Unity APV manual][unity-apv], [Godot SDFGI manual][godot-sdfgi], and [Godot VoxelGI
manual][godot-voxelgi].

## Should Re: Flora adopt the paper form?

Not as a wholesale replacement. The current steady-state feedback is already paper-like where it
matters physically, while the two-pass bootstrap solves a repository-specific edit/publication
problem that the paper leaves to the integrator.

Adopting previous-frame temporal accumulation more directly would offer:

- one-update responsiveness instead of waiting for both complete S0 and complete S1 before any
  current-revision lighting is publishable;
- new angular information over time if ray directions are rotated;
- less visible Monte Carlo noise through hysteresis;
- natural amortization for continuous sun/light changes and budgeted probe subsets.

It would also introduce:

- stale-light ghosting and lag after material, sun, or geometry changes;
- an invalidation problem: deciding which probes/history are still owned by the new geometry;
- potential light leaks or mixed revisions if old visibility/radiance is reused too broadly;
- more parameters and tests for hysteresis, disocclusion, reset, and convergence;
- batch-order hazards if “previous frame” is not kept logically immutable.

The existing design offers the inverse tradeoff: deterministic complete fields, exact revision
identity, stable batch-order behavior, and easy S0/S1/S2 acceptance tests, but higher cold-bootstrap
latency and no temporal angular supersampling.

## Recommended hybrid direction

Keep the Active/Staging publication boundary and immutable source/destination semantics. They are
what prevent terrain edits from blacking out the whole scene or exposing a half-updated field. Do
not delete S0 merely to imitate the paper.

If measured update latency or 64-ray angular error becomes unacceptable, test these changes in this
order:

1. Add a per-update global rotation for radiance rays and a controlled history blend for ordinary
   same-geometry feedback. Keep any fixed rays needed for relocation/classification separate, as
   RTXGI does.
2. Preserve hard history resets for changed geometry until a conservative probe/cell invalidation
   rule exists. Reusing old Active everywhere would sacrifice the correctness just gained by the
   staging design.
3. Experiment with warm-starting only demonstrably unchanged regions, while invalid regions retain
   `SeedSky -> SingleBounce`. Continue to publish only complete, revision-consistent fields.
4. Compare fixed workloads in release mode: time to first new field, time to convergence, maximum
   stale-light duration after edits, donor/dogleg transport, portal/wall leakage, and stationary
   temporal variance. The paper-style path is justified only if its latency/quality gain survives
   those correctness gates.

This hybrid follows the modern pattern--persistent history, rotated samples, budgeted convergence--
without giving up Re: Flora's strongest property: an Active field is always complete and a geometry
edit cannot make consumers observe a partially rebuilt field.

## Primary sources

- Zander Majercik et al., [*Dynamic Diffuse Global Illumination with Ray-Traced Irradiance
  Fields*][paper-page], JCGT 8(2), 2019. See sections 4, 4.2-4.4, and 5.3.
- NVIDIA, [RTXGI DDGI Integration Guide][rtxgi-integration] and [DDGIVolume
  Reference][rtxgi-volume], pinned to current upstream commit `f33e496c`.
- Epic Games, [Lumen Global Illumination and Reflections][lumen-overview] and [Lumen update/cache
  settings][lumen-update].
- Unity, [Introduction to Adaptive Probe Volumes][unity-apv], Unity 6.0 manual.
- Godot Engine, [Signed Distance Field Global Illumination][godot-sdfgi] and [Voxel Global
  Illumination][godot-voxelgi], stable manual.

[paper-page]: https://jcgt.org/published/0008/02/01/
[paper-pdf]: https://jcgt.org/published/0008/02/01/paper-lowres.pdf
[rtxgi-integration]: https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Integration.md#tracing-probe-rays-for-a-ddgivolume
[rtxgi-volume]: https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/DDGIVolume.md#updating-a-ddgivolume
[lumen-overview]: https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-global-illumination-and-reflections-in-unreal-engine
[lumen-update]: https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-global-illumination-and-reflections-in-unreal-engine#lumenlightingupdatespeed
[unity-apv]: https://docs.unity3d.com/6000.0/Documentation/Manual/urp/probevolumes-concept.html
[godot-sdfgi]: https://docs.godotengine.org/en/stable/tutorials/3d/global_illumination/using_sdfgi.html
[godot-voxelgi]: https://docs.godotengine.org/en/stable/tutorials/3d/global_illumination/using_voxel_gi.html
[stage-enum]: ../../../src/ddgi/scheduler.rs#L8-L55
[bootstrap-publication]: ../../../src/ddgi/resources.rs#L1368-L1404
[probe-trace]: ../../../shader/slang/ddgi_probe_trace.slang#L61-L180
[transport-spec-secondary]: ../../ddgi_indirect_transport_spec.md#L162-L176
[transport-current-policy]: ../../ddgi_indirect_transport_spec.md#L187-L193
[transport-acceptance]: ../../ddgi_transport_acceptance.md#L45-L60
[terrain-reference]: ../../../shader/slang/tracer.slang#L188-L278
[path-tracing-gui]: ../../../config/gui.toml#L138-L171
[industry-update-strategies]: industry-dynamic-scene-update-strategies.md
