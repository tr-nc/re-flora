# DDGI dynamic-scene update strategies

Research date: 2026-08-01

## Question and terminology

This note compares two ways to shade a front-face surface hit by a DDGI probe ray:

- **A — revision-local deterministic bootstrap.** For one target geometry revision, first build a direct-sky and visibility seed field. Retrace against that same revision, shade each hit from the immutable seed plus current-revision direct lighting, and publish a strict single-bounce final field only after the whole build is ready.
- **B — temporal recursive feedback.** Shade each hit with direct lighting plus indirect irradiance sampled from the previous/final DDGI field. Blend the result into probe history and let second through higher-order diffuse bounces propagate over later updates.

“Previous” in B means a read-only field from before the current probe update. Reading values that have already been overwritten by earlier batches in the same update would make the result depend on update order; none of the sources recommends that design.

## Short answer

For **DDGI specifically, B is the published and RTXGI production path**. The 2019 paper shades probe-hit surfels with direct and indirect illumination from the previous frame, then blends the result with history. It explicitly uses this recursion to amortize multiple bounces over time. The 2021 production paper and the official RTXGI integration/sample preserve that design and add convergence controls, probe states, relocation/classification, scrolling invalidation, and application-controlled scheduling [S1, §4, pp. 8–10; S2, §§4.2–4.3 and 6, pp. 9–18; S4; S6].

No reviewed DDGI paper or RTXGI SDK material describes A as the normal steady-state production algorithm. **A is therefore a project-specific correctness/bootstrap design, not “the paper form” of DDGI.** It is attractive when a geometry revision must not inherit stale visibility, but it deliberately stops at one indirect bounce and spends an extra trace/filter phase.

For editable voxels, the recommended direction is a **hybrid**:

1. publish the new voxel/trace representation under an exact geometry revision;
2. invalidate the probes whose lighting may depend on the edited region (full volume while that dependency set is unknown);
3. bootstrap invalid probes from a revision-local direct-sky/visibility seed, with history disabled and direct lights evaluated at the retraced hit;
4. publish only data carrying the target revision;
5. then switch those probes to B and ramp normal hysteresis back in for stable multi-bounce convergence.

Items 1–5 are a design inference from the source mechanisms, not a recipe supplied by RTXGI. They combine A's revision correctness with B's production-quality temporal multi-bounce behavior.

### Sky-colored probes do not identify A or B

Sky color is orthogonal to the hit-radiance choice. In the official RTXGI sample, a **probe ray miss** stores sky radiance immediately, while a **front-face hit** follows the separate direct-light plus recursive-indirect path [S6]. A renderer can therefore show blue environment light on surfaces even when terrain-hit RGB is always zero: probes that see the sky integrate blue miss radiance, and consumers sample that directional irradiance through the visibility field.

Consequently, if a current implementation has `miss -> sky` and `terrain hit -> 0`, it is earlier than both complete variants in this note. It has environment sampling and visibility, but neither A's terrain-mediated single bounce nor B's recursive terrain-hit feedback.

## A versus B

| Property | A — deterministic seed then retrace | B — previous/final field feedback |
| --- | --- | --- |
| First publish after an edit | Can be defined to contain only the target revision and exactly one diffuse bounce | May contain a controlled mixture of old history and new samples until convergence |
| Bounce count | Strictly one indirect bounce per two-stage build | Second through higher-order bounces emerge across updates |
| Determinism | Straightforward if rays, batching, inputs, and reduction order are deterministic | Possible with deterministic inputs, but the answer also depends on prior history and update count |
| Stale-light risk | Low when the seed and final field are revision-tagged and atomically published | Must be controlled with invalidation, zero/reduced hysteresis, update priority, and revision ownership |
| Cost of a cold rebuild | At least a seed trace/filter plus a bounce retrace/filter | One regular update at a time; cost is amortized over frames |
| Steady-state quality | No natural higher-bounce transport unless more explicit stages are added | Natural approximate multi-bounce transport; this is the DDGI paper/RTXGI behavior |
| Abrupt topology changes | Easy to reason about if affected data is withheld until rebuilt | Faster continuity, but stale light can visibly “flow” until the affected history is replaced |
| Best role | Cold start, edit bootstrap, deterministic tests, ground-truthable single-bounce milestone | Normal runtime steady state after the current geometry revision is established |

### What is source-supported

