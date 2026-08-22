# Localized geometry edits without global DDGI disturbance

Research date: 2026-08-23

## Question

When a small part of dynamic or destructible geometry changes, which mechanisms in the published DDGI lineage and official production implementations can avoid unnecessarily disturbing distant probes? In particular, do any public sources implement an arbitrary

```text
edited geometry AABB -> exact affected probe set
```

mapping, preserve unaffected atlas history, or invalidate history locally?

## Short answer

The public DDGI lineage contains **mature building blocks**, but not a complete edit-dependency solver:

- DDGI's normal steady state retains temporal history and blends new ray results into it. The 2021 production paper adds per-texel change detection, event-driven hysteresis reduction, probe states, and a conservative dynamic-object AABB wake-up rule [S1, §4.2; S2, §§4.3 and 6].
- RTXGI's official code preserves most of an atlas in two concrete situations: infinite scrolling clears only newly exposed probe planes, and NVIDIA's UE4 plugin updates only a cyclic subset of probe indices when a ray budget is configured [S3; S6].
- RTXGI classification can skip irradiance and distance work for inactive probes, and variability can stop updates after a volume settles [S3–S5]. These are usefulness and convergence mechanisms, not geometry-edit dependency tracking.
- The 2021 paper explicitly says that reducing hysteresis only for probes affected by an object or lighting change would probably be more effective than its all-probe heuristic, but identifies that refinement as future work [S2, §4.3, PDF pp. 10–11].

No reviewed DDGI paper, RTXGI SDK path, or official RTXGI UE4 plugin path provides an exact arbitrary-edit dependency graph, records which geometry blocks each probe ray traversed, propagates dirty state through recursive probe-to-probe transport, or exposes a general per-probe geometry-revision invalidation API.

Therefore, **preserving unaffected atlas/history is source-backed; discovering the exact unaffected set for arbitrary destructible geometry is application-owned research or engineering**. The earlier idea of recording ray-to-voxel dependencies is a plausible design inference, not a published RTXGI feature.

## Source-supported mechanisms

| Mechanism | Published or official behavior | What it solves | What it does not solve |
| --- | --- | --- | --- |
| Temporal history | Blend each probe texel's new estimate with its previous value using hysteresis [S1, §4; S4]. | Prevents every ordinary update from becoming a cold start. | Does not identify which histories became invalid after topology changes. |
| Per-texel change response | Lower irradiance hysteresis when the new estimate differs substantially; cleared black texels use zero history [S2, §4.3; S4]. | Makes genuinely changed texels respond faster while stable texels retain history. | Detection occurs after tracing; the RTXGI shader still dispatches the volume and has one volume-level base hysteresis. |
| Scene-event hysteresis response | Temporarily reduce irradiance and, for large object changes, visibility hysteresis [S2, §4.3]. | Speeds convergence after known large changes. | The evaluated implementation reduces it for all probes; affected-only selection is explicitly future work. |
| Probe states | Off/Sleeping/Newly Awake/Newly Vigilant/Awake/Vigilant states prune work and give newly useful probes rapid initialization [S2, §6]. | Avoids tracing probes that cannot currently contribute. | States express usefulness, not whether a particular edit invalidated a previously useful probe's lighting. |
| Expanded dynamic-object AABB | Expand each dynamic object's AABB by one probe cell plus self-shadow bias and wake sleeping probes inside it [S2, §6.3]. | Conservatively activates probes newly needed to shade a moving object. | It is not a proof that only those probes' irradiance or visibility depend on the object. It does not invalidate active-probe history. |
| Classification | Fixed rays classify probes as active or inactive; inactive probes skip normal ray tracing and atlas blending [S3, S5]. | Removes probes inside geometry or without nearby useful geometry. | It is not an edit mask and still traces fixed classification rays for inactive probes. |
| Infinite scrolling | Clear only the edge planes that leapfrog into newly exposed space; interior probes retain irradiance and distance [S3, S4]. | Provides a production example of exact partial history preservation when changed ownership is known from circular-grid indexing. | The cleared set comes from volume movement, not arbitrary geometry influence. |
| UE4 probe ray budget | Select one volume by weighted round robin, then update a cyclic contiguous probe-index range determined by `ProbeUpdateRayBudget`; other atlas tiles remain untouched [S6]. | Provides an official implementation of partial atlas update and bounded per-frame work. | Selection is index round robin, not edit-local or importance driven. |
| Probe variability | Measure convergence and let the application pause or resume updates for a volume [S3]. | Stops settled random sampling from producing continuous low-frequency temporal noise. | The public reduction is volume-wide and event restart policy is application-owned. |

