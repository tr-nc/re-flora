import color;
import composition_camera_effects;
import composition_sky;
import composition_types;
import projection;
import ray;
import scene_query;
import sunlight;
import tracer_material;
import tracer_types;
import voxel_material;

[[vk::binding(0, 0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(1, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(2, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(3, 0)]] ConstantBuffer<U_StarlightInfo> starlight_info;
[[vk::binding(4, 0)]] ConstantBuffer<U_GodRayInfo> god_ray_info;
[[vk::binding(5, 0)]] ConstantBuffer<U_VoxelColors> voxel_colors;
[[vk::binding(6, 0)]] Sampler2D unified_opaque_hdr_tex;
[[vk::binding(7, 0)]] [[vk::image_format("r32f")]]
readonly RWTexture2D<float> unified_opaque_depth_tex;
[[vk::binding(8, 0)]] [[vk::image_format("r32ui")]]
readonly RWTexture2D<uint> opaque_provenance_tex;
[[vk::binding(9, 0)]] [[vk::image_format("r32f")]]
readonly RWTexture2D<float> glass_front_depth_tex;
[[vk::binding(10, 0)]] [[vk::image_format("r32ui")]]
readonly RWTexture2D<uint> glass_front_data_tex;
[[vk::binding(11, 0)]] [[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;
[[vk::binding(12, 0)]] [[vk::image_format("rgba16f")]]
readonly RWTexture2D<float4> lens_flare_output_tex;
[[vk::binding(13, 0)]] [[vk::image_format("r32f")]]
readonly RWTexture2D<float> god_ray_output_tex;
[[vk::binding(14, 0)]] Sampler2D sun_sprite_tex;
[[vk::binding(15, 0)]] [[vk::image_format("rgba16f")]]
RWTexture2D<float4> composited_tex;
[[vk::binding(16, 0)]] [[vk::image_format("rg32ui")]]
RWTexture2D<uint2> glass_debug_tex;
// Metadata is an open-addressed cell-key table. z/w hold packed diagnostics; w bit 31 is
// published last as the shade-ready marker. The separate active-slot list turns sparse hash
// entries into a dense transport workload.
[[vk::binding(17, 0)]]
RWStructuredBuffer<uint4> glass_voxel_cache_metadata;
[[vk::binding(18, 0)]]
// Two float4 values per cache slot: complete transport, then the all-transmitted
// screen-candidate contribution that can be replaced at a raster silhouette.
RWStructuredBuffer<float4> glass_voxel_cache_radiance;
[[vk::binding(19, 0)]]
RWStructuredBuffer<uint> glass_voxel_cache_active_slots;
[[vk::binding(20, 0)]]
RWStructuredBuffer<uint> glass_voxel_cache_active_count;

struct GlassVoxelCachePushConstants
{
    uint pass;
};

[[vk::push_constant]]
ConstantBuffer<GlassVoxelCachePushConstants> glass_voxel_cache_push;

static const uint OPAQUE_PROVENANCE_VOXEL = 1u;
static const uint OPAQUE_PROVENANCE_RASTER = 2u;
static const uint OPAQUE_PROVENANCE_SKY = 3u;

static const uint GLASS_FALLBACK_NONE = 0u;
static const uint GLASS_FALLBACK_SCREEN_HIT = 1u;
static const uint GLASS_FALLBACK_VOXEL = 2u;
static const uint GLASS_FALLBACK_SKY = 3u;
static const uint GLASS_FALLBACK_REJECTED_DEPTH = 4u;
static const uint GLASS_FALLBACK_REJECTED_PROVENANCE = 5u;
static const uint GLASS_FALLBACK_OFFSCREEN = 6u;
static const uint GLASS_FALLBACK_FOREGROUND = 7u;
static const uint GLASS_FALLBACK_EXHAUSTED = 8u;
static const uint GLASS_FALLBACK_QUERY_BUDGET = 9u;

// Screen-space ray intersection thickness is expressed in linear camera-space world units.
// A fixed NDC tolerance grows into an arbitrarily large world-space slab with distance and can
// therefore erase real foreground/background ordering.
static const float GLASS_SCREEN_TRACE_THICKNESS_WORLD = 2.0 / 256.0;
static const uint GLASS_DEPTH_ORDER_ULPS = 4u;
static const uint GLASS_SCREEN_TRACE_MAX_STEPS = 256u;
static const uint GLASS_VOXEL_CACHE_BUILD_PASS = 0u;
static const uint GLASS_VOXEL_CACHE_SHADE_PASS = 1u;
static const uint GLASS_VOXEL_CACHE_CLASSIFY_PASS = 2u;
static const uint GLASS_EXACT_PIXEL_BUILD_PASS = 3u;
static const uint GLASS_VOXEL_CACHE_RESOLVE_PASS = 4u;
static const uint GLASS_EXACT_PIXEL_RESOLVE_PASS = 5u;
static const uint GLASS_VOXEL_CACHE_CAPACITY = 1u << 18u;
static const uint GLASS_VOXEL_CACHE_MASK = GLASS_VOXEL_CACHE_CAPACITY - 1u;
static const uint GLASS_VOXEL_CACHE_MAX_PROBES = 32u;
static const uint GLASS_VOXEL_COORD_BITS = 10u;
static const uint GLASS_VOXEL_COORD_MASK = (1u << GLASS_VOXEL_COORD_BITS) - 1u;
static const uint GLASS_CACHE_FACE_SIGNATURE_MASK = 0x0fu;
static const uint GLASS_CACHE_BOUNDARY_EXACT_BIT = 1u << 30u;
static const uint GLASS_CACHE_EXACT_PRIMARY_BIT = 1u << 31u;
// Below this projected radius, one cached transport sample remains subpixel-stable.
// Larger Glass voxels keep their cached normal/reflections but recompute the primary
// transmitted branch per pixel so cache sharing cannot expose raster disocclusions.
static const float GLASS_CACHE_FULL_EXACT_FOOTPRINT_PIXELS = 3.0;
static const float GLASS_CACHE_VISIBLE_RADIANCE_DELTA = 0.25;
static const int GLASS_RASTER_BOUNDARY_RADIUS_PIXELS = 6;
// Symmetric two-point prefilter positions along a voxel-face diagonal.
static const float GLASS_VOXEL_FACE_SAMPLE_OFFSET = 0.2886751346 / 256.0;
static const float GLASS_VOXEL_FACE_HALF_EXTENT = 0.5 / 256.0;

struct GlassActivePath
{
    float3 origin;
    float3 direction;
    float3 throughput;
    uint serial;
    uint active;
    uint screen_candidate;
};

struct GlassResolveResult
{
    float3 radiance;
    float3 primary_screen_contribution;
    uint scene_queries;
    uint dda_steps;
    uint interfaces;
    uint peak_active_paths;
    uint tir_events;
    uint throughput_cutoffs;
    uint top_k_pruned;
    uint exhausted;
    uint fallback_reason;
    uint raster_screen_hits;
};

GlassActivePath emptyGlassPath()
{
    GlassActivePath path;
    path.origin = float3(0.0);
    path.direction = float3(0.0, 0.0, -1.0);
    path.throughput = float3(0.0);
    path.serial = 0u;
    path.active = 0u;
    path.screen_candidate = 0u;
    return path;
}

GlassResolveResult emptyGlassResolveResult()
{
    GlassResolveResult result;
    result.radiance = float3(0.0);
    result.primary_screen_contribution = float3(0.0);
    result.scene_queries = 0u;
    result.dda_steps = 0u;
    result.interfaces = 0u;
    result.peak_active_paths = 0u;
    result.tir_events = 0u;
    result.throughput_cutoffs = 0u;
    result.top_k_pruned = 0u;
    result.exhausted = 0u;
    result.fallback_reason = GLASS_FALLBACK_NONE;
    result.raster_screen_hits = 0u;
    return result;
}

uint glassVoxelCellKey(int3 cell)
{
    if (any(cell < int3(0)) || any(cell > int3(GLASS_VOXEL_COORD_MASK))) return 0u;
    uint3 coordinate = uint3(cell);
    uint packed = coordinate.x |
        (coordinate.y << GLASS_VOXEL_COORD_BITS) |
        (coordinate.z << (GLASS_VOXEL_COORD_BITS * 2u));
    return packed + 1u;
}

int3 glassVoxelCellFromKey(uint key)
{
    uint packed = key - 1u;
    return int3(
        packed & GLASS_VOXEL_COORD_MASK,
        (packed >> GLASS_VOXEL_COORD_BITS) & GLASS_VOXEL_COORD_MASK,
        (packed >> (GLASS_VOXEL_COORD_BITS * 2u)) & GLASS_VOXEL_COORD_MASK);
}

int3 glassInterfaceVoxelCell(SceneQueryEvent event)
{
    VoxelMaterial toMaterial = voxelMaterial(event.to_voxel_type, true);
    return toMaterial.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC
        ? event.to_cell
        : event.from_cell;
}

uint glassVoxelCacheHash(uint key)
{
    uint hash = key;
    hash ^= hash >> 16u;
    hash *= 0x7feb352du;
    hash ^= hash >> 15u;
    hash *= 0x846ca68bu;
    hash ^= hash >> 16u;
    return hash;
}

bool glassVoxelCacheSlot(uint key, bool insert, out uint slot, out bool inserted)
{
    inserted = false;
    if (key == 0u)
    {
        slot = 0u;
        return false;
    }
    uint firstSlot = glassVoxelCacheHash(key) & GLASS_VOXEL_CACHE_MASK;
    [loop]
    for (uint probe = 0u; probe < GLASS_VOXEL_CACHE_MAX_PROBES; ++probe)
    {
        slot = (firstSlot + probe) & GLASS_VOXEL_CACHE_MASK;
        uint observedKey = glass_voxel_cache_metadata[slot].x;
        if (observedKey == key) return true;
        if (observedKey == 0u && !insert) return false;
        if (observedKey != 0u) continue;
        InterlockedCompareExchange(
            glass_voxel_cache_metadata[slot].x, 0u, key, observedKey);
        if (observedKey == 0u)
        {
            inserted = true;
            return true;
        }
        if (observedKey == key) return true;
    }
    slot = 0u;
    return false;
}

uint glassInterfaceFaceSignature(SceneQueryEvent event)
{
    uint axes = event.tied_axes & 0x07u;
    uint axis = (axes & 1u) != 0u ? 0u : ((axes & 2u) != 0u ? 1u : 2u);
    return axes | ((event.incident_face_normal[axis] > 0.0 ? 1u : 0u) << 3u);
}

bool loadGlassVoxelCacheRadiance(
    int3 cell, uint faceSignature, out float3 radiance,
    out uint diagnostics)
{
    uint key = glassVoxelCellKey(cell);
    uint slot;
    bool ignoredInserted;
    if (!glassVoxelCacheSlot(key, false, slot, ignoredInserted)) return false;
    uint4 metadata = glass_voxel_cache_metadata[slot];
    if ((metadata.w & (1u << 31u)) == 0u ||
        (metadata.y & GLASS_CACHE_FACE_SIGNATURE_MASK) != faceSignature)
        return false;
    radiance = glass_voxel_cache_radiance[slot * 2u].rgb;
    diagnostics = metadata.z;
    return true;
}

uint2 packGlassDiagnostics(GlassResolveResult result, float3 resolvedColor)
{
    uint packedDebug = (result.fallback_reason & 0x0fu) |
        ((min(result.interfaces, 0xffu) & 0xffu) << 4u) |
        ((min(result.scene_queries, 0x0fu) & 0x0fu) << 12u) |
        ((result.exhausted & 1u) << 16u) |
        ((min(result.dda_steps, 0x3fffu) & 0x3fffu) << 17u) |
        ((result.raster_screen_hits != 0u ? 1u : 0u) << 31u);
    uint packedPathDebug =
        (min(result.peak_active_paths, 0x0fu) & 0x0fu) |
        ((min(result.tir_events, 0xffu) & 0xffu) << 4u) |
        ((min(result.throughput_cutoffs, 0xffu) & 0xffu) << 12u) |
        ((min(result.top_k_pruned, 0xffu) & 0xffu) << 20u) |
        ((any(isnan(resolvedColor)) || any(isinf(resolvedColor)) ? 1u : 0u) << 28u);
    return uint2(packedDebug, packedPathDebug);
}

float glassPathLuminance(float3 throughput)
{
    return sceneQueryThroughputLuminance(throughput);
}

float3 glassSky(float3 direction)
{
    return computeSkyWithSunAndStars(
        normalize(direction), sun_info, camera_info,
        starlight_info, sun_sprite_tex);
}

float3 glassVoxelFallback(SceneQueryEvent event)
{
    float3 albedo = voxelColorByType(voxel_colors, event.to_voxel_type);
    float cosine = max(0.0, dot(normalize(event.incident_face_normal),
                                normalize(sun_info.sun_dir)));
    float3 ambient = albedo * 0.18;
    float3 direct = albedo * sun_info.sun_color * sun_info.sun_luminance * cosine;
    return ambient + direct + voxelSurfaceEmission(voxel_colors, event.to_voxel_type);
}

float glassCameraDepthFromNdc(float depth)
{
    float4 viewPosition = mul(
        camera_info.proj_mat_inv, float4(0.0, 0.0, depth, 1.0));
    return -viewPosition.z / viewPosition.w;
}

bool glassDepthPrecedes(float nearerDepth, float fartherDepth)
{
    if (!(nearerDepth < fartherDepth)) return false;
    return asuint(fartherDepth) - asuint(nearerDepth) > GLASS_DEPTH_ORDER_ULPS;
}

bool glassTerminalIsBehindLocalFront(int2 coordinate, float terminalDepth)
{
    // Refraction moves the terminal to another screen coordinate. Comparing it
    // with the source pixel's front depth rejects valid samples at oblique views.
    uint localFrontData = glass_front_data_tex[coordinate];
    if ((localFrontData & 1u) == 0u) return true;
    float localFrontDepth = glass_front_depth_tex[coordinate];
    return glassDepthPrecedes(localFrontDepth, terminalDepth);
}

bool validateGlassTerminalCoordinate(
    int2 coordinate, float hitDepth, out uint provenance,
    out uint rejectionReason)
{
    float sampledDepth = unified_opaque_depth_tex[coordinate];
    provenance = opaque_provenance_tex[coordinate];
    // Raster geometry must be discovered by the ordered projected-segment walk above.
    // Accepting it only at the arbitrary voxel terminal coordinate can skip a nearer raster hit.
    if (provenance != OPAQUE_PROVENANCE_VOXEL)
    {
        rejectionReason = GLASS_FALLBACK_REJECTED_PROVENANCE;
        return false;
    }
    float sampledCameraDepth = glassCameraDepthFromNdc(sampledDepth);
    float hitCameraDepth = glassCameraDepthFromNdc(hitDepth);
    bool terminalDepthMatches = abs(sampledCameraDepth - hitCameraDepth) <=
        GLASS_SCREEN_TRACE_THICKNESS_WORLD;
    if (!glassTerminalIsBehindLocalFront(coordinate, sampledDepth) ||
        !terminalDepthMatches)
    {
        rejectionReason = GLASS_FALLBACK_REJECTED_DEPTH;
        return false;
    }
    rejectionReason = GLASS_FALLBACK_NONE;
    return true;
}

bool projectGlassWorldToScreen(
    float3 position, uint2 extent,
    out float2 uv, out float2 pixel, out float depth)
{
    float4 clip = mul(camera_info.view_proj_mat, float4(position, 1.0));
    if (clip.w <= 0.0)
    {
        uv = float2(0.0);
        pixel = float2(0.0);
        depth = 1.0;
        return false;
    }
    uv = clip.xy / clip.w * 0.5 + 0.5;
    pixel = uv * float2(extent);
    depth = clip.z / clip.w;
    return all(uv > float2(0.0)) && all(uv < float2(1.0));
}

bool traceGlassOpaqueScreenSegment(
    float3 segmentOrigin, SceneQueryEvent terminalEvent,
    uint2 extent, out float2 hitUv, out uint hitProvenance)
{
    float2 startUv;
    float2 startPixel;
    float startDepth;
    float2 endUv;
    float2 endPixel;
    float endDepth;
    if (!projectGlassWorldToScreen(
            segmentOrigin, extent, startUv, startPixel, startDepth) ||
        !projectGlassWorldToScreen(
            terminalEvent.position, extent, endUv, endPixel, endDepth))
    {
        hitUv = endUv;
        hitProvenance = 0u;
        return false;
    }

    // Walk the projected segment in screen order. Each iteration owns one contiguous
    // pixel-sized ray-depth interval, so thin raster geometry cannot fall between sparse
    // world-space samples. NDC depth is linear along the projected line.
    float2 pixelDelta = endPixel - startPixel;
    float projectedLength = max(abs(pixelDelta.x), abs(pixelDelta.y));
    uint stepCount = clamp(
        uint(ceil(projectedLength)), 1u, GLASS_SCREEN_TRACE_MAX_STEPS);
    [loop]
    for (uint stepIndex = 0u;
         stepIndex < GLASS_SCREEN_TRACE_MAX_STEPS;
         ++stepIndex)
    {
        if (stepIndex >= stepCount) break;
        float intervalStart = float(stepIndex) / float(stepCount);
        float intervalEnd = float(stepIndex + 1u) / float(stepCount);
        float intervalCenter = (intervalStart + intervalEnd) * 0.5;
        float2 samplePixel = lerp(startPixel, endPixel, intervalCenter);
        int2 coordinate = clamp(
            int2(floor(samplePixel)), int2(0), int2(extent) - int2(1));
        uint provenance = opaque_provenance_tex[coordinate];
        bool finalInterval = stepIndex + 1u == stepCount;
        if (provenance != OPAQUE_PROVENANCE_RASTER &&
            !(finalInterval &&
              terminalEvent.kind == SCENE_QUERY_EVENT_OPAQUE &&
              provenance == OPAQUE_PROVENANCE_VOXEL))
        {
            continue;
        }

        float rayDepthStart = glassCameraDepthFromNdc(
            lerp(startDepth, endDepth, intervalStart));
        float rayDepthEnd = glassCameraDepthFromNdc(
            lerp(startDepth, endDepth, intervalEnd));
        float rayDepthMin = min(rayDepthStart, rayDepthEnd);
        float rayDepthMax = max(rayDepthStart, rayDepthEnd);
        float sampledDepth = unified_opaque_depth_tex[coordinate];
        float sampledCameraDepth = glassCameraDepthFromNdc(sampledDepth);
        if (sampledCameraDepth <
                rayDepthMin - GLASS_SCREEN_TRACE_THICKNESS_WORLD ||
            sampledCameraDepth >
                rayDepthMax + GLASS_SCREEN_TRACE_THICKNESS_WORLD)
        {
            continue;
        }
        if (provenance == OPAQUE_PROVENANCE_VOXEL &&
            !glassTerminalIsBehindLocalFront(coordinate, sampledDepth))
        {
            continue;
        }

        hitUv = (float2(coordinate) + 0.5) / float2(extent);
        hitProvenance = provenance;
        return true;
    }

    hitUv = endUv;
    hitProvenance = 0u;
    return false;
}

float3 sampleGlassUnrefractedFallback(
    float2 sourceScreenUv, SceneQueryEvent event, float3 segmentDirection,
    out uint sourceProvenance)
{
    uint width;
    uint height;
    unified_opaque_depth_tex.GetDimensions(width, height);
    int2 coordinate = clamp(
        int2(sourceScreenUv * float2(width, height)),
        int2(0), int2(width, height) - int2(1));
    uint frontData = glass_front_data_tex[coordinate];
    float opaqueDepth = unified_opaque_depth_tex[coordinate];
    float frontDepth = glass_front_depth_tex[coordinate];
    if ((frontData & 1u) != 0u &&
        !glassDepthPrecedes(opaqueDepth, frontDepth))
    {
        // SceneQuery has no representation for raster-only producers such as flora.
        // This source texel is valid only when its depth is authoritatively behind Glass.
        sourceProvenance = opaque_provenance_tex[coordinate];
        return unified_opaque_hdr_tex.Load(int3(coordinate, 0)).rgb;
    }
    sourceProvenance = 0u;
    return event.kind == SCENE_QUERY_EVENT_OPAQUE
        ? glassVoxelFallback(event)
        : glassSky(segmentDirection);
}

float3 sampleGlassTerminalRadiance(
    SceneQueryEvent event, uint screenCandidate, float2 sourceScreenUv,
    float3 segmentOrigin, float3 segmentDirection,
    out uint fallbackReason, out uint terminalProvenance)
{
    bool opaqueTerminal = event.kind == SCENE_QUERY_EVENT_OPAQUE;
    fallbackReason = opaqueTerminal
        ? GLASS_FALLBACK_VOXEL
        : GLASS_FALLBACK_SKY;
    terminalProvenance = 0u;
    if (screenCandidate == 0u)
        return opaqueTerminal
            ? glassVoxelFallback(event)
            : glassSky(segmentDirection);

    uint width;
    uint height;
    unified_opaque_depth_tex.GetDimensions(width, height);
    uint2 extent = uint2(width, height);
    float2 uv;
    float2 terminalPixel;
    float hitDepth;
    if (!projectGlassWorldToScreen(
            event.position, extent, uv, terminalPixel, hitDepth))
    {
        fallbackReason = GLASS_FALLBACK_OFFSCREEN;
        return sampleGlassUnrefractedFallback(
            sourceScreenUv, event, segmentDirection, terminalProvenance);
    }

    float2 segmentHitUv;
    uint segmentHitProvenance;
    if (traceGlassOpaqueScreenSegment(
            segmentOrigin, event, extent,
            segmentHitUv, segmentHitProvenance))
    {
        fallbackReason = GLASS_FALLBACK_SCREEN_HIT;
        terminalProvenance = segmentHitProvenance;
        if (segmentHitProvenance == OPAQUE_PROVENANCE_VOXEL)
        {
            uint sourceProvenance;
            float3 sourceColor = sampleGlassUnrefractedFallback(
                sourceScreenUv, event, segmentDirection, sourceProvenance);
            // Raster-only producers are absent from the voxel SceneQuery. If refraction found
            // no raster surface, preserve one that is authoritatively visible behind this
            // Glass pixel instead of erasing it with the voxel terminal.
            if (sourceProvenance == OPAQUE_PROVENANCE_RASTER)
            {
                terminalProvenance = sourceProvenance;
                return sourceColor;
            }
        }
        int2 segmentHitCoordinate = int2(segmentHitUv * float2(extent));
        // Depth and provenance validated this integer texel. A filtered read would blend
        // an unvalidated neighboring object across a silhouette.
        return unified_opaque_hdr_tex.Load(int3(segmentHitCoordinate, 0)).rgb;
    }

    if (!opaqueTerminal)
        return sampleGlassUnrefractedFallback(
            sourceScreenUv, event, segmentDirection, terminalProvenance);

    int2 coordinate = int2(uv * float2(width, height));
    uint provenance;
    uint rejectionReason;
    if (validateGlassTerminalCoordinate(
            coordinate, hitDepth, provenance, rejectionReason))
    {
        fallbackReason = GLASS_FALLBACK_SCREEN_HIT;
        terminalProvenance = provenance;
        uint sourceProvenance;
        float3 sourceColor = sampleGlassUnrefractedFallback(
            sourceScreenUv, event, segmentDirection, sourceProvenance);
        if (sourceProvenance == OPAQUE_PROVENANCE_RASTER)
        {
            terminalProvenance = sourceProvenance;
            return sourceColor;
        }
        return unified_opaque_hdr_tex.Load(int3(coordinate, 0)).rgb;
    }

    fallbackReason = rejectionReason;
    return sampleGlassUnrefractedFallback(
        sourceScreenUv, event, segmentDirection, terminalProvenance);
}

void insertGlassPath(inout GlassActivePath paths[SCENE_QUERY_MAX_ACTIVE_PATHS],
                     GlassActivePath candidate,
                     inout GlassResolveResult result)
{
    if (glassPathLuminance(candidate.throughput) < SCENE_QUERY_THROUGHPUT_CUTOFF)
    {
        result.throughput_cutoffs += 1u;
        return;
    }
    int emptyIndex = -1;
    int weakestIndex = -1;
    float weakestLuminance = 1.0e30;
    uint activeCount = 0u;
    [unroll]
    for (uint index = 0u; index < SCENE_QUERY_MAX_ACTIVE_PATHS; ++index)
    {
        if (paths[index].active == 0u)
        {
            if (emptyIndex < 0) emptyIndex = int(index);
            continue;
        }
        activeCount += 1u;
        float luminance = glassPathLuminance(paths[index].throughput);
        if (luminance < weakestLuminance ||
            (luminance == weakestLuminance &&
             paths[index].serial > paths[uint(weakestIndex)].serial))
        {
            weakestLuminance = luminance;
            weakestIndex = int(index);
        }
    }
    if (emptyIndex >= 0)
    {
        paths[uint(emptyIndex)] = candidate;
        result.peak_active_paths = max(result.peak_active_paths, activeCount + 1u);
        return;
    }
    float candidateLuminance = glassPathLuminance(candidate.throughput);
    if (candidateLuminance > weakestLuminance ||
        (candidateLuminance == weakestLuminance &&
         candidate.serial < paths[uint(weakestIndex)].serial))
    {
        paths[uint(weakestIndex)] = candidate;
    }
    result.top_k_pruned += 1u;
}

SceneQueryEvent unpackGlassFrontEvent(uint packed, float2 screenUv,
                                      float glassFrontDepth, Ray cameraRay)
{
    SceneQueryEvent event;
    event.kind = SCENE_QUERY_EVENT_INTERFACE;
    event.position = projectScreenUvToWorld(
        screenUv, glassFrontDepth, camera_info.view_proj_mat_inv);
    event.tied_axes = (packed >> 1u) & 0x07u;
    int3 rayStep = int3(sign(cameraRay.direction));
    int3 crossedStep = int3(
        (event.tied_axes & 1u) != 0u ? rayStep.x : 0,
        (event.tied_axes & 2u) != 0u ? rayStep.y : 0,
        (event.tied_axes & 4u) != 0u ? rayStep.z : 0);
    event.incident_face_normal = normalize(-float3(crossedStep));
    event.to_cell = int3(floor(
        event.position * 256.0 + float3(crossedStep) * 0.25));
    event.from_cell = event.to_cell - crossedStep;
    float selectedDirection = 0.0;
    float exactDistance = 0.0;
    if (crossedStep.x != 0)
    {
        float boundary = float(crossedStep.x > 0
            ? event.to_cell.x : event.from_cell.x) / 256.0;
        selectedDirection = abs(cameraRay.direction.x);
        exactDistance = (boundary - cameraRay.origin.x) / cameraRay.direction.x;
    }
    if (crossedStep.y != 0 && abs(cameraRay.direction.y) > selectedDirection)
    {
        float boundary = float(crossedStep.y > 0
            ? event.to_cell.y : event.from_cell.y) / 256.0;
        selectedDirection = abs(cameraRay.direction.y);
        exactDistance = (boundary - cameraRay.origin.y) / cameraRay.direction.y;
    }
    if (crossedStep.z != 0 && abs(cameraRay.direction.z) > selectedDirection)
    {
        float boundary = float(crossedStep.z > 0
            ? event.to_cell.z : event.from_cell.z) / 256.0;
        exactDistance = (boundary - cameraRay.origin.z) / cameraRay.direction.z;
    }
    event.position = cameraRay.origin + cameraRay.direction * exactDistance;
    event.to_cell = int3(floor(
        event.position * 256.0 + float3(crossedStep) * 0.25));
    event.from_cell = event.to_cell - crossedStep;
    // Depth unprojection can land a few ULPs to either side of the voxel plane. Snap only
    // crossed axes back to their exact integer boundary before applying the cell-directed bias.
    if (crossedStep.x != 0)
        event.position.x = float(crossedStep.x > 0
            ? event.to_cell.x : event.from_cell.x) / 256.0;
    if (crossedStep.y != 0)
        event.position.y = float(crossedStep.y > 0
            ? event.to_cell.y : event.from_cell.y) / 256.0;
    if (crossedStep.z != 0)
        event.position.z = float(crossedStep.z > 0
            ? event.to_cell.z : event.from_cell.z) / 256.0;
    event.from_voxel_type = (packed >> 8u) & 0x0fu;
    event.to_voxel_type = (packed >> 12u) & 0x0fu;
    event.medium_distance_world = length(event.position - cameraRay.origin);
    event.dda_steps = 0u;
    return event;
}

void spawnGlassInterfacePaths(
    SceneQueryEvent event, GlassActivePath path,
    inout GlassActivePath paths[SCENE_QUERY_MAX_ACTIVE_PATHS],
    inout GlassResolveResult result, inout uint nextSerial)
{
    result.interfaces += 1u;
    if (result.interfaces > SCENE_QUERY_MAX_INTERFACE_EVENTS)
    {
        result.exhausted = 1u;
        result.fallback_reason = GLASS_FALLBACK_EXHAUSTED;
        return;
    }
    VoxelMaterial fromMaterial = voxelMaterial(event.from_voxel_type, true);
    VoxelMaterial toMaterial = voxelMaterial(event.to_voxel_type, true);
    float fresnel = sceneQueryDielectricFresnel(
        path.direction, event.incident_face_normal,
        fromMaterial.ior, toMaterial.ior);
    float3 reflectedDirection = normalize(reflect(
        path.direction, event.incident_face_normal));
    GlassActivePath reflected = emptyGlassPath();
    reflected.origin = sceneQueryOffsetAcrossInterface(event, false, 256.0);
    reflected.direction = reflectedDirection;
    reflected.throughput = path.throughput * fresnel;
    reflected.serial = nextSerial++;
    reflected.active = 1u;
    reflected.screen_candidate = 0u;

    float3 transmittedDirection;
    if (sceneQueryRefractDielectric(
            path.direction, event.incident_face_normal,
            fromMaterial.ior, toMaterial.ior, transmittedDirection))
    {
        GlassActivePath transmitted = emptyGlassPath();
        transmitted.origin = sceneQueryOffsetAcrossInterface(event, true, 256.0);
        transmitted.direction = transmittedDirection;
        transmitted.throughput = path.throughput * (1.0 - fresnel);
        transmitted.serial = nextSerial++;
        transmitted.active = 1u;
        transmitted.screen_candidate = path.screen_candidate;
        insertGlassPath(paths, transmitted, result);
    }
    else
    {
        result.tir_events += 1u;
        reflected.throughput = path.throughput;
    }
    insertGlassPath(paths, reflected, result);
}

GlassResolveResult traceGlassRadianceFromFrontEvent(
    Ray cameraRay, float2 screenUv, float glassFrontDepth,
    SceneQueryEvent frontEvent)
{
    GlassResolveResult result = emptyGlassResolveResult();
    GlassActivePath paths[SCENE_QUERY_MAX_ACTIVE_PATHS];
    [unroll]
    for (uint index = 0u; index < SCENE_QUERY_MAX_ACTIVE_PATHS; ++index)
        paths[index] = emptyGlassPath();
    GlassActivePath cameraPath = emptyGlassPath();
    cameraPath.origin = cameraRay.origin;
    cameraPath.direction = normalize(cameraRay.direction);
    cameraPath.throughput = float3(1.0);
    cameraPath.active = 1u;
    cameraPath.screen_candidate = 1u;
    uint nextSerial = 1u;

    // The front event was produced by the primary pass and is reused here without another DDA.
    // It still consumes one slot in the cross-pass path budget.
    result.scene_queries = 1u;
    VoxelMaterial frontMedium = voxelMaterial(
        frontEvent.from_voxel_type, true);
    if (frontMedium.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC)
    {
        cameraPath.throughput *= sceneQueryBeerLambert(
            sceneQueryAbsorptionCoefficient(frontMedium),
            frontEvent.medium_distance_world);
    }
    spawnGlassInterfacePaths(
        frontEvent, cameraPath, paths, result, nextSerial);

    for (uint queryIndex = 1u; queryIndex < SCENE_QUERY_MAX_SCENE_QUERIES; ++queryIndex)
    {
        int selectedIndex = -1;
        float selectedLuminance = -1.0;
        uint selectedSerial = 0xffffffffu;
        [unroll]
        for (uint index = 0u; index < SCENE_QUERY_MAX_ACTIVE_PATHS; ++index)
        {
            if (paths[index].active == 0u) continue;
            float luminance = glassPathLuminance(paths[index].throughput);
            if (luminance > selectedLuminance ||
                (luminance == selectedLuminance && paths[index].serial < selectedSerial))
            {
                selectedIndex = int(index);
                selectedLuminance = luminance;
                selectedSerial = paths[index].serial;
            }
        }
        if (selectedIndex < 0) return result;

        GlassActivePath path = paths[uint(selectedIndex)];
        paths[uint(selectedIndex)].active = 0u;
        result.scene_queries += 1u;
        SceneQueryEvent event = walkVoxelMediaToNextEvent(
            chunk_atlas, path.origin, path.direction, 256.0, true,
            SCENE_QUERY_MAX_DDA_STEPS);
        result.dda_steps += event.dda_steps;
        VoxelMaterial fromMaterial = voxelMaterial(event.from_voxel_type, true);
        if (fromMaterial.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC)
        {
            path.throughput *= sceneQueryBeerLambert(
                sceneQueryAbsorptionCoefficient(fromMaterial),
                event.medium_distance_world);
        }
        if (glassPathLuminance(path.throughput) < SCENE_QUERY_THROUGHPUT_CUTOFF)
        {
            result.throughput_cutoffs += 1u;
            continue;
        }

        if (event.kind == SCENE_QUERY_EVENT_OPAQUE ||
            event.kind == SCENE_QUERY_EVENT_MISS)
        {
            uint fallbackReason;
            uint terminalProvenance;
            float3 terminal = sampleGlassTerminalRadiance(
                event, path.screen_candidate, screenUv,
                path.origin, path.direction,
                fallbackReason, terminalProvenance);
            result.radiance += path.throughput * terminal;
            if (path.screen_candidate != 0u)
                result.primary_screen_contribution += path.throughput * terminal;
            if (terminalProvenance == OPAQUE_PROVENANCE_RASTER)
                result.raster_screen_hits += 1u;
            if (fallbackReason == GLASS_FALLBACK_SCREEN_HIT ||
                result.fallback_reason == GLASS_FALLBACK_NONE)
                result.fallback_reason = fallbackReason;
            continue;
        }
        if (event.kind != SCENE_QUERY_EVENT_INTERFACE)
        {
            result.exhausted = 1u;
            result.fallback_reason = GLASS_FALLBACK_EXHAUSTED;
            continue;
        }

        spawnGlassInterfacePaths(
            event, path, paths, result, nextSerial);
    }

    // The fixed query cap has a defined, world-safe residual fallback. True DDA/interface
    // failures remain exhaustion errors; reaching this cap does not produce an undefined pixel.
    uint budgetFallbacks = 0u;
    [unroll]
    for (uint index = 0u; index < SCENE_QUERY_MAX_ACTIVE_PATHS; ++index)
    {
        if (paths[index].active == 0u) continue;
        result.radiance += paths[index].throughput * glassSky(paths[index].direction);
        if (paths[index].screen_candidate != 0u)
        {
            result.primary_screen_contribution +=
                paths[index].throughput * glassSky(paths[index].direction);
        }
        budgetFallbacks += 1u;
    }
    if (budgetFallbacks != 0u)
        result.fallback_reason = GLASS_FALLBACK_QUERY_BUDGET;
    if (result.exhausted != 0u) result.fallback_reason = GLASS_FALLBACK_EXHAUSTED;
    return result;
}

float3 traceGlassPrimaryScreenContributionFromFrontEvent(
    Ray cameraRay, float2 screenUv, SceneQueryEvent frontEvent)
{
    // Reflected branches never retain screen_candidate. Recompute only the unique chain of
    // transmitted branches whose raster terminal can vary inside one cached Glass voxel;
    // the complete cached result continues to supply every reflected branch.
    float3 throughput = float3(1.0);
    VoxelMaterial frontMedium = voxelMaterial(
        frontEvent.from_voxel_type, true);
    if (frontMedium.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC)
    {
        throughput *= sceneQueryBeerLambert(
            sceneQueryAbsorptionCoefficient(frontMedium),
            frontEvent.medium_distance_world);
    }

    SceneQueryEvent interfaceEvent = frontEvent;
    float3 direction = normalize(cameraRay.direction);
    uint queriesUsed = 1u;
    uint interfaces = 0u;
    [loop]
    while (interfaces < SCENE_QUERY_MAX_INTERFACE_EVENTS)
    {
        interfaces += 1u;
        VoxelMaterial fromMaterial = voxelMaterial(
            interfaceEvent.from_voxel_type, true);
        VoxelMaterial toMaterial = voxelMaterial(
            interfaceEvent.to_voxel_type, true);
        float fresnel = sceneQueryDielectricFresnel(
            direction, interfaceEvent.incident_face_normal,
            fromMaterial.ior, toMaterial.ior);
        float3 transmittedDirection;
        if (!sceneQueryRefractDielectric(
                direction, interfaceEvent.incident_face_normal,
                fromMaterial.ior, toMaterial.ior, transmittedDirection))
        {
            return float3(0.0);
        }
        throughput *= 1.0 - fresnel;
        if (glassPathLuminance(throughput) < SCENE_QUERY_THROUGHPUT_CUTOFF)
            return float3(0.0);

        float3 origin = sceneQueryOffsetAcrossInterface(
            interfaceEvent, true, 256.0);
        direction = normalize(transmittedDirection);
        if (queriesUsed >= SCENE_QUERY_MAX_SCENE_QUERIES)
            return throughput * glassSky(direction);

        SceneQueryEvent event = walkVoxelMediaToNextEvent(
            chunk_atlas, origin, direction, 256.0, true,
            SCENE_QUERY_MAX_DDA_STEPS);
        queriesUsed += 1u;
        fromMaterial = voxelMaterial(event.from_voxel_type, true);
        if (fromMaterial.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC)
        {
            throughput *= sceneQueryBeerLambert(
                sceneQueryAbsorptionCoefficient(fromMaterial),
                event.medium_distance_world);
        }
        if (glassPathLuminance(throughput) < SCENE_QUERY_THROUGHPUT_CUTOFF)
            return float3(0.0);

        if (event.kind == SCENE_QUERY_EVENT_OPAQUE ||
            event.kind == SCENE_QUERY_EVENT_MISS)
        {
            uint fallbackReason;
            uint terminalProvenance;
            float3 terminal = sampleGlassTerminalRadiance(
                event, 1u, screenUv, origin, direction,
                fallbackReason, terminalProvenance);
            return throughput * terminal;
        }
        if (event.kind != SCENE_QUERY_EVENT_INTERFACE)
            return float3(0.0);
        interfaceEvent = event;
    }
    return float3(0.0);
}

uint glassFallbackPriority(uint reason)
{
    if (reason == GLASS_FALLBACK_EXHAUSTED) return 9u;
    if (reason == GLASS_FALLBACK_QUERY_BUDGET) return 8u;
    if (reason == GLASS_FALLBACK_REJECTED_DEPTH ||
        reason == GLASS_FALLBACK_REJECTED_PROVENANCE) return 7u;
    if (reason == GLASS_FALLBACK_OFFSCREEN) return 6u;
    if (reason == GLASS_FALLBACK_VOXEL) return 5u;
    if (reason == GLASS_FALLBACK_SKY) return 4u;
    if (reason == GLASS_FALLBACK_SCREEN_HIT) return 3u;
    return reason == GLASS_FALLBACK_FOREGROUND ? 2u : 0u;
}

void accumulateGlassResolveSample(
    GlassResolveResult sample, inout GlassResolveResult aggregate)
{
    aggregate.radiance += sample.radiance;
    aggregate.primary_screen_contribution += sample.primary_screen_contribution;
    aggregate.scene_queries = max(aggregate.scene_queries, sample.scene_queries);
    aggregate.dda_steps += sample.dda_steps;
    aggregate.interfaces = max(aggregate.interfaces, sample.interfaces);
    aggregate.peak_active_paths = max(
        aggregate.peak_active_paths, sample.peak_active_paths);
    aggregate.tir_events += sample.tir_events;
    aggregate.throughput_cutoffs += sample.throughput_cutoffs;
    aggregate.top_k_pruned += sample.top_k_pruned;
    aggregate.exhausted |= sample.exhausted;
    aggregate.raster_screen_hits += sample.raster_screen_hits;
    if (glassFallbackPriority(sample.fallback_reason) >
        glassFallbackPriority(aggregate.fallback_reason))
    {
        aggregate.fallback_reason = sample.fallback_reason;
    }
}

void averageGlassResolveSamples(
    uint sampleCount, inout GlassResolveResult aggregate)
{
    aggregate.radiance /= float(sampleCount);
    aggregate.primary_screen_contribution /= float(sampleCount);
    aggregate.dda_steps = (aggregate.dda_steps + sampleCount / 2u) / sampleCount;
    aggregate.tir_events = (aggregate.tir_events + sampleCount / 2u) / sampleCount;
    aggregate.throughput_cutoffs =
        (aggregate.throughput_cutoffs + sampleCount / 2u) / sampleCount;
    aggregate.top_k_pruned =
        (aggregate.top_k_pruned + sampleCount / 2u) / sampleCount;
}

uint glassInterfaceAxis(SceneQueryEvent event)
{
    uint axes = event.tied_axes & 0x07u;
    if ((axes & 1u) != 0u) return 0u;
    if ((axes & 2u) != 0u) return 1u;
    return 2u;
}

void glassInterfaceFaceFrame(
    int3 glassCell, SceneQueryEvent event,
    out float3 faceCenter, out float3 tangentU, out float3 tangentV)
{
    uint axis = glassInterfaceAxis(event);
    tangentU = axis == 0u ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0);
    tangentV = axis == 2u ? float3(0.0, 1.0, 0.0) : float3(0.0, 0.0, 1.0);
    VoxelMaterial toMaterial = voxelMaterial(event.to_voxel_type, true);
    float glassSide = toMaterial.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC
        ? 1.0 : -1.0;
    float3 axisNormal = float3(0.0);
    axisNormal[axis] = sign(event.incident_face_normal[axis]) * glassSide;
    faceCenter = (float3(glassCell) + 0.5) / 256.0 +
        axisNormal * GLASS_VOXEL_FACE_HALF_EXTENT;
}

GlassResolveResult traceGlassFrontSample(
    float3 samplePosition, SceneQueryEvent frontEvent)
{
    Ray sampleRay;
    sampleRay.origin = camera_info.pos.xyz;
    sampleRay.direction = normalize(samplePosition - sampleRay.origin);
    sampleRay.inverse_direction = 1.0 / sampleRay.direction;
    frontEvent.position = samplePosition;
    frontEvent.medium_distance_world = length(samplePosition - sampleRay.origin);
    float4 clip = mul(
        camera_info.view_proj_mat, float4(samplePosition, 1.0));
    if (clip.w <= 0.0)
    {
        GlassResolveResult result = emptyGlassResolveResult();
        result.radiance = glassSky(sampleRay.direction);
        result.fallback_reason = GLASS_FALLBACK_OFFSCREEN;
        return result;
    }
    float2 screenUv = clip.xy / clip.w * 0.5 + 0.5;
    float frontDepth = clip.z / clip.w;
    return traceGlassRadianceFromFrontEvent(
        sampleRay, screenUv, frontDepth, frontEvent);
}

float3 glassFrontDebugColor(uint packed)
{
    if ((packed & 1u) == 0u) return float3(0.0);
    uint axes = (packed >> 1u) & 0x07u;
    float signValue = ((packed >> 4u) & 1u) != 0u ? 1.0 : 0.35;
    return float3((axes & 1u) != 0u ? 1.0 : 0.0,
                  (axes & 2u) != 0u ? 1.0 : 0.0,
                  (axes & 4u) != 0u ? 1.0 : 0.0) * signValue;
}

float3 provenanceDebugColor(uint provenance)
{
    if (provenance == OPAQUE_PROVENANCE_VOXEL) return float3(0.15, 0.45, 1.0);
    if (provenance == OPAQUE_PROVENANCE_RASTER) return float3(0.15, 1.0, 0.3);
    if (provenance == OPAQUE_PROVENANCE_SKY) return float3(0.1, 0.1, 0.1);
    return float3(1.0, 0.25, 0.05);
}

float3 screenValidityDebugColor(uint reason)
{
    if (reason == GLASS_FALLBACK_SCREEN_HIT) return float3(0.1, 1.0, 0.2);
    if (reason == GLASS_FALLBACK_VOXEL) return float3(0.1, 0.35, 1.0);
    if (reason == GLASS_FALLBACK_SKY) return float3(0.15, 0.15, 0.15);
    if (reason == GLASS_FALLBACK_FOREGROUND) return float3(1.0, 1.0, 1.0);
    if (reason == GLASS_FALLBACK_EXHAUSTED) return float3(1.0, 0.0, 1.0);
    if (reason == GLASS_FALLBACK_QUERY_BUDGET) return float3(1.0, 0.8, 0.05);
    if (reason == GLASS_FALLBACK_NONE) return float3(0.0);
    return float3(1.0, 0.25, 0.05);
}

float3 applyGlassCameraEffects(float3 color, float2 screenUv)
{
    color += sampleLensFlare(screenUv, lens_flare_output_tex);
    float sunVisibility = sunLuminanceFromDirection(sun_info.sun_dir, 1.0);
    float godRayWeight = sampleGodRay(screenUv, god_ray_output_tex) *
                         god_ray_info.weight * sunVisibility;
    float3 godRayColor = srgbToLinear(god_ray_info.color);
    return lerp(color, godRayColor, godRayWeight);
}

void buildGlassVoxelCache(uint2 pixel, uint2 extent)
{
    int2 coordinate = int2(pixel);
    uint frontData = glass_front_data_tex[coordinate];
    if ((frontData & 1u) == 0u) return;
    float frontDepth = glass_front_depth_tex[coordinate];
    float opaqueDepth = unified_opaque_depth_tex[coordinate];
    if (glassDepthPrecedes(opaqueDepth, frontDepth)) return;

    float2 screenUv = (float2(pixel) + 0.5) / float2(extent);
    Ray cameraRay = generateCameraRay(screenUv, camera_info.view_proj_mat_inv);
    SceneQueryEvent frontEvent = unpackGlassFrontEvent(
        frontData, screenUv, frontDepth, cameraRay);
    // Every visible pixel contributes only its authoritative front Glass cell. Concurrent
    // pixels that see the same cell converge on one hash slot and one active-list entry.
    uint key = glassVoxelCellKey(glassInterfaceVoxelCell(frontEvent));
    uint slot;
    bool inserted;
    if (!glassVoxelCacheSlot(key, true, slot, inserted)) return;
    if (inserted)
    {
        uint activeIndex;
        InterlockedAdd(glass_voxel_cache_active_count[0], 1u, activeIndex);
        if (activeIndex < GLASS_VOXEL_CACHE_CAPACITY)
            glass_voxel_cache_active_slots[activeIndex] = slot;
    }
}

void shadeGlassVoxelCache(uint activeIndex)
{
    if (activeIndex >= min(
            glass_voxel_cache_active_count[0], GLASS_VOXEL_CACHE_CAPACITY)) return;
    uint slot = glass_voxel_cache_active_slots[activeIndex];
    uint key = glass_voxel_cache_metadata[slot].x;
    if (key == 0u) return;
    int3 glassCell = glassVoxelCellFromKey(key);
    float3 cellCenter = (float3(glassCell) + 0.5) / 256.0;
    // One camera-to-center ray defines one geometric DDA normal and one transport color for the
    // entire visible voxel. This is the deliberate one-normal/one-color-per-voxel invariant.
    Ray representativeRay;
    representativeRay.origin = camera_info.pos.xyz;
    representativeRay.direction = normalize(cellCenter - representativeRay.origin);
    representativeRay.inverse_direction = 1.0 / representativeRay.direction;

    SceneQueryEvent frontEvent = walkVoxelMediaToNextEvent(
        chunk_atlas, representativeRay.origin, representativeRay.direction,
        256.0, true, SCENE_QUERY_MAX_DDA_STEPS);
    GlassResolveResult result = emptyGlassResolveResult();
    if (frontEvent.kind == SCENE_QUERY_EVENT_INTERFACE)
    {
        // A single-axis event owns a rectangular face. Always prefilter its transport across
        // two symmetric positions: turning this off below a one-pixel projected footprint
        // aliases the cell-center visibility signal into stable distance-dependent moire bands.
        // Edge/corner events have no single face to integrate and retain their exact event ray.
        if (countbits(frontEvent.tied_axes & 0x07u) == 1u)
        {
            float3 faceCenter;
            float3 tangentU;
            float3 tangentV;
            glassInterfaceFaceFrame(
                glassCell, frontEvent, faceCenter, tangentU, tangentV);
            // Each deterministic sample still splits both Fresnel branches; their mean remains
            // one geometric normal and one cached transport color for the entire voxel.
            [unroll]
            for (uint sampleIndex = 0u; sampleIndex < 2u; ++sampleIndex)
            {
                float u = sampleIndex != 0u ? 1.0 : -1.0;
                float v = u;
                float3 samplePosition = faceCenter +
                    (tangentU * u + tangentV * v) *
                    GLASS_VOXEL_FACE_SAMPLE_OFFSET;
                GlassResolveResult sample = traceGlassFrontSample(
                    samplePosition, frontEvent);
                accumulateGlassResolveSample(sample, result);
            }
            averageGlassResolveSamples(2u, result);
        }
        else
        {
            result = traceGlassFrontSample(frontEvent.position, frontEvent);
        }
    }
    else if (frontEvent.kind == SCENE_QUERY_EVENT_MISS)
    {
        result.radiance = glassSky(representativeRay.direction);
        result.fallback_reason = GLASS_FALLBACK_SKY;
    }
    else
    {
        result.radiance = glassSky(representativeRay.direction);
        result.exhausted = frontEvent.kind == SCENE_QUERY_EVENT_STEP_BUDGET ? 1u : 0u;
        result.fallback_reason = result.exhausted != 0u
            ? GLASS_FALLBACK_EXHAUSTED
            : GLASS_FALLBACK_SKY;
    }

    glass_voxel_cache_metadata[slot].y = frontEvent.kind == SCENE_QUERY_EVENT_INTERFACE
        ? glassInterfaceFaceSignature(frontEvent)
        : 0u;
    glass_voxel_cache_radiance[slot * 2u] = float4(result.radiance, 1.0);
    glass_voxel_cache_radiance[slot * 2u + 1u] =
        float4(result.primary_screen_contribution, 1.0);
    uint2 diagnostics = packGlassDiagnostics(result, result.radiance);
    glass_voxel_cache_metadata[slot].z = diagnostics.x;
    glass_voxel_cache_metadata[slot].w = diagnostics.y | (1u << 31u);
}

bool glassVoxelTransportIsDiscontinuous(
    int3 glassCell, uint faceSignature,
    uint centerDiagnostics, float3 centerRadiance)
{
    uint axes = faceSignature & 0x07u;
    if (countbits(axes) != 1u)
        return false;

    uint axis = (axes & 1u) != 0u ? 0u : ((axes & 2u) != 0u ? 1u : 2u);
    int3 tangentU = axis == 0u ? int3(0, 1, 0) : int3(1, 0, 0);
    int3 tangentV = axis == 2u ? int3(0, 1, 0) : int3(0, 0, 1);
    [unroll]
    for (uint neighborIndex = 0u; neighborIndex < 4u; ++neighborIndex)
    {
        int signValue = (neighborIndex & 1u) != 0u ? 1 : -1;
        int3 offset = neighborIndex < 2u
            ? tangentU * signValue
            : tangentV * signValue;
        float3 neighborRadiance;
        uint neighborDiagnostics;
        if (!loadGlassVoxelCacheRadiance(
                glassCell + offset, faceSignature,
                neighborRadiance, neighborDiagnostics))
            continue;
        uint centerRasterHit = centerDiagnostics >> 31u;
        uint neighborRasterHit = neighborDiagnostics >> 31u;
        if (centerRasterHit != neighborRasterHit)
        {
            float3 difference = abs(neighborRadiance - centerRadiance);
            if (max(max(difference.x, difference.y), difference.z) >=
                GLASS_CACHE_VISIBLE_RADIANCE_DELTA)
            {
                return true;
            }
        }
    }
    return false;
}

float glassVoxelFootprintPixels(
    int3 glassCell, uint faceSignature, uint2 extent)
{
    uint axes = faceSignature & 0x07u;
    if (countbits(axes) != 1u) return 0.0;
    uint axis = (axes & 1u) != 0u ? 0u : ((axes & 2u) != 0u ? 1u : 2u);
    float3 tangentU =
        axis == 0u ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0);
    float3 tangentV =
        axis == 2u ? float3(0.0, 1.0, 0.0) : float3(0.0, 0.0, 1.0);
    float3 axisNormal = float3(0.0);
    axisNormal[axis] = (faceSignature & 0x08u) != 0u ? 1.0 : -1.0;
    float3 faceCenter = (float3(glassCell) + 0.5) / 256.0 +
        axisNormal * GLASS_VOXEL_FACE_HALF_EXTENT;
    float2 centerUv;
    float2 centerPixel;
    float centerDepth;
    float2 uUv;
    float2 uPixel;
    float uDepth;
    float2 vUv;
    float2 vPixel;
    float vDepth;
    if (!projectGlassWorldToScreen(
            faceCenter, extent, centerUv, centerPixel, centerDepth) ||
        !projectGlassWorldToScreen(
            faceCenter + tangentU / 256.0,
            extent, uUv, uPixel, uDepth) ||
        !projectGlassWorldToScreen(
            faceCenter + tangentV / 256.0,
            extent, vUv, vPixel, vDepth))
    {
        return 0.0;
    }
    return max(
        length(uPixel - centerPixel), length(vPixel - centerPixel));
}

void classifyGlassVoxelCache(uint activeIndex, uint2 extent)
{
    if (activeIndex >= min(
            glass_voxel_cache_active_count[0], GLASS_VOXEL_CACHE_CAPACITY)) return;
    uint slot = glass_voxel_cache_active_slots[activeIndex];
    uint4 metadata = glass_voxel_cache_metadata[slot];
    if (metadata.x == 0u || (metadata.w & (1u << 31u)) == 0u) return;
    uint faceSignature = metadata.y & GLASS_CACHE_FACE_SIGNATURE_MASK;
    int3 glassCell = glassVoxelCellFromKey(metadata.x);
    float footprintPixels = glassVoxelFootprintPixels(
        glassCell, faceSignature, extent);
    uint classified = metadata.y;
    // Screen-footprint LOD keeps distant/subpixel voxels on the one-color cache path.
    // Close voxels need exact primary transmission everywhere; intermediate voxels
    // pay that cost only where cached transport crosses a visible raster boundary.
    if (footprintPixels >= GLASS_CACHE_FULL_EXACT_FOOTPRINT_PIXELS)
    {
        classified |= GLASS_CACHE_EXACT_PRIMARY_BIT;
    }
    else if (footprintPixels >= 1.0)
    {
        float3 centerRadiance = glass_voxel_cache_radiance[slot * 2u].rgb;
        if (glassVoxelTransportIsDiscontinuous(
                glassCell, faceSignature, metadata.z, centerRadiance))
            classified |= GLASS_CACHE_BOUNDARY_EXACT_BIT;
    }
    glass_voxel_cache_metadata[slot].y = classified;
}

bool glassPixelNeedsExactRasterBoundary(
    int2 coordinate, uint2 extent, uint cachedDiagnostics)
{
    bool cachedRasterHit = (cachedDiagnostics >> 31u) != 0u;
    bool localRaster =
        opaque_provenance_tex[coordinate] == OPAQUE_PROVENANCE_RASTER;
    if (localRaster == cachedRasterHit) return false;

    int radius = GLASS_RASTER_BOUNDARY_RADIUS_PIXELS;
    const int2 directions[8] = {
        int2(-1, 0), int2(1, 0), int2(0, -1), int2(0, 1),
        int2(-1, -1), int2(1, -1), int2(-1, 1), int2(1, 1)
    };
    [unroll]
    for (uint index = 0u; index < 8u; ++index)
    {
        int2 neighbor = coordinate + directions[index] * radius;
        if (any(neighbor < int2(0)) || any(neighbor >= int2(extent)))
            continue;
        bool neighborRaster =
            opaque_provenance_tex[neighbor] == OPAQUE_PROVENANCE_RASTER;
        if (neighborRaster != localRaster) return true;
    }
    return false;
}

void buildGlassExactPixelList(uint2 pixel, uint2 extent)
{
    int2 coordinate = int2(pixel);
    uint frontData = glass_front_data_tex[coordinate];
    if ((frontData & 1u) == 0u) return;
    float frontDepth = glass_front_depth_tex[coordinate];
    float opaqueDepth = unified_opaque_depth_tex[coordinate];
    if (glassDepthPrecedes(opaqueDepth, frontDepth)) return;

    float2 screenUv = (float2(pixel) + 0.5) / float2(extent);
    Ray ray = generateCameraRay(screenUv, camera_info.view_proj_mat_inv);
    SceneQueryEvent frontEvent = unpackGlassFrontEvent(
        frontData, screenUv, frontDepth, ray);
    uint key = glassVoxelCellKey(glassInterfaceVoxelCell(frontEvent));
    uint slot;
    bool ignoredInserted;
    if (!glassVoxelCacheSlot(key, false, slot, ignoredInserted)) return;
    uint4 metadata = glass_voxel_cache_metadata[slot];
    bool fullExact =
        (metadata.y & GLASS_CACHE_EXACT_PRIMARY_BIT) != 0u;
    bool boundaryExact =
        (metadata.y & GLASS_CACHE_BOUNDARY_EXACT_BIT) != 0u;
    if ((metadata.w & (1u << 31u)) == 0u ||
        (!fullExact &&
         (!boundaryExact ||
          !glassPixelNeedsExactRasterBoundary(
              coordinate, extent, metadata.z))))
    {
        return;
    }

    uint activeIndex;
    InterlockedAdd(glass_voxel_cache_active_count[0], 1u, activeIndex);
    if (activeIndex < GLASS_VOXEL_CACHE_CAPACITY)
        glass_voxel_cache_active_slots[activeIndex] =
            pixel.y * extent.x + pixel.x;
}

void resolveGlassVoxelPixel(uint2 pixel, uint2 extent, bool forceExact)
{
    int2 coordinate = int2(pixel);
    float2 screenUv = (float2(pixel) + 0.5) / float2(extent);
    float3 opaqueColor = unified_opaque_hdr_tex.Load(int3(coordinate, 0)).rgb;
    uint frontData = glass_front_data_tex[coordinate];
    float frontDepth = glass_front_depth_tex[coordinate];
    float opaqueDepth = unified_opaque_depth_tex[coordinate];
    uint provenance = opaque_provenance_tex[coordinate];

    uint reason = GLASS_FALLBACK_NONE;
    GlassResolveResult result = emptyGlassResolveResult();
    float3 resolvedColor = opaqueColor;
    uint2 diagnostics = uint2(0u);
    bool usedCachedDiagnostics = false;
    if ((frontData & 1u) != 0u)
    {
        if (glassDepthPrecedes(opaqueDepth, frontDepth))
        {
            reason = GLASS_FALLBACK_FOREGROUND;
            result.fallback_reason = reason;
        }
        else
        {
            Ray ray = generateCameraRay(screenUv, camera_info.view_proj_mat_inv);
            SceneQueryEvent frontEvent = unpackGlassFrontEvent(
                frontData, screenUv, frontDepth, ray);
            uint key = glassVoxelCellKey(glassInterfaceVoxelCell(frontEvent));
            uint slot;
            bool ignoredInserted;
            if (glassVoxelCacheSlot(key, false, slot, ignoredInserted))
            {
                uint2 cachedDiagnostics = glass_voxel_cache_metadata[slot].zw;
                if ((cachedDiagnostics.y & (1u << 31u)) != 0u)
                {
                    float3 cachedRadiance =
                        glass_voxel_cache_radiance[slot * 2u].rgb;
                    resolvedColor = cachedRadiance;
                    diagnostics = uint2(
                        cachedDiagnostics.x,
                        cachedDiagnostics.y & ~(1u << 31u));
                    reason = diagnostics.x & 0x0fu;
                    usedCachedDiagnostics = true;
                    if (forceExact)
                    {
                        float3 cachedPrimaryScreen =
                            glass_voxel_cache_radiance[slot * 2u + 1u].rgb;
                        float3 exactPrimaryScreen =
                            traceGlassPrimaryScreenContributionFromFrontEvent(
                                ray, screenUv, frontEvent);
                        resolvedColor = max(
                            float3(0.0),
                            cachedRadiance - cachedPrimaryScreen +
                                exactPrimaryScreen);
                    }
                }
                else
                {
                    // A missing ready marker is fail-correct: retain the old exact per-pixel
                    // transport instead of consuming stale radiance from a prior frame.
                    result = traceGlassRadianceFromFrontEvent(
                        ray, screenUv, frontDepth, frontEvent);
                    resolvedColor = result.radiance;
                    reason = result.fallback_reason;
                }
            }
            else
            {
                result = traceGlassRadianceFromFrontEvent(
                    ray, screenUv, frontDepth, frontEvent);
                resolvedColor = result.radiance;
                reason = result.fallback_reason;
            }
        }
    }

    if (!usedCachedDiagnostics)
        diagnostics = packGlassDiagnostics(result, resolvedColor);

    if (shading_info.glass_debug_view == 1u)
        resolvedColor = glassFrontDebugColor(frontData);
    else if (shading_info.glass_debug_view == 2u)
        resolvedColor = provenanceDebugColor(provenance);
    else if (shading_info.glass_debug_view == 3u)
        resolvedColor = screenValidityDebugColor(reason);
    else
        resolvedColor = applyGlassCameraEffects(resolvedColor, screenUv);
    glass_debug_tex[coordinate] = diagnostics;
    composited_tex[coordinate] = float4(resolvedColor, 1.0);
}

void resolveGlassExactPixel(uint activeIndex, uint2 extent)
{
    if (activeIndex >= min(
            glass_voxel_cache_active_count[0], GLASS_VOXEL_CACHE_CAPACITY)) return;
    uint flattenedPixel = glass_voxel_cache_active_slots[activeIndex];
    uint2 pixel = uint2(
        flattenedPixel % extent.x,
        flattenedPixel / extent.x);
    if (any(pixel >= extent)) return;
    resolveGlassVoxelPixel(pixel, extent, true);
}

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    if (glass_voxel_cache_push.pass == GLASS_VOXEL_CACHE_SHADE_PASS)
    {
        shadeGlassVoxelCache(dispatchThreadId.x);
        return;
    }
    uint width;
    uint height;
    composited_tex.GetDimensions(width, height);
    uint2 extent = uint2(width, height);
    if (glass_voxel_cache_push.pass == GLASS_VOXEL_CACHE_CLASSIFY_PASS)
    {
        classifyGlassVoxelCache(dispatchThreadId.x, extent);
        return;
    }
    if (glass_voxel_cache_push.pass == GLASS_EXACT_PIXEL_RESOLVE_PASS)
    {
        resolveGlassExactPixel(dispatchThreadId.x, extent);
        return;
    }
    if (any(dispatchThreadId.xy >= extent)) return;
    if (glass_voxel_cache_push.pass == GLASS_VOXEL_CACHE_BUILD_PASS)
    {
        buildGlassVoxelCache(dispatchThreadId.xy, extent);
        return;
    }
    if (glass_voxel_cache_push.pass == GLASS_EXACT_PIXEL_BUILD_PASS)
    {
        buildGlassExactPixelList(dispatchThreadId.xy, extent);
        return;
    }
    resolveGlassVoxelPixel(dispatchThreadId.xy, extent, false);
}
