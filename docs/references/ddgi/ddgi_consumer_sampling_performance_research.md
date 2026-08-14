# DDGI consumer sampling performance research

> Date: 2026-08-14
>
> Scope: production performance of sampling the eight probes surrounding one DDGI shading
> point, with specific application to Re: Flora terrain, raster flora, and leaves.
>
> Evidence labels: **Fact** is directly supported by repository source or a primary external
> source; **Inference** maps that evidence onto Re: Flora; **Recommendation** still requires a
> matched release-mode A/B benchmark.

## Executive answer

**Eight-probe trilinear sampling is normally an acceptable production operation. It is not,
by itself, eight ray traces and it does not mean “the eight newest probes.”** It means the eight
corners of the grid cell containing the shading point. The official RTXGI query loops over those
eight probes, skips inactive probes, samples one filtered distance value and one filtered
irradiance value from each survivor, evaluates compact weighting math, then normalizes the sum.
[RTXGI `Irradiance.hlsl`, pinned source](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/Irradiance.hlsl#L89-L203).

There is strong production evidence for this cost level:

- **Fact:** the original DDGI paper measured `Sample irradiance probes for primary ray shading`
  at **0.5 ms** at 1920×1080 on an RTX 2080 Ti. That test used a `32 × 8 × 32` grid, 64 update
  rays per probe, 8×8 RGB10A2 irradiance, and RG16F depth. It is old, vendor-specific evidence,
  not a transferable budget for this renderer, but it proves that a full-screen eight-probe
  gather can be practical. [Majercik et al. 2019, Table 2, PDF p. 19](https://jcgt.org/published/0008/02/01/paper-lowres.pdf#page=19).
- **Fact:** the production follow-up recommends evaluating the gather inline in the main
  shading pass instead of writing and reading an extra full-frame indirect-light target, because
  inline evaluation reduces bandwidth. Its techniques were developed while integrating DDGI
  into RTXGI, Unity, Unreal Engine 4, and commercial games.
  [Majercik et al. 2021, §7.5 and conclusion, PDF pp. 24–25](https://jcgt.org/published/0010/02/01/paper-lowres.pdf#page=24).
- **Fact:** id Software reports that idTech 7 lit dynamic geometry and transparencies with a
  **per-vertex linear blend of the eight closest irradiance-volume probes**. This was shipped
  engine practice, although those probes were baked two-band SH and did not perform DDGI moment
  or exact voxel visibility at the receiver.
  [id Software, *Fast as Hell: idTech 8 Global Illumination*, slide 7](https://advances.realtimerendering.com/s2025/content/SOUSA_SIGGRAPH_2025_Final.pdf#page=8).

**The current Re: Flora operation is materially heavier than classical DDGI.** For every
surviving corner it can perform:

1. a metadata SSBO load;
2. relocation-aware position, surface-side, and support tests;
3. one bilinear visibility-moment atlas sample plus Chebyshev math;
4. one **variable-length exact voxel segment traversal** from the receiver to the probe; and
5. one bilinear irradiance-atlas sample.

Steps 3 and 5 are the normal DDGI texture work. Step 4 is a project-specific correctness layer,
not part of NVIDIA's standard receiver query. It walks packed voxel occupancy with an 8³ empty
block accelerator and a grid-dependent step bound capped at 2048. Eight receiver probes can
therefore mean up to eight divergent voxel traversals, which is qualitatively more expensive than
“eight texture lookups.” See
[`ddgi_query.slang`](../../../shader/slang/ddgi_query.slang#L444-L469),
[`ddgi_voxel_visibility.slang`](../../../shader/slang/ddgi_voxel_visibility.slang#L101-L225), and
[`voxel_visibility.rs`](../../../src/ddgi/voxel_visibility.rs#L90-L112).

**The highest-probability optimization target for flora is therefore not reducing eight to four
probes. It is avoiding repeated receiver work:** keep and invalidate a persistent flora/leaf
lighting cache, reuse the eight visibility results while only refreshing irradiance when the DDGI
lighting revision changes, and evaluate exact voxel visibility at a lower spatial or temporal
frequency than the cheap irradiance gather. This preserves the world-space GI contract for flora
and leaves without paying eight segment marches for every visible cache entry every frame.

The current branch still needs a matched release-mode A/B before anyone can call its cost
acceptable. External evidence establishes that the representation is production-proven; it does
not establish the cost of Re: Flora's exact visibility or current cache population count.

## What the current renderer actually executes

### The regular query

**Fact:** `sampleDdgiDiffuseEnvironmentFromAtlas` unrolls a 2×2×2 cage and therefore examines at
most eight spatial corners. It does not select probes by update time or “freshness.” Probe
publication already chooses one coherent active field; all eight samples come from that published
revision. See [`ddgi_query.slang`](../../../shader/slang/ddgi_query.slang#L877-L915).

For each valid and supported corner, the full consumer mode executes the moment and exact
visibility branches before reading irradiance. The useful cost model is therefore:

```text
cost(query) ~= 8 * (
    metadata + spatial weights
    + moment-atlas sample + Chebyshev
    + exact voxel segment traversal
    + irradiance-atlas sample
)
```

Early rejection can reduce this, but it also introduces lane divergence when neighboring cache
entries reject different probes or exact traversals hit at different lengths.

### Terrain already has a receiver cache

**Fact:** terrain stores canonical irradiance and eight 16-bit packed visibility values. When a
matching terrain cache entry exists, it reuses those visibility results and only reloads probe
metadata and irradiance to rebuild the smoothly weighted result. See
[`ddgi_query.slang`](../../../shader/slang/ddgi_query.slang#L1002-L1023) and
[`ddgi_query.slang`](../../../shader/slang/ddgi_query.slang#L1145-L1207).

**Inference:** this is already the right decomposition: geometric visibility has a different
invalidation lifetime from lighting radiance. It is a stronger template for flora optimization
than changing the number of probe corners.

### Flora and leaves cache the result, but rebuild the cache every frame

**Fact:** the flora compute shader dispatches one thread for every visible
`instance × mesh voxel` cache entry and calls `sampleFloraEnvironment` for each surviving entry.
The tree-leaf compute shader makes one query per visible leaf instance. See
[`flora_lighting_cache.comp.slang`](../../../shader/slang/flora_lighting_cache.comp.slang#L27-L72)
and
[`tree_leaf_lighting_cache.comp.slang`](../../../shader/slang/tree_leaf_lighting_cache.comp.slang#L17-L48).

**Fact:** the vertex shaders do not gather eight DDGI probes. They read one already-computed
`float4` from `flora_lighting_cache`. See
[`flora.vert.slang`](../../../shader/slang/flora.vert.slang#L82-L92) and
[`leaves.vert.slang`](../../../shader/slang/leaves.vert.slang#L82-L93).

**Fact:** the host allocates buffers per GPU frame slot and records both flora and leaf cache
dispatches for the visible frame plan whenever raster-flora DDGI lighting is enabled. The buffer
capacity is persistent, but its lighting contents are recomputed rather than revisioned and reused
across frames. See
[`flora_lighting_cache.rs`](../../../src/tracer/flora_lighting_cache.rs#L6-L62) and
[`tracer/mod.rs`](../../../src/tracer/mod.rs#L4175-L4332).

**Inference:** if profiling shows `graphics.flora_lighting_cache` or
`graphics.leaf_lighting_cache` is large, the issue is likely the number of cache entries multiplied
by the full receiver query, not the eventual flora/leaf draw's one-buffer-load lighting path.

## Where the cost normally comes from

| Component | Classical RTXGI query | Current Re: Flora full query | Likely behavior |
| --- | --- | --- | --- |
| Probe selection and trilinear weights | Eight fixed corners, compact ALU | Eight fixed corners plus relocation/support/surface-side tests | Normally secondary unless register pressure or divergence becomes severe |
| Irradiance | Up to eight bilinear atlas samples | Up to eight bilinear `RGBA32F` atlas samples | Bandwidth/cache sensitive; neighboring shading points often share probes and directions, improving locality |
| Visibility moments | Up to eight bilinear distance samples plus Chebyshev | Same structure, currently `RG32F` | Additional bandwidth plus moderate ALU; usually predictable and bounded |
| Exact visibility | None | Up to eight variable-length 3D voxel traversals | High-risk cost: dependent memory access and divergent loop length |
| Repetition frequency | Typically per shaded pixel/vertex or chosen cache sample | terrain cache reuse, but all visible flora cache entries and leaf instances repopulated each frame | Multiplier can dominate even when one query is individually reasonable |
| Output/writeback | Inline gather can avoid an intermediate full-screen pass | Flora compute writes one `float4` per cache entry, then vertex shader rereads it | Beneficial if many vertices reuse each entry; wasteful if entry is only consumed once or unchanged across frames |

The official RTXGI source confirms the bounded standard shape: probe-state load, distance sample,
Chebyshev visibility, irradiance sample, and normalization.
[RTXGI `Irradiance.hlsl`](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/Irradiance.hlsl#L89-L203).

**Inference:** for standard RTXGI, texture traffic and texture latency are credible primary
concerns, especially with large formats, while the scalar weighting math is compact. For Re:
Flora, it is unsafe to call the shader merely bandwidth-bound until the exact traversal is removed
in an A/B: the traversal adds a potentially much larger control-flow and random-access term.

## What production systems do

### 1. Keep eight probes, optimize their representation and use site

The original DDGI production configuration deliberately chooses 8×8 irradiance and 16×16
visibility tiles because powers-of-two tile sizes map cleanly to 32/64-lane hardware, reduce
bandwidth and memory, and make convolution/indexing coherent. Its optimized compute update is
reported as 3× faster than the earlier pixel-shader update. This applies primarily to probe
updates, but the compact atlas formats and gutters also directly affect consumer cache behavior.
[Majercik et al. 2021, §7.2, PDF pp. 19–20](https://jcgt.org/published/0010/02/01/paper-lowres.pdf#page=19).

The official SDK uses `Texture2DArray` resources and hardware bilinear sampling for irradiance and
distance. It does not ray trace visibility at the shading point.
[RTXGI `Irradiance.hlsl`](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/Irradiance.hlsl#L18-L21).

**Recommendation:** independently benchmark compact consumer atlas formats. Re: Flora currently
uses `RGBA32F` irradiance and `RG32F` visibility, while the 2019 measured configuration used
RGB10A2 and RG16F. `RGBA16F` irradiance plus `RG16F` moments is the conservative first experiment;
RGB9E5/RGB10A2 needs stricter energy, banding, and convergence validation. This can reduce both
resident memory and the most regular part of the query bandwidth, but will not remove exact DDA
cost.

### 2. Classification and sleeping optimize updates, not the normal eight-corner topology

RTXGI probe classification marks probes inactive when they appear embedded in geometry or have no
nearby geometry. Inactive probes are skipped by the receiver query. More importantly, inactive
probes shoot only the fixed classification rays and are skipped by probe blending.
[RTXGI `ProbeClassificationCS.hlsl`](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/ProbeClassificationCS.hlsl#L137-L214),
[`ProbeTraceRGS.hlsl`](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/samples/test-harness/shaders/ddgi/ProbeTraceRGS.hlsl#L53-L57), and
[`ProbeBlendingCS.hlsl`](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/ProbeBlendingCS.hlsl#L389-L408).

The production paper reports a 30–50% average improvement from probe sleeping in its tested
scenes, and spends the recovered time on more rays for active probes.
[Majercik et al. 2021, §7.1 and Figure 8, PDF pp. 18–19](https://jcgt.org/published/0010/02/01/paper-lowres.pdf#page=18).

**Inference:** classification is valuable for Re: Flora's builder/update budget, but it is not a
general cure for consumer sampling. A surface still needs the valid corners of its cage; overly
aggressive classification can reduce interpolation support and worsen leaks or discontinuities.

### 3. Cache or reduce evaluation frequency for repeated consumers

Lumen's production pattern is to cache expensive lighting and amortize updates. Epic documents
that Surface Cache direct and indirect lighting updates are spread across multiple frames, and its
World Space Radiance Cache exposes both probe resolution and probes-updated-per-frame as explicit
performance controls. Lumen Screen Probe Gather can integrate at half resolution, with documented
noise and normal-softening trade-offs.
[Epic, Lumen Technical Details](https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-technical-details-in-unreal-engine) and
[Lumen Performance Guide](https://dev.epicgames.com/documentation/unreal-engine/lumen-performance-guide-for-unreal-engine?lang=en-US).

idTech 8's current GHOST system follows the same reuse principle at a different scale:

- irradiance volumes use compact RGB9E5 color and RG16F visibility in 8×8 tiles;
- half- or quarter-resolution final gather performs no material shading and only queries caches;
- a shared froxel irradiance volume is used by particles and glass so transparent consumers do not
  repeat the lookup work; and
- its shipped hotspot reports 0.489–0.6 ms final gather on the listed consoles, with the whole GI
  system relying on denoise, temporal filtering, and upscale.

Sources: [idTech 8 irradiance volumes, slide 20](https://advances.realtimerendering.com/s2025/content/SOUSA_SIGGRAPH_2025_Final.pdf#page=21),
[final gather, slides 21–22](https://advances.realtimerendering.com/s2025/content/SOUSA_SIGGRAPH_2025_Final.pdf#page=22),
[transparency froxel sharing, slide 28](https://advances.realtimerendering.com/s2025/content/SOUSA_SIGGRAPH_2025_Final.pdf#page=29), and
[shipped hotspot timings, slide 31](https://advances.realtimerendering.com/s2025/content/SOUSA_SIGGRAPH_2025_Final.pdf#page=32).

**Inference:** Re: Flora should copy the reuse principle, not the whole GHOST renderer. Its
equivalent is a persistent world/revision-aware flora lighting cache or a coarse crown/chunk
irradiance cache, not a new screen-space denoiser.

### 4. Choose per-object, per-vertex, or per-pixel based on spatial scale

There is no single production sampling frequency:

- Unity's ordinary Light Probe path gives a renderer one interpolated probe; a Light Probe Proxy
  Volume instead stores a 3D grid of interpolated SH in textures and samples it during rendering
  when a large object needs spatial variation.
  [Unity Light Probe Proxy Volume manual](https://docs.unity3d.com/2018.3/Documentation/Manual/class-LightProbeProxyVolume.html).
- Unity's probe API explicitly caches the last tetrahedron index on the renderer to accelerate the
  next interpolation query, an example of preserving spatial lookup state across frames.
  [Unity `LightProbes.GetInterpolatedProbe`](https://docs.unity3d.com/2021.2/Documentation/ScriptReference/LightProbes.GetInterpolatedProbe.html).
- Unreal's Volumetric Lightmaps interpolate on the GPU per pixel for quality, while their older
  Indirect Lighting Cache interpolated once per dynamic object. Epic reports 0.02 ms GPU cost for
  Volumetric Lightmap lighting on one PS4 third-person character, but that is precomputed SH—not
  dynamic DDGI with receiver visibility.
  [Epic, Volumetric Lightmaps](https://dev.epicgames.com/documentation/en-us/unreal-engine/volumetric-lightmaps-in-unreal-engine).
- idTech 7 used the eight-probe blend per vertex for dynamic geometry and transparency.
  [idTech 8 GHOST presentation, slide 7](https://advances.realtimerendering.com/s2025/content/SOUSA_SIGGRAPH_2025_Final.pdf#page=8).

**Inference for Re: Flora:** per-pixel would be unnecessary for tiny grass blades and leaves;
per-object can visibly flatten a large crown or tall plant; the current per-flora-voxel choice is a
reasonable quality tier. The performance problem is that this tier is repopulated every frame.
For dense leaves, a small tree-local 3D cache or quantized world cache can share one receiver
query among nearby leaves and retain spatial variation.

## Optimization options for Re: Flora

The order below is based on expected return and semantic risk, not on an unmeasured millisecond
claim.

### P0: measure the four existing visibility modes

The renderer already has `full`, `moment-only`, `exact-only`, and `none` consumer visibility
modes. Use the same release scene and separately report:

- `graphics.flora_lighting_cache`;
- `graphics.leaf_lighting_cache`;
- terrain/tracer cost;
- final flora and leaf draw cost; and
- cache-entry/instance counts.

Interpretation:

| Comparison | Isolates |
| --- | --- |
| `none` versus DDGI off | metadata, weights, and eight irradiance samples |
| `moment-only` versus `none` | eight visibility-atlas samples and Chebyshev math |
| `exact-only` versus `none` | eight exact voxel segment traversals |
| `full` versus both single modes | combined latency, divergence, and resource contention |

This measurement is the required answer to “is current performance acceptable?” A total frame
budget and target GPU are also required; source inspection alone cannot decide acceptance.

### P1: make flora and leaf lighting caches persistent and revision-aware

Recommended cache identity:

```text
stable sample identity
+ world position / quantized position
+ rest or lighting normal class
+ published DDGI query revision
+ probe relocation / geometry revision
```

Do not dispatch a receiver query when all of those inputs still match. New visibility, new LOD
entries, terrain edits inside the dependency region, movement across a cache cell, or a published
DDGI revision make an entry dirty. Wind animation alone should not invalidate a cache if its
chosen lighting sample position and lighting normal remain stable.

On a DDGI lighting-only revision, retain cached geometry visibility and refresh irradiance. This
extends the split already implemented by the terrain cache. On a terrain/relocation revision,
refresh both.

Expected suitability:

- **terrain:** already partially implemented and high value;
- **grass/flora:** very high value because roots and voxel identities are stable;
- **leaves:** high value if the cache uses a stable rest normal or bounded normal classes; exact
  per-frame wind-normal fidelity is not worth full DDGI visibility refresh without visual proof;
- **moving future objects:** retain per-object dirty tracking; update when transform crosses a
  spatial/normal threshold.

Primary production precedent is Lumen's multi-frame cached lighting and idTech's shared froxel
irradiance volume, cited above. The exact cache key and invalidation contract are Re: Flora design
choices.

### P2: decouple exact visibility frequency from irradiance frequency

Evaluate these in increasing quality risk:

1. Cache all eight full visibility weights for a stable flora sample and reuse them until geometry
   or relocation changes.
2. For leaves, compute one exact-visibility set per tree-local cache cell, then let each leaf apply
   its own trilinear/surface-side weight and irradiance direction.
3. Temporally rotate through dirty leaf/flora entries while preserving the previous result; give
   new or newly visible entries priority.
4. Test `moment-only` specifically for leaves and small flora, retaining full exact visibility for
   terrain and large opaque objects.

The last option is the riskiest near thin walls, caves, and terrain overhangs. Standard RTXGI is
evidence that moment-only is a production design, not proof that it passes this project's exact
visibility acceptance scenes.

### P3: reduce redundant spatial samples for dense vegetation

The DDGI field spacing is much coarser than individual leaves. Querying each leaf independently can
oversample the same low-frequency field. Candidate representations are:

- one query per tiny grass instance;
- a small fixed grid per large plant or tree crown;
- a world-quantized vegetation receiver cache shared across instances; or
- one query per instance at distance, per-voxel near the camera.

Interpolate cached irradiance inside the plant/crown and retain direct sun, leaf shadow, and other
high-frequency lighting separately. This follows the quality ladder documented by Unity's
per-renderer probe versus proxy-volume grid and idTech's per-vertex probe path.

### P4: compact atlases and improve coherence

After visibility A/B and persistent caching:

- test `RGBA16F` irradiance and `RG16F` visibility;
- keep octahedral one-pixel gutters and hardware bilinear filtering;
- dispatch flora cache work in spatially coherent order so adjacent lanes tend to use the same
  eight probes and visibility blocks; and
- only investigate spatial/Morton remapping of probe tiles if a GPU capture shows poor texture
  cache behavior. The standard atlas layout is already production-proven; remapping without cache
  counter evidence is speculative.

### P5: selectively lower shading frequency, not the DDGI topology

For distant flora or lower tiers, update cached GI every 2–4 frames or update a fraction of entries
each frame, then interpolate old/new irradiance. This saves the whole query rather than only a
fraction of its samples. Prioritize camera-near, newly visible, moved, and newly invalidated
entries. Track response latency explicitly; a lower update frequency must not hide stale lighting
after a terrain or sun/sky revision.

Lumen and GHOST prove that lower-resolution and temporal reuse can ship, but they also document
noise, softness, denoising, and catch-up trade-offs. Re: Flora's deterministic field can use a much
simpler persistent-cache update schedule and should not add stochastic noise merely to imitate
those systems.

## Options not recommended first

- **Four-probe/tetrahedral replacement:** reduces samples but changes interpolation topology,
  relocation behavior, and acceptance fixtures. It attacks the regular bounded part while leaving
  per-probe exact DDA and per-frame repetition. The likely ROI is below caching visibility or
  entries.
- **Probe classification as the only fix:** primarily saves probe trace/update work. It cannot
  remove the need for enough trustworthy receiver probes.
- **A full screen-space GI buffer for flora:** may share work, but breaks or complicates off-screen,
  transparent, and arbitrary-object consumers. The existing world-space cache seam is the project's
  advantage.
- **Per-pixel DDGI on leaves:** unnecessary spatial frequency and alpha/overdraw multiplication.
- **Temporal jitter of the eight probe corners:** the cage is a deterministic interpolation basis,
  not a Monte Carlo sample set. Jitter belongs in probe update rays or a separately designed
  stochastic final gather, not in selecting arbitrary cage corners.

## Production decision matrix

| Approach | Saves | Terrain | Grass/flora | Leaves | Main risk |
| --- | --- | --- | --- | --- | --- |
| Persistent result cache | Whole unchanged receiver query | Existing partial pattern | Best first target | Best first target | Correct invalidation and cache residency |
| Persistent eight-visibility cache | Exact DDA and moment sampling | Existing pattern | Excellent | Excellent with spatial grouping | Relocation/geometry revisions must invalidate |
| Moment-only consumer tier | Exact DDA | Risky around terrain seams | Plausible | Strong candidate | Leaks behind terrain/thin walls |
| Per-instance GI | Entry count | Too coarse | Good for tiny plants/distant LOD | Too flat for large crowns | Spatial flattening |
| Tree/crown local grid | Entry count with spatial variation | Not applicable | Large plants only | Strong candidate | Cache construction and transition seams |
| Compact atlas formats | Texture bandwidth and memory | Good if acceptance passes | Good | Good | Precision, banding, energy drift |
| Temporal dirty-entry budget | Worst-frame work | Only if cache exists | Good | Very good | Lighting response latency |
| Four-probe topology | Roughly half bounded probe work | High correctness risk | Same risk | Same risk | Rewrites interpolation assumptions |

## Bottom line

1. **Eight neighboring probes are not inherently too expensive.** Classical DDGI and shipped
   engines use this topology, and published full-frame gather measurements are sub-millisecond on
   their stated hardware.
2. **Re: Flora is not paying only the classical cost.** Full mode adds eight potentially long,
   divergent exact voxel traversals.
3. **The flora draw is already cheap with respect to DDGI.** Its separate cache-population pass is
   where probe queries occur.
4. **The most defensible first optimization is persistent, revision-aware flora/leaf receiver
   caching, with visibility lifetime separated from radiance lifetime.** This copies a pattern the
   terrain path already uses and matches the broader production practice of amortizing cached GI.
5. **The first profiling experiment should be `full / moment-only / exact-only / none`, not 8
   probes versus 4.** It identifies whether the real constraint is exact traversal, moment/atlas
   bandwidth, or merely the number of entries rebuilt every frame.

## Primary sources

- Zander Majercik, Jean-Philippe Guertin, Derek Nowrouzezahrai, and Morgan McGuire,
  [*Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields*](https://jcgt.org/published/0008/02/01/),
  JCGT 2019.
- Zander Majercik, Adam Marrs, Josef Spjut, and Morgan McGuire,
  [*Scaling Probe-Based Real-Time Dynamic Global Illumination for Production*](https://jcgt.org/published/0010/02/01/),
  JCGT 2021.
- NVIDIA GameWorks,
  [RTXGI-DDGI official source at commit `f33e496`](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/tree/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6).
- Tiago Sousa, id Software,
  [*Fast as Hell: idTech 8 Global Illumination*](https://advances.realtimerendering.com/s2025/content/SOUSA_SIGGRAPH_2025_Final.pdf),
  SIGGRAPH Advances in Real-Time Rendering 2025.
- Epic Games,
  [Lumen Technical Details](https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-technical-details-in-unreal-engine),
  [Lumen Performance Guide](https://dev.epicgames.com/documentation/unreal-engine/lumen-performance-guide-for-unreal-engine?lang=en-US), and
  [Volumetric Lightmaps](https://dev.epicgames.com/documentation/en-us/unreal-engine/volumetric-lightmaps-in-unreal-engine).
- Unity Technologies,
  [Light Probe Proxy Volume](https://docs.unity3d.com/2018.3/Documentation/Manual/class-LightProbeProxyVolume.html) and
  [`LightProbes.GetInterpolatedProbe`](https://docs.unity3d.com/2021.2/Documentation/ScriptReference/LightProbes.GetInterpolatedProbe.html).
