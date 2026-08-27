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
[[vk::binding(16, 0)]] [[vk::image_format("r32ui")]]
RWTexture2D<uint> glass_debug_tex;

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

static const float GLASS_DEPTH_TOLERANCE = 0.003;

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
    uint3 cell = uint3(max(event.to_cell, int3(0)));
    uint hashId = (cell.x * 73856093u ^ cell.y * 19349663u ^
                   cell.z * 83492791u) & 3u;
    float3 albedo = voxelColorByTypeAndHash(
        voxel_colors, event.to_voxel_type, hashId);
    float cosine = max(0.0, dot(normalize(event.incident_face_normal),
                                normalize(sun_info.sun_dir)));
    float3 ambient = albedo * 0.18;
    float3 direct = albedo * sun_info.sun_color * sun_info.sun_luminance * cosine;
    return ambient + direct + voxelSurfaceEmission(voxel_colors, event.to_voxel_type);
}

float3 sampleGlassTerminalRadiance(
    SceneQueryEvent event, uint screenCandidate, float glassFrontDepth,
    out uint fallbackReason, out uint terminalProvenance)
{
    fallbackReason = GLASS_FALLBACK_VOXEL;
    terminalProvenance = 0u;
    if (screenCandidate == 0u) return glassVoxelFallback(event);

    float4 clip = mul(camera_info.view_proj_mat, float4(event.position, 1.0));
    if (clip.w <= 0.0)
    {
        fallbackReason = GLASS_FALLBACK_OFFSCREEN;
        return glassVoxelFallback(event);
    }
    float2 uv = clip.xy / clip.w * 0.5 + 0.5;
    if (any(uv <= float2(0.0)) || any(uv >= float2(1.0)))
    {
        fallbackReason = GLASS_FALLBACK_OFFSCREEN;
        return glassVoxelFallback(event);
    }

    uint width;
    uint height;
    unified_opaque_depth_tex.GetDimensions(width, height);
    int2 coordinate = int2(uv * float2(width, height));
    float sampledDepth = unified_opaque_depth_tex[coordinate];
    float hitDepth = clip.z / clip.w;
    if (sampledDepth <= glassFrontDepth + GLASS_DEPTH_TOLERANCE ||
        abs(sampledDepth - hitDepth) > GLASS_DEPTH_TOLERANCE)
    {
        fallbackReason = GLASS_FALLBACK_REJECTED_DEPTH;
        return glassVoxelFallback(event);
    }
    uint provenance = opaque_provenance_tex[coordinate];
    if (provenance != OPAQUE_PROVENANCE_VOXEL &&
        provenance != OPAQUE_PROVENANCE_RASTER)
    {
        fallbackReason = GLASS_FALLBACK_REJECTED_PROVENANCE;
        return glassVoxelFallback(event);
    }
    fallbackReason = GLASS_FALLBACK_SCREEN_HIT;
    terminalProvenance = provenance;
    return unified_opaque_hdr_tex.SampleLevel(uv, 0.0).rgb;
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

GlassResolveResult traceGlassRadiance(Ray cameraRay, float glassFrontDepth)
{
    GlassResolveResult result = emptyGlassResolveResult();
    GlassActivePath paths[SCENE_QUERY_MAX_ACTIVE_PATHS];
    [unroll]
    for (uint index = 0u; index < SCENE_QUERY_MAX_ACTIVE_PATHS; ++index)
        paths[index] = emptyGlassPath();
    paths[0].origin = cameraRay.origin;
    paths[0].direction = normalize(cameraRay.direction);
    paths[0].throughput = float3(1.0);
    paths[0].active = 1u;
    paths[0].screen_candidate = 1u;
    result.peak_active_paths = 1u;
    uint nextSerial = 1u;

    for (uint queryIndex = 0u; queryIndex < SCENE_QUERY_MAX_SCENE_QUERIES; ++queryIndex)
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

        if (event.kind == SCENE_QUERY_EVENT_OPAQUE)
        {
            uint fallbackReason;
            uint terminalProvenance;
            float3 terminal = sampleGlassTerminalRadiance(
                event, path.screen_candidate, glassFrontDepth,
                fallbackReason, terminalProvenance);
            result.radiance += path.throughput * terminal;
            if (terminalProvenance == OPAQUE_PROVENANCE_RASTER)
                result.raster_screen_hits += 1u;
            if (fallbackReason == GLASS_FALLBACK_SCREEN_HIT ||
                result.fallback_reason == GLASS_FALLBACK_NONE)
                result.fallback_reason = fallbackReason;
            continue;
        }
        if (event.kind == SCENE_QUERY_EVENT_MISS)
        {
            result.radiance += path.throughput * glassSky(path.direction);
            if (result.fallback_reason == GLASS_FALLBACK_NONE)
                result.fallback_reason = GLASS_FALLBACK_SKY;
            continue;
        }
        if (event.kind != SCENE_QUERY_EVENT_INTERFACE)
        {
            result.exhausted = 1u;
            result.fallback_reason = GLASS_FALLBACK_EXHAUSTED;
            continue;
        }

        result.interfaces += 1u;
        if (result.interfaces > SCENE_QUERY_MAX_INTERFACE_EVENTS)
        {
            result.exhausted = 1u;
            result.fallback_reason = GLASS_FALLBACK_EXHAUSTED;
            continue;
        }
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

    // The fixed query cap has a defined, world-safe residual fallback. True DDA/interface
    // failures remain exhaustion errors; reaching this cap does not produce an undefined pixel.
    uint budgetFallbacks = 0u;
    [unroll]
    for (uint index = 0u; index < SCENE_QUERY_MAX_ACTIVE_PATHS; ++index)
    {
        if (paths[index].active == 0u) continue;
        result.radiance += paths[index].throughput * glassSky(paths[index].direction);
        budgetFallbacks += 1u;
    }
    if (budgetFallbacks != 0u)
        result.fallback_reason = GLASS_FALLBACK_QUERY_BUDGET;
    if (result.exhausted != 0u) result.fallback_reason = GLASS_FALLBACK_EXHAUSTED;
    return result;
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

[shader("compute")]
[numthreads(8, 8, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    composited_tex.GetDimensions(width, height);
    if (any(dispatchThreadId.xy >= uint2(width, height))) return;
    int2 coordinate = int2(dispatchThreadId.xy);
    float2 screenUv = (float2(dispatchThreadId.xy) + 0.5) /
                      float2(width, height);
    float3 opaqueColor = unified_opaque_hdr_tex.Load(int3(coordinate, 0)).rgb;
    uint frontData = glass_front_data_tex[coordinate];
    float frontDepth = glass_front_depth_tex[coordinate];
    float opaqueDepth = unified_opaque_depth_tex[coordinate];
    uint provenance = opaque_provenance_tex[coordinate];

    uint reason = GLASS_FALLBACK_NONE;
    GlassResolveResult result = emptyGlassResolveResult();
    float3 resolvedColor = opaqueColor;
    if ((frontData & 1u) != 0u)
    {
        if (opaqueDepth + GLASS_DEPTH_TOLERANCE < frontDepth)
        {
            reason = GLASS_FALLBACK_FOREGROUND;
        }
        else
        {
            Ray ray = generateCameraRay(screenUv, camera_info.view_proj_mat_inv);
            result = traceGlassRadiance(ray, frontDepth);
            resolvedColor = result.radiance;
            reason = result.fallback_reason;
        }
    }

    uint packedDebug = (reason & 0x0fu) |
        ((min(result.interfaces, 0xffu) & 0xffu) << 4u) |
        ((min(result.scene_queries, 0x0fu) & 0x0fu) << 12u) |
        ((result.exhausted & 1u) << 16u) |
        ((min(result.dda_steps, 0x3fffu) & 0x3fffu) << 17u) |
        ((result.raster_screen_hits != 0u ? 1u : 0u) << 31u);
    glass_debug_tex[coordinate] = packedDebug;

    if (shading_info.glass_debug_view == 1u)
        resolvedColor = glassFrontDebugColor(frontData);
    else if (shading_info.glass_debug_view == 2u)
        resolvedColor = provenanceDebugColor(provenance);
    else if (shading_info.glass_debug_view == 3u)
        resolvedColor = screenValidityDebugColor(reason);
    else
        resolvedColor = applyGlassCameraEffects(resolvedColor, screenUv);
    composited_tex[coordinate] = float4(resolvedColor, 1.0);
}