- The 2019 paper updates active probes from dynamic geometry and lighting every frame. Probe-hit surfels use lighting and probe data from the previous frame, which amortizes multi-bounce transport and smooths discontinuities, at the cost of visible latency after large visibility changes [S1, §4, pp. 8–10].
- Its reference implementation updates every probe with an equal ray count. It experimented with subsets and camera-distance schedules but chose the conservative all-probe baseline because adaptive schedules added scene-specific parameters and bookkeeping [S1, §4.2, p. 9].
- The 2021 production work retains temporal blending. It lowers irradiance hysteresis when a texel changes significantly and sets it to zero for very large changes. For a large object change such as a ceiling collapse, it reports temporarily halving irradiance and visibility hysteresis; its implementation applied this globally and calls affected-probe-only reduction promising future work [S2, §4.3, pp. 10–11].
- RTXGI's integration guide tells the application to perform direct lighting at front-face hits and then sample nearby probe irradiance recursively [S4]. The official sample does exactly that, reading the volume's irradiance/distance/data textures at the hit and adding the diffuse indirect term to direct lighting [S6].
- RTXGI's blend shader weights the previous irradiance using `probeHysteresis`, uses zero history after a black/cleared probe, and lowers hysteresis when it detects a large change [S7].

### What is inference

- A provides a stronger **revision boundary** than the published B baseline because no pre-edit indirect field is allowed into the seed. This follows from A's definition; it is not an evaluated claim in the papers.
- A does not have to become the permanent update path. Using it only to bootstrap invalid probes avoids paying its two-stage cost forever and allows B to provide higher bounces afterward.
- A strict single-bounce field is a useful acceptance milestone, but shipping only A would give up one of the central visual benefits claimed by DDGI/RTXGI: temporally accumulated multi-bounce color transport.

## Dynamic and editable geometry in DDGI/RTXGI

### Scene representation comes first

**Source fact.** RTXGI deliberately leaves ray-tracing acceleration structures, shader tables, and hit-material shading to the application. The integration guide requires the application to maintain the geometry/material representation used for probe rays [S4]. Its sample traces the application's `SceneTLAS` and shades the returned hit [S6].

**Inference for voxel terrain.** RTXGI cannot invalidate stale voxel lighting if probe rays still see an old voxel revision. Re: Flora should make “voxel publication to the trace representation” a prerequisite of every DDGI rebuild token. Whether the representation is a TLAS, SDF, occupancy texture, or custom voxel marcher does not change that ordering requirement.

### Moving objects are normally absorbed temporally

**Source fact.** The 2019 baseline traces dynamic scene geometry and incorporates its changed visibility and shading into continuing probe updates [S1, §4]. The 2021 system does not drive its static-geometry position optimizer from moving geometry. Instead, it conservatively expands dynamic-object AABBs by one probe cell plus bias and wakes sleeping probes inside those bounds; newly awake probes may use extra rays with zero hysteresis before returning to normal updates [S2, §§5–6, pp. 12–18].

**Source fact.** The 2021 paper distinguishes a large object/visibility change from an ordinary update by temporarily reducing both irradiance and visibility hysteresis [S2, §4.3, pp. 10–11]. This is an explicit stale-light response, although its published heuristic affects all probes rather than a computed dependency set.

**Inference.** A transient movable object should usually not force probe relocation every frame. Let current ray hits, visibility moments, probe wake-up, and reduced history absorb it. A persistent topology edit—adding or deleting terrain—is different: it changes which probe positions are inside solid space and can justify reclassification and relocation for the invalidated set.

### Classification, relocation, and scheduling

**Source fact.** RTXGI can run relocation and classification at runtime. Classification uses fixed, non-rotating rays to mark probes inside geometry or without nearby useful geometry inactive. Inactive probes skip most tracing and atlas update work; fixed rays remain available for stable classification [S5].

**Source fact.** RTXGI documents per-frame, lower-frequency, or asynchronous volume updates. Its variability metric can pause converged volume updates and tells the application to resume them after events such as an object moving or an explosion [S5]. These are scheduling hooks, not a built-in edited-AABB dependency solver.

**Source fact.** Infinite scrolling invalidates only newly exposed edge planes and preserves interior probe history [S5]. This is a concrete partial invalidation mechanism for volume movement, but it does not identify probes affected by arbitrary geometry edits.

**Inference.** The reviewed sources do not provide a generic mapping from `edited voxel AABB -> every probe ray affected by the edit`. Safe local invalidation therefore needs project-owned dependency tracking or a conservative influence expansion. Until that exists, a full-domain invalidation/rebuild is conservative and understandable.