## What the DDGI papers actually implement

### 2019: deliberately conservative all-probe updates

The original paper tested updating only a subset of probes, including camera-radius selection, and varying ray counts with camera distance. It reports expected performance gains but rejects those variants for the reference results because of scene-dependent controls and bookkeeping. The published reference configuration updates every probe with the same ray count every frame [S1, §4.2, PDF p. 9]. Later, §6 states again that optimized probe selection and adaptive updates are future work [S1, PDF p. 16].

This is evidence that subset scheduling was known, not evidence for a mature local-edit invalidation algorithm.

### 2021: production convergence controls, but affected-only remains future work

The production paper contributes three directly relevant mechanisms [S2]:

1. **Per-texel hysteresis adjustment.** A moderate irradiance change reduces hysteresis and a very large change uses zero hysteresis for that update (§4.3).
2. **Scene event heuristics.** A ceiling collapse is the paper's example of a large object change; irradiance and visibility hysteresis are temporarily reduced (§4.3).
3. **Per-probe states.** Newly awake or vigilant probes can trace extra rays with zero hysteresis, while sleeping/off probes avoid ordinary updates (§6).

The boundary is unusually explicit: the paper says its scene-event heuristics reduce hysteresis for **all** probes, not only local probes, and says affected-probe-only reduction would probably be more effective but was not explored in depth [S2, §4.3, PDF pp. 10–11].

The expanded-AABB rule in §6.3 is sometimes easy to overread. Its purpose is to wake **sleeping** probes near dynamic objects so those objects can be shaded. It does not compute all active probes whose rays, visibility moments, or recursive irradiance transitively depend on the object.

The same paper's camera-tracking window is a genuine partial-preservation design: one grid plane leapfrogs to the front, while the other probe locations and histories remain stable [S2, §7.3, PDF pp. 20–21]. This works because the newly owned plane is known exactly from the circular-buffer movement; no light-transport dependency analysis is required.

## What official RTXGI source code implements

The code review below is pinned to NVIDIA RTXGI-DDGI commit `f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6` (2024-02-05), so links do not drift.

### Core SDK dispatch and atlas blending

`DDGIVolumeBase::GetRayDispatchDimensions()` returns the dimensions of the whole ray-data texture [S5]. The sample ray-generation shader derives a probe index directly from the dispatch coordinates. It early-outs normal rays only when classification marks the probe inactive; fixed rays remain for classification/relocation [S5]. There is no edit mask or arbitrary probe-index list in this path.

The public SDK classification state is also narrower than the paper's six-state production model: `Common.hlsl` defines only active and inactive states [S5]. Vulkan relocation and classification dispatch over `GetNumProbes()`, and the ordinary `ClearProbes()` implementation clears the complete irradiance and distance atlases [S5]. The public sample's variability policy reads a volume average and pauses or resumes the whole volume [S5]. Thus, the core SDK does not quietly contain a finer dirty-probe system behind the higher-level documentation.

`ProbeBlendingCS.hlsl` has three relevant history cases [S4]:

- scrolled planes are cleared to black and skip blending;
- inactive probes skip blending and retain their existing atlas values;
- active probes read the previous atlas texel and blend with the new result.

For irradiance, a cleared black texel forces zero hysteresis. A large measured lighting change lowers the texel's hysteresis. Distance history uses the volume hysteresis without the irradiance change heuristic. No geometry revision or edited AABB participates in these decisions.

### Official UE4 plugin: real partial atlas preservation

NVIDIA's UE4 4.27 plugin adds a practical scheduling layer that the core SDK does not provide [S6]:

- only one eligible volume is selected per frame, weighted by `UpdatePriority`;
- `ProbeUpdateRayBudget / raysPerProbe` determines a probe count;
- `ProbeIndexStart` advances cyclically through the volume;
- trace and blend passes receive only `ProbeIndexStart` and `ProbeIndexCount`.

Because the atlas resources are persistent and only that range is blended, unselected probe tiles retain their previous irradiance and distance. This is the closest public RTXGI implementation precedent for **partial atlas preservation**.

It still does not prioritize probes near changed geometry. Its selection policy is volume priority plus cyclic index order. Converting the contiguous range into an application-supplied work list or mask would be a project extension.

### SDK ownership boundary

The integration guide leaves acceleration structures, material hit shading, and probe-ray dispatch to the application [S7]. The volume reference likewise says lower-frequency or asynchronous scheduling is possible but not directly implemented by the SDK [S3]. This makes application-owned local scheduling possible; it does not make such scheduling an existing RTXGI algorithm.

## Similar production mechanisms that are not DDGI

### Unreal Engine Lumen

Lumen is useful as a mature production-cache comparison, but its data structures and update rules are not interchangeable with DDGI [S8–S10]:

- Lumen's Surface Cache parameterizes mesh surfaces as Cards, maintains it around the camera, and amortizes lighting updates across frames.
- Its official performance guide exposes the fraction of Surface Cache lighting updated per frame and budgets the number of World Space Radiance Cache probes traced per frame.
- Unreal exposes `Invalidate Lumen Surface Cache` on a target `Primitive Component`, providing an official component-scoped refresh hook for material changes.
- Epic warns that budgets that are too small can produce catch-up popping, while increasing update speed raises GPU cost.

These sources support the broad production pattern of persistent caches, scoped invalidation where ownership is known, prioritized/budgeted refresh, and temporal catch-up. They do **not** establish that Lumen computes a DDGI-style `edited AABB -> affected probes` set, preserves octahedral DDGI tiles, or tracks recursive dependencies between irradiance probes.

## Public implementation audit: requested capabilities

| Requested capability | Public result | Evidence boundary |
| --- | --- | --- |
| Arbitrary edited AABB to exact affected probes | **Not found.** | The 2021 paper calls affected-only hysteresis a promising refinement; RTXGI's public shaders and host API contain scroll-plane and classification state, but no arbitrary edit mask [S2–S5]. |
| Partial atlas preservation | **Yes, for known scheduling/topology cases.** | RTXGI scrolling retains interior probe data; the official UE4 plugin updates only `ProbeIndexStart..Count` [S3, S4, S6]. |
| Local history invalidation | **Only for scrolled planes in public RTXGI.** | Cleared scroll-plane texels use zero history. Per-texel change detection lowers history after a changed sample arrives, but is not preselected from geometry edits [S4]. |
| Local probe wake-up from object bounds | **Yes, in the 2021 paper's state algorithm.** | Expanded dynamic-object AABBs wake sleeping probes; this is not dependency-complete invalidation [S2, §6.3]. |
| Ray-to-geometry-block dependency recording | **Not found.** | No reviewed paper, core SDK shader/host path, or official UE4 plugin path stores traversed geometry IDs per probe. |
| Recursive dirty propagation between probes | **Not found.** | Recursive irradiance is sampled during hit shading, but the public implementation does not record producer/consumer probe edges [S7]. |
| Application-provided arbitrary probe work list | **Not found in the reviewed RTXGI interfaces.** | The core SDK dispatches atlas dimensions; the UE4 plugin accepts a cyclic contiguous start/count, not a sparse list [S5, S6]. |

“Not found” is limited to the public primary-source set below. Proprietary engine integrations may contain additional mechanisms that are not documented publicly.

## Mature-first direction for Re: Flora

This is a design inference from the source mechanisms, not an implementation recipe published by NVIDIA.

The lowest-experiment path is **not** to begin with exact ray-dependency tracking. It is to compose already-demonstrated mechanisms:

1. **Retain the existing atlas/history across a terrain edit.** This follows DDGI's normal temporal operation and RTXGI's partial-update precedent. Avoid making an unrelated distant probe a black/zero-history cold start solely because a global geometry revision changed.
2. **Use the edited AABB only as a conservative priority/wake region.** Expand it by at least one probe cell plus the query bias, matching the 2021 state-machine precedent. Locally re-run geometry-sensitive classification/relocation and temporarily use low or zero history for probes selected by that rule.
3. **Continue a background full-volume round robin with normal hysteresis.** This follows the official UE4 plugin's bounded subset updates and eventually discovers nonlocal effects that a proximity AABB cannot prove absent.
4. **Preserve atlas texels outside the current update set.** This is directly demonstrated by RTXGI scrolling and the UE4 subset scheduler.
5. **Use per-texel change detection once new samples arrive.** Large genuine changes converge quickly; distant samples that remain similar retain normal history.
6. **Measure disturbance and convergence separately.** A local edit test should bound distant-probe delta at the first publication and still verify that nonlocal physically real changes eventually propagate.

This policy is conservative about correctness without subjecting all probes to the same cold-start noise. It does not claim that the edit AABB is an exact influence bound: newly added occluders can intersect a long ray from a distant probe, and recursive irradiance can carry a local change farther than one cell. The background sweep is what closes that gap.