## What “production practice” means here

### Within DDGI: B is the normal answer

This conclusion is strongly source-supported:

- both DDGI papers describe a temporally updated irradiance field;
- the 2019 paper explicitly reads previous-frame probes for recursive indirect light;
- the 2021 paper focuses on making temporal convergence production-ready rather than replacing recursion with a deterministic single-bounce rebuild;
- RTXGI's official integration steps and sample implement recursive irradiance at probe hits;
- RTXGI advertises temporally accumulated, multi-bounce dynamic GI and acknowledges minimum latency after lighting changes [S1–S7].

The narrower claim is important: **B is common in the documented DDGI lineage.** The sources do not establish a market-wide count of proprietary engines, so “most games use B” would be stronger than the available evidence.

### Across other dynamic-GI systems: temporal caches and budgets are common, but the algorithms differ

Unreal Lumen is useful corroborating evidence for the broad engineering pattern, not for DDGI internals:

- Lumen is a different GI architecture built around screen traces, a ray-traceable Lumen Scene, Surface Cache lighting, radiosity/final gather, and world/screen-space radiance caches [S8–S10].
- Epic documents that only a fraction of Surface Cache lighting and a configurable number of radiance-cache probes are updated per frame. Increasing those budgets improves responsiveness; low budgets produce catch-up and popping [S10].
- Lumen exposes lighting update-speed controls because cached GI changes can propagate slowly [S9]. Software tracing updates cached distance-field representations; hardware tracing must update acceleration structures, and dynamically deforming triangle geometry has an explicit per-frame setup cost [S9–S10].

Thus, Lumen supports the general inference that production dynamic GI accepts temporal convergence and prioritizes subsets instead of rebuilding a globally exact solution after every mutation. It does **not** show that Lumen uses DDGI's eight-probe query, visibility moments, or B's exact surfel-recursion loop.

Unity Adaptive Probe Volumes are an even more important non-equivalence:

- APV automatically places probes and lets static or dynamic objects sample baked lighting [S11].
- Runtime sky color can change, but Unity describes sky occlusion and bounced sky response stored at probes as static baked data; changing the occlusion setup requires rebaking [S12]. Lighting Scenarios are also pre-baked alternatives rather than runtime geometry retracing [S11–S12].

APV therefore is not evidence for either A or B as a destructible-scene DDGI policy. By itself it does not update indirect occlusion after arbitrary voxel addition/deletion.

## Recommended hybrid for destructible voxels

The following is a project recommendation synthesized from the mechanisms above.

### 1. Preserve exact geometry ownership

Every probe build/update should carry a geometry revision. Rays, hit shading, classification, relocation, atlas filtering, and final publication must all refer to that revision. A superseded build may finish GPU work but must not publish.

### 2. Invalidate conservatively

- Near term: invalidate the full volume after a terrain revision, which is correct while the affected-probe set is unknown.
- Later: track probe-ray dependencies on voxel chunks, or conservatively combine changed chunks, nearby probe cages, previous hit locations, and a propagation halo.
- Continue serving unaffected probes only after the local set is provably conservative. Otherwise stale visibility can reintroduce leaks.

### 3. Bootstrap invalid probes without stale indirect history

For invalid probes, clear or bypass irradiance and visibility history. Use deterministic direct sky and current-revision occlusion as the seed; evaluate current-revision direct lights at the retraced surface hit and optionally publish that strict single-bounce result. This corresponds to A, localized to the invalidated set.

This is stricter than the paper's temporary hysteresis reduction. It is justified when a deleted roof or newly sealed wall would make old visibility qualitatively wrong, not merely noisy.

### 4. Resume temporal recursive feedback

After the current-revision bootstrap is visible, use B:

- sample only a published field with compatible geometry ownership;
- use zero or low hysteresis for newly bootstrapped probes;
- ramp toward the normal hysteresis after large deltas settle;
- prioritize recently edited, visible, nearby, newly awake, or high-variability probes;
- retain normal history for probes outside the conservative invalidation set.

This retains immediate correctness at the edit seam while recovering multi-bounce light over subsequent updates.

### 5. Re-run geometry-sensitive metadata

Reclassify invalidated probes after voxel topology changes. Re-run relocation when an edit can place a probe inside solid terrain or expose one previously trapped in terrain. Avoid using ordinary movable objects to continuously tug probe positions; that agrees with the stability rationale in the 2021 paper.

### 6. Expose the transition explicitly

Useful runtime/debug states are:

```text
valid history
  -> geometry revision changed
  -> invalid / fail-closed
  -> current-revision direct seed
  -> current-revision single bounce published
  -> temporal multi-bounce converging
  -> converged / low variability
```

Acceptance captures should name the geometry revision, bounce mode, update count, and active/staging token. That prevents a visually plausible result from silently using stale history.

## Decision for Re: Flora

- Keep **A as the next correctness milestone and future edit bootstrap**. It makes terrain-hit shading, albedo transfer, backface behavior, visibility ownership, and atomic publication independently testable.
- Do not treat A as the final DDGI architecture. Once strict single-bounce is stable, add **B as the normal steady state** for higher-order diffuse transport.
- Keep the current full-volume, fail-closed edit response until a conservative local dependency set exists. Performance is currently acceptable and correctness is observable.
- When local invalidation is implemented, apply A only to the invalid set while unaffected probes continue serving valid history; then let B converge the edited region.

## Sources

All sources below are primary papers, official documentation, or official source code. Accessed 2026-08-01.

| ID | Source | Direct URL | Used for |
| --- | --- | --- | --- |
| S1 | Majercik et al., *Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields* (JCGT, 2019) | <https://jcgt.org/published/0008/02/01/paper-lowres.pdf> | Previous-frame surfel shading, dynamic updates, all-probe baseline, hysteresis, multi-bounce latency |
| S2 | Majercik et al., *Scaling Probe-Based Real-Time Dynamic Global Illumination for Production* (JCGT, 2021) | <https://jcgt.org/published/0010/02/01/paper-lowres.pdf> | Fast convergence, large-object hysteresis response, static relocation, dynamic-object wake-up, probe states |
| S3 | NVIDIA, *RTXGI Algorithms* | <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Algorithms.md> | Official DDGI scope, benefits, temporal-latency limitation |
| S4 | NVIDIA, *RTXGI Integration Guide* | <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Integration.md#tracing-probe-rays-for-a-ddgivolume> | Application-owned acceleration structure and hit shading; recursive irradiance step |
| S5 | NVIDIA, *RTXGI DDGIVolume Reference* | <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/DDGIVolume.md> | Update scheduling, scrolling invalidation, relocation, classification, variability/event wake-up |
| S6 | NVIDIA RTXGI sample, `ProbeTraceRGS.hlsl` | <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/samples/test-harness/shaders/ddgi/ProbeTraceRGS.hlsl#L131-L200> | Sky miss path; direct hit lighting plus recursive probe irradiance in official code |
| S7 | NVIDIA RTXGI SDK, `ProbeBlendingCS.hlsl` | <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/ProbeBlendingCS.hlsl#L505-L550> | History weighting, cleared-history zero hysteresis, change response |
| S8 | Epic Games, *Lumen Global Illumination and Reflections* | <https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-global-illumination-and-reflections-in-unreal-engine> | Lumen's fully dynamic, multi-bounce scope |
| S9 | Epic Games, *Lumen Technical Details* | <https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-technical-details-in-unreal-engine> | Lumen Scene representations, cache latency, hardware-RT dynamic-geometry cost |
| S10 | Epic Games, *Lumen Performance Guide* | <https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-performance-guide-for-unreal-engine> | Partial Surface Cache/probe update budgets and scene-representation update cost |
| S11 | Unity, *Adaptive Probe Volumes* | <https://docs.unity.cn/Packages/com.unity.render-pipelines.high-definition@17.6/manual/probevolumes.html> | APV scope, baked lighting scenarios, runtime consumption |
| S12 | Unity, *Update light from the sky at runtime with sky occlusion* | <https://docs.unity.cn/2023.3/Documentation/Manual/urp/probevolumes-skyocclusion.html> | Runtime sky color versus static baked sky occlusion and bounced-light data |

## Evidence limits

- The papers report integrations into RTXGI, Unity, Unreal Engine 4, and proprietary commercial engines, but they do not disclose every shipped engine's exact invalidation policy. The conclusion about production DDGI therefore concerns the documented DDGI lineage, not a complete industry census.
- RTXGI delegates acceleration-structure maintenance and hit shading to the application. It does not prescribe voxel-edit revision tokens or dependency tracking.
- Lumen and APV are included only to compare dynamic-scene engineering patterns. Neither is an implementation of DDGI, and their caches/probes must not be mapped one-to-one onto DDGI concepts.