Only if this mature composition still leaves unacceptable stale-light latency should the project consider a sparse work list, ray-to-voxel dependency recording, or probe-to-probe dirty propagation. Those are deeper custom systems with memory, invalidation, false-negative, and recursive-transport costs that the public RTXGI implementation does not pay.

## Unsupported claims and remaining uncertainty

- It would be unsupported to call exact ray-to-voxel dependency tracking “the RTXGI solution.” It is an application design proposal.
- It would be unsupported to treat a one-cell-expanded edit AABB as a complete light-transport influence bound. The paper uses that expansion to wake sleeping probes near dynamic objects.
- It would be unsupported to infer that RTXGI variability selects individual dirty probes. The documented output is reduced to a volume average and the application decides when to pause or resume the volume.
- It would be unsupported to transfer Lumen's cache invalidation directly to DDGI. Lumen caches mesh-surface Cards and multiple radiance/final-gather structures, not DDGI's fixed eight-probe field.
- The sources do not disclose the local-invalidation policies of proprietary engines mentioned by the 2021 paper. Absence from the public SDK is not proof that no shipped engine has such a system.

## Sources

All sources are primary papers, official documentation, or official source code. Accessed 2026-08-23.

| ID | Source | Direct URL | Used for |
| --- | --- | --- | --- |
| S1 | Majercik et al., *Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields* (JCGT, 2019) | [Publisher PDF](majercik-2019-ddgi.pdf) and <https://jcgt.org/published/0008/02/01/> | All-probe baseline, tested subset schedules, temporal hysteresis, adaptive-update future work |
| S2 | Majercik et al., *Scaling Probe-Based Real-Time Dynamic Global Illumination for Production* (JCGT, 2021) | [Publisher PDF](majercik-2021-scaling-ddgi.pdf) and <https://jcgt.org/published/0010/02/01/> | Per-texel/event hysteresis, affected-only future work, probe states, expanded dynamic AABBs, newly awake initialization, tracking-window preservation |
| S3 | NVIDIA, *RTXGI DDGIVolume Reference* | <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/DDGIVolume.md> | Scheduling ownership, scroll-plane invalidation, classification, fixed rays, variability |
| S4 | NVIDIA RTXGI SDK, `ProbeBlendingCS.hlsl` | <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/ProbeBlendingCS.hlsl#L350-L570> | Scroll clearing, inactive-probe early-out, previous-atlas blending, zero history for cleared irradiance, per-texel change response |
| S5 | NVIDIA RTXGI SDK and sample source | <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/src/ddgi/DDGIVolume.cpp#L206-L209>, <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/samples/test-harness/shaders/ddgi/ProbeTraceRGS.hlsl#L35-L61>, <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/include/Common.hlsl#L38-L43>, <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/src/ddgi/gfx/DDGIVolume_VK.cpp#L468-L642>, <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/src/ddgi/gfx/DDGIVolume_VK.cpp#L1185-L1203>, <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/samples/test-harness/src/graphics/DDGI_VK.cpp#L1619-L1640> | Whole ray-atlas dispatch, two public classification states, all-probe classification/relocation, full clear, and volume-average variability policy |
| S6 | NVIDIA RTXGI UE4 4.27 plugin, README, settings, and update source | <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/ue4-plugin/4.27/RTXGI/README.md#L97-L105>, <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/ue4-plugin/4.27/RTXGI/Source/RTXGI/Private/RTXGIPluginSettings.h#L75-L85>, <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/ue4-plugin/4.27/RTXGI/Source/RTXGI/Private/DDGIVolumeUpdate.cpp#L565-L603>, <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/ue4-plugin/4.27/RTXGI/Source/RTXGI/Private/DDGIVolumeUpdate.cpp#L803-L823> | Weighted volume scheduling, ray budget, cyclic probe subset, persistent partial atlas update |
| S7 | NVIDIA, *RTXGI Integration Guide* | <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Integration.md> | Application-owned acceleration structures, ray dispatch, hit shading, and recursive irradiance |
| S8 | Epic Games, *Lumen Technical Details* | <https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-technical-details-in-unreal-engine> | Surface Cache/Card representation, amortized updates, scene synchronization, throttling |
| S9 | Epic Games, *Lumen Performance Guide* | <https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-performance-guide-for-unreal-engine> | Fractional Surface Cache updates and radiance-cache probe budgets |
| S10 | Epic Games, *Invalidate Lumen Surface Cache* | <https://dev.epicgames.com/documentation/en-us/unreal-engine/BlueprintAPI/Rendering/Lighting/InvalidateLumenSurfaceCache> | Primitive-component-scoped Surface Cache refresh hook |
