import contree_node;
import ddgi_exact_sun_visibility;
import ddgi_query;
import environment_lighting;
import marching_result;
import noise_tex;
import packer;
import ray;
import scene_marching;
import skylight;
import terrain_edit_preview;
import terrain_ray_origin;
import tracer_material;
import tracer_shadowing;
import tracer_types;
import transform;
import voxel_data;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2, 0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(5, 0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(6, 0)]] ConstantBuffer<U_EnvInfo> env_info;
[[vk::binding(7, 0)]] StructuredBuffer<ContreeNode> contree_node_data;
[[vk::binding(8, 0)]] StructuredBuffer<uint> contree_leaf_data;
[[vk::binding(9, 0)]] [[vk::image_format("rg32ui")]]
readonly RWTexture3D<uint2> scene_tex;
[[vk::binding(10, 0)]] Sampler2D shadow_map_tex;
[[vk::binding(11, 0)]] ConstantBuffer<U_VoxelColors> voxel_colors;
[[vk::binding(12, 0)]] Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(13, 0)]] Sampler2D leaf_shadow_mask_tex;
[[vk::binding(14, 0)]] Sampler2D cloud_shadow_tex;
[[vk::binding(15, 0)]] ConstantBuffer<U_TerrainEditPreview> terrain_edit_preview;
[[vk::binding(16, 0)]] [[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;
[[vk::binding(17, 0)]] Sampler2D shadow_map_tex_for_vsm_ping;

struct TerrainDdgiCacheMetadata
{
    uint key;
    uint revision;
    uint ready_frame;
    uint reserved;
};

[[vk::binding(39, 0)]]
RWStructuredBuffer<TerrainDdgiCacheMetadata> terrain_ddgi_cache_metadata;
[[vk::binding(40, 0)]]
RWStructuredBuffer<float4> terrain_ddgi_cache_irradiance;

[[vk::binding(0, 1)]] [[vk::image_format("r32ui")]]
RWTexture2D<uint> compute_output_tex;
[[vk::binding(1, 1)]] [[vk::image_format("r32f")]]
RWTexture2D<float> compute_depth_tex;

[[vk::binding(2, 1)]]
RWStructuredBuffer<float4> environment_irradiance_capture;

[[vk::binding(0, 2)]] Sampler2DArray scalar_bn;
[[vk::binding(1, 2)]] Sampler2DArray unit_vec2_bn;
[[vk::binding(5, 2)]] Sampler2DArray fast_weighted_cosine_bn;

MarchingResult generalSceneMarching(Ray ray, uint localInvocationIndex)
{
    return marchScene(contree_node_data, contree_leaf_data, scene_tex,
                      ray.origin, ray.direction, ray.inverse_direction,
                      localInvocationIndex);
}

float3 sampleDiffuseBounce(int3 seed, float3 normal)
{
    float3 localDirection = randomWeightedCosine(fast_weighted_cosine_bn, seed);
    return mul(localDirection, makeTbn(normal));
}

float3 sampleSunDisk(int3 seed, float sunSize, float3 sunDirection)
{
    float2 randomXy = randomWeightedCosine(fast_weighted_cosine_bn, seed).xy * sunSize;
    return normalize(sunDirection + mul(float3(randomXy, 0.0), makeTbn(sunDirection)));
}

static const uint DDGI_DEBUG_FINAL = 0u;
static const uint DDGI_DEBUG_MOMENT_VISIBILITY = 1u;
static const uint DDGI_DEBUG_EXACT_VISIBILITY = 2u;
static const uint DDGI_DEBUG_VISIBILITY_ERROR = 3u;
static const uint DDGI_DEBUG_EXACT_IRRADIANCE = 4u;
static const uint DDGI_DEBUG_IRRADIANCE_ERROR = 5u;
static const uint DDGI_DEBUG_WEIGHT_SUM = 6u;
static const uint DDGI_DEBUG_DOMINANT_PROBE = 7u;
static const uint DDGI_DEBUG_PROBE_STATE = 8u;
static const uint DDGI_DEBUG_RELOCATION = 9u;
static const uint DDGI_DEBUG_IRRADIANCE_ATLAS = 10u;
static const uint DDGI_DEBUG_VISIBILITY_ATLAS = 11u;
static const uint DDGI_DEBUG_UNOCCLUDED_IRRADIANCE = 12u;
static const uint DDGI_DEBUG_EQUAL_WEIGHT_IRRADIANCE = 13u;
static const uint DDGI_DEBUG_RAW_CAGE_IRRADIANCE = 14u;
static const uint DDGI_TERRAIN_HARD_ORIGIN_CENTER_FIXED = 1u;
static const uint DDGI_TERRAIN_HARD_ORIGIN_SURFACE_FIXED = 2u;
static const uint TERRAIN_DDGI_CACHE_CAPACITY = 1u << 20u;
static const uint TERRAIN_DDGI_CACHE_MASK = TERRAIN_DDGI_CACHE_CAPACITY - 1u;
static const uint TERRAIN_DDGI_CACHE_MAX_PROBES = 8u;
static const uint TERRAIN_DDGI_CACHE_COMPUTING_BIT = 0x80000000u;

float3 terrainDdgiHardVisibilityOrigin(float3 voxelCenter,
                                       float3 surfacePosition,
                                       float3 normal)
{
    float3 normalDirection = normalize(normal);
    uint mode = shading_info.ddgi_terrain_hard_origin;
    if (mode == DDGI_TERRAIN_HARD_ORIGIN_CENTER_FIXED)
    {
        return voxelCenter + normalDirection *
            gui_input.terrain_ray_origin_offset_world;
    }
    if (mode == DDGI_TERRAIN_HARD_ORIGIN_SURFACE_FIXED)
    {
        return surfacePosition +
            normalDirection * gui_input.terrain_ray_origin_offset_world;
    }
    float biasWorld = max(
        0.0, shading_info.environment_probe_visibility_bias_world);
    return surfacePosition + normalDirection * biasWorld;
}

float3 terrainShadowReceiverPosition(float3 voxelCenter, float3 normal)
{
    return terrainRayOriginAlongNormal(
        voxelCenter, normal, gui_input.terrain_ray_origin_offset_world);
}

float3 terrainShadowReceiverPositionFromSurface(float3 surfacePosition,
                                                float3 normal)
{
    return terrainRayOriginFromSurface(
        surfacePosition, normal, gui_input.terrain_ray_origin_offset_world);
}

uint terrainDdgiCacheKey(float3 voxelCenter)
{
    uint3 voxelCoordinate = uint3(floor(voxelCenter * 256.0));
    // Terrain stores one canonical smooth normal per voxel, so the voxel
    // coordinate fully identifies every input to the DDGI receiver query.
    uint packedKey = voxelCoordinate.x | (voxelCoordinate.y << 9u) |
                     (voxelCoordinate.z << 18u);
    return packedKey + 1u;
}

uint terrainDdgiCacheHash(uint key)
{
    uint hash = key;
    hash ^= hash >> 16u;
    hash *= 0x7feb352du;
    hash ^= hash >> 15u;
    hash *= 0x846ca68bu;
    hash ^= hash >> 16u;
    return hash;
}

bool terrainDdgiCacheSlot(uint key, out uint slot)
{
    uint firstSlot = terrainDdgiCacheHash(key) & TERRAIN_DDGI_CACHE_MASK;
    [unroll]
    for (uint probe = 0u; probe < TERRAIN_DDGI_CACHE_MAX_PROBES; ++probe)
    {
        slot = (firstSlot + probe) & TERRAIN_DDGI_CACHE_MASK;
        uint observedKey = terrain_ddgi_cache_metadata[slot].key;
        if (observedKey == key) return true;
        if (observedKey != 0u) continue;
        InterlockedCompareExchange(
            terrain_ddgi_cache_metadata[slot].key, 0u, key, observedKey);
        if (observedKey == 0u || observedKey == key) return true;
    }
    slot = 0u;
    return false;
}

float3 sampleTerrainDdgiEnvironmentCached(
    float3 voxelCenter, float3 worldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition)
{
    uint slot;
    uint key = terrainDdgiCacheKey(voxelCenter);
    if (!terrainDdgiCacheSlot(key, slot))
    {
        return sampleDdgiDiffuseEnvironment(
            shading_info, worldPosition, surfaceNormal,
            hardVisibilityWorldPosition).irradiance;
    }

    uint targetRevision = shading_info.ddgi_query_revision;
    TerrainDdgiCacheMetadata metadata = terrain_ddgi_cache_metadata[slot];
    if (metadata.revision == targetRevision &&
        metadata.ready_frame != env_info.frame_serial_idx)
    {
        return terrain_ddgi_cache_irradiance[slot].xyz;
    }

    uint observedRevision = metadata.revision;
    bool canPublish = (observedRevision & TERRAIN_DDGI_CACHE_COMPUTING_BIT) == 0u;
    if (canPublish)
    {
        InterlockedCompareExchange(
            terrain_ddgi_cache_metadata[slot].revision,
            observedRevision,
            targetRevision | TERRAIN_DDGI_CACHE_COMPUTING_BIT,
            observedRevision);
        canPublish = observedRevision == metadata.revision;
    }

    float3 irradiance = sampleDdgiDiffuseEnvironment(
        shading_info, worldPosition, surfaceNormal,
        hardVisibilityWorldPosition).irradiance;
    if (canPublish)
    {
        terrain_ddgi_cache_irradiance[slot] = float4(irradiance, 1.0);
        terrain_ddgi_cache_metadata[slot].ready_frame = env_info.frame_serial_idx;
        DeviceMemoryBarrier();
        InterlockedExchange(
            terrain_ddgi_cache_metadata[slot].revision, targetRevision);
    }
    return irradiance;
}

float3 shadowRayColor(Ray ray, float3 receiverNormal)
{
    float sunAboveHorizon = step(0.0, sun_info.sun_dir.y);
    DirectSunShadowReceiver receiver = makeDirectSunShadowReceiver(
        float4(ray.origin, 1.0), int3(0), DIRECT_SUN_SHADOW_SOURCE_ALL,
        DIRECT_TERRAIN_SHADOW_VSM,
        DIRECT_SUN_SHADOW_SOURCE_ALL,
        DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE);
    receiver = withReceiverPlane(receiver, receiverNormal);
    float shadow = directSunShadowTransmittanceVsmLeafCloud(
        receiver, gui_input, shadow_camera_info.view_proj_mat,
        shadow_map_tex_for_vsm_ping, leaf_shadow_opacity_blended_tex,
        leaf_shadow_mask_tex, cloud_shadow_tex);
    return shadow * sun_info.sun_color * sun_info.sun_luminance * sunAboveHorizon;
}

float3 nextTracingPosition(float3 voxelCenter, float3 voxelNormal)
{
    return terrainRayOriginAlongNormal(
        voxelCenter, voxelNormal, gui_input.terrain_ray_origin_offset_world);
}

uint terrainAtlasData(float3 centerPosition)
{
    int3 coordinate = int3(floor(centerPosition * 256.0));
    uint width;
    uint height;
    uint depth;
    chunk_atlas.GetDimensions(width, height, depth);
    int3 size = int3(width, height, depth);
    if (any(coordinate < int3(0)) || any(coordinate >= size)) return 0u;
    return chunk_atlas[uint3(coordinate)];
}

void parseTraceResult(out float3 albedo, MarchingResult result)
{
    albedo = voxelColorByTypeAndHash(voxel_colors, result.voxel_type, result.hash_id);
    uint atlasData = terrainAtlasData(result.center_position);
    albedo = applyTerrainMoisture(albedo, result.voxel_type,
                                  voxelMoistureFromAtlasData(atlasData));
    albedo = applyTerrainFertility(albedo, result.voxel_type,
                                   voxelFertilityFromAtlasData(atlasData),
                                   result.center_position);
}

float3 directLighting(float3 albedo, float3 normal, float3 surfacePosition)
{
    Ray shadowRay;
    shadowRay.direction = normalize(sun_info.sun_dir);
    shadowRay.inverse_direction = 1.0 / shadowRay.direction;
    shadowRay.origin = terrainShadowReceiverPositionFromSurface(
        surfacePosition, normal);
    float cosine = dot(shadowRay.direction, normal);
    if (cosine <= 0.0) return float3(0.0);
    return shadowRayColor(shadowRay, normal) * cosine * albedo;
}

static const uint PATH_TRACING_MAX_BOUNCES = 8u;

int3 pathTracingBounceSeed(int3 seed, uint bounceIndex, int2 offset)
{
    int bounce = int(bounceIndex) + 1;
    return int3((seed.xy + offset * bounce) % 128, seed.z);
}

float3 pathTracingDirectIrradiance(MarchingResult receiver, int3 seed,
                                   uint localInvocationIndex)
{
    if (sun_info.sun_dir.y <= 0.0) return float3(0.0);

    Ray shadowRay;
    shadowRay.direction = sampleSunDisk(
        seed, sun_info.sun_size, normalize(sun_info.sun_dir));
    shadowRay.inverse_direction = 1.0 / shadowRay.direction;
    shadowRay.origin = terrainShadowReceiverPosition(
        receiver.center_position, receiver.normal);
    float cosine = dot(shadowRay.direction, receiver.normal);
    if (cosine <= 0.0) return float3(0.0);

    MarchingResult shadowHit = generalSceneMarching(shadowRay, localInvocationIndex);
    float visibility = shadowHit.is_hit ? 0.0 : 1.0;
    return sun_info.sun_color * sun_info.sun_luminance * cosine * visibility;
}

float3 pathTracingIndirectIrradiance(MarchingResult primary, int3 seed,
                                     uint localInvocationIndex)
{
    float3 irradiance = float3(0.0);
    float3 throughput = float3(1.0);
    MarchingResult receiver = primary;
    uint maxBounces = min(gui_input.path_tracing_max_bounces,
                          PATH_TRACING_MAX_BOUNCES);

    [Loop]
    for (uint bounceIndex = 0u; bounceIndex < PATH_TRACING_MAX_BOUNCES;
         ++bounceIndex)
    {
        if (bounceIndex >= maxBounces) break;

        int3 indirectSeed = pathTracingBounceSeed(
            seed, bounceIndex, int2(13, 37));
        Ray indirectRay;
        indirectRay.direction = sampleDiffuseBounce(
            indirectSeed, normalize(receiver.normal));
        indirectRay.inverse_direction = 1.0 / indirectRay.direction;
        indirectRay.origin = nextTracingPosition(
            receiver.center_position, receiver.normal);

        MarchingResult result = generalSceneMarching(indirectRay,
                                                     localInvocationIndex);
        if (!result.is_hit)
        {
            irradiance += throughput * getAuthoredSkyRadiance(
                indirectRay.direction, normalize(sun_info.sun_dir));
            break;
        }
        if (dot(indirectRay.direction, result.normal) > 0.0) break;

        float3 sampleAlbedo;
        parseTraceResult(sampleAlbedo, result);
        throughput *= sampleAlbedo;
        int3 sunSeed = pathTracingBounceSeed(
            seed, bounceIndex, int2(47, 71));
        irradiance += throughput * pathTracingDirectIrradiance(
            result, sunSeed, localInvocationIndex);
        receiver = result;
    }
    return irradiance;
}

float3 pathTraceTerrainReference(MarchingResult primary, float3 albedo,
                                 int3 seed, uint localInvocationIndex,
                                 out float3 environmentIrradiance,
                                 out float3 directLight)
{
    int3 primarySunSeed = pathTracingBounceSeed(seed, 0u, int2(83, 101));
    directLight = albedo * pathTracingDirectIrradiance(
        primary, primarySunSeed, localInvocationIndex);
    environmentIrradiance = gui_input.path_tracing_ambient_light +
        pathTracingIndirectIrradiance(primary, seed, localInvocationIndex);
    return directLight + albedo * environmentIrradiance;
}

float depthFromWorldPosition(float3 worldPosition)
{
    float4 ndc = mul(camera_info.view_proj_mat, float4(worldPosition, 1.0));
    return ndc.z / ndc.w;
}

DdgiQueryResult sampleDdgiExactTerrainReference(
    float3 worldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition)
{
    DdgiQueryResult result = makeEmptyDdgiQueryResult();
    float3 normal = normalize(surfaceNormal);
    if (ddgiQueryIsTerrainInvalidated(shading_info, worldPosition)) return result;
    if (ddgiQueryUsesGlobalSky(shading_info, worldPosition))
    {
        result.irradiance = sampleDdgiGlobalSky(normal);
        result.mean_visibility = 1.0;
        return result;
    }
    [unroll]
    for (uint z = 0u; z < 2u; ++z)
    {
        [unroll]
        for (uint y = 0u; y < 2u; ++y)
        {
            [unroll]
            for (uint x = 0u; x < 2u; ++x)
            {
                DdgiProbeContribution contribution =
                    getDdgiFullVisibilityProbeContribution(
                        shading_info, worldPosition, normal,
                        hardVisibilityWorldPosition, uint3(x, y, z));
                float visibility = contribution.trustworthy
                    ? contribution.hard_visibility
                    : 0.0;
                accumulateDdgiContribution(result, contribution, visibility);
            }
        }
    }
    finalizeDdgiQueryResult(result);
    return result;
}

// Diagnostic-only reference: retain the normal probe validity, relocation-aware
// position weight, surface-side weight, and support-distance gates, but force
// both consumer visibility terms to one. This isolates probe transport/query
// irradiance from visibility attenuation without changing the final path.
DdgiQueryResult sampleDdgiUnoccludedTerrainReference(
    float3 worldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition)
{
    DdgiQueryResult result = makeEmptyDdgiQueryResult();
    float3 normal = normalize(surfaceNormal);
    if (ddgiQueryIsTerrainInvalidated(shading_info, worldPosition)) return result;
    if (ddgiQueryUsesGlobalSky(shading_info, worldPosition))
    {
        result.irradiance = sampleDdgiGlobalSky(normal);
        result.mean_visibility = 1.0;
        return result;
    }
    [unroll]
    for (uint z = 0u; z < 2u; ++z)
    {
        [unroll]
        for (uint y = 0u; y < 2u; ++y)
        {
            [unroll]
            for (uint x = 0u; x < 2u; ++x)
            {
                DdgiProbeContribution contribution =
                    getDdgiUnoccludedProbeContribution(
                        shading_info, worldPosition, normal,
                        hardVisibilityWorldPosition, uint3(x, y, z));
                accumulateDdgiContribution(result, contribution, 1.0);
            }
        }
    }
    finalizeDdgiQueryResult(result);
    return result;
}

DdgiQueryResult sampleDdgiEqualWeightTerrainReference(
    float3 worldPosition, float3 surfaceNormal,
    float3 hardVisibilityWorldPosition)
{
    DdgiQueryResult result = makeEmptyDdgiQueryResult();
    float3 normal = normalize(surfaceNormal);
    if (ddgiQueryIsTerrainInvalidated(shading_info, worldPosition)) return result;
    if (ddgiQueryUsesGlobalSky(shading_info, worldPosition))
    {
        result.irradiance = sampleDdgiGlobalSky(normal);
        result.mean_visibility = 1.0;
        return result;
    }
    [unroll]
    for (uint z = 0u; z < 2u; ++z)
    {
        [unroll]
        for (uint y = 0u; y < 2u; ++y)
        {
            [unroll]
            for (uint x = 0u; x < 2u; ++x)
            {
                DdgiProbeContribution contribution =
                    getDdgiUnoccludedProbeContribution(
                        shading_info, worldPosition, normal,
                        hardVisibilityWorldPosition, uint3(x, y, z));
                accumulateDdgiEqualWeightContribution(result, contribution);
            }
        }
    }
    finalizeDdgiQueryResult(result);
    return result;
}

float3 ddgiTerrainDebugValue(float2 screenUv, float3 worldPosition,
                             float3 surfaceNormal,
                             float3 hardVisibilityWorldPosition,
                             DdgiQueryResult momentResult)
{
    uint view = shading_info.ddgi_debug_view;
    if (view == DDGI_DEBUG_FINAL) return momentResult.irradiance;
    if (view == DDGI_DEBUG_MOMENT_VISIBILITY)
        return float3(momentResult.mean_visibility);
    if (view == DDGI_DEBUG_WEIGHT_SUM)
        return float3(saturate(momentResult.accumulated_base_weight),
                      saturate(momentResult.accumulated_weight),
                      float(momentResult.contributing_probe_count) / 8.0);
    if (view == DDGI_DEBUG_DOMINANT_PROBE)
        return ddgiDominantProbeDebugColor(momentResult);
    if (view == DDGI_DEBUG_UNOCCLUDED_IRRADIANCE)
    {
        DdgiQueryResult unoccludedResult =
            sampleDdgiUnoccludedTerrainReference(
                worldPosition, surfaceNormal, hardVisibilityWorldPosition);
        return unoccludedResult.irradiance;
    }
    if (view == DDGI_DEBUG_EQUAL_WEIGHT_IRRADIANCE)
    {
        DdgiQueryResult equalWeightResult =
            sampleDdgiEqualWeightTerrainReference(
                worldPosition, surfaceNormal, hardVisibilityWorldPosition);
        return equalWeightResult.irradiance;
    }
    if (view == DDGI_DEBUG_RAW_CAGE_IRRADIANCE)
        return sampleDdgiRawCageIrradiance(
            shading_info, worldPosition, surfaceNormal);
    if (view == DDGI_DEBUG_PROBE_STATE)
        return ddgiProbeStateDebugColor(shading_info, worldPosition);
    if (view == DDGI_DEBUG_RELOCATION)
        return ddgiRelocationDebugColor(shading_info, worldPosition);

    DdgiQueryResult exactResult = sampleDdgiExactTerrainReference(
        worldPosition, surfaceNormal, hardVisibilityWorldPosition);
    if (view == DDGI_DEBUG_EXACT_VISIBILITY)
        return float3(exactResult.mean_visibility);
    if (view == DDGI_DEBUG_VISIBILITY_ERROR)
        return float3(abs(momentResult.mean_visibility -
                          exactResult.mean_visibility));
    if (view == DDGI_DEBUG_EXACT_IRRADIANCE)
        return exactResult.irradiance;
    if (view == DDGI_DEBUG_IRRADIANCE_ERROR)
        return abs(momentResult.irradiance - exactResult.irradiance);
    return float3(0.0);
}

void getPixelColor(out float3 color, out float depth,
                   out float3 environmentIrradiance,
                   out float3 environmentCaptureIrradiance,
                   out float3 directLight,
                   out float terrainHit,
                   out float4 environmentCaptureWorld, Ray ray,
                   float2 screenUv, int3 seed, uint localInvocationIndex)
{
    terrainHit = 0.0;
    environmentCaptureIrradiance = float3(0.0);
    directLight = float3(0.0);
    environmentCaptureWorld = float4(0.0);
    if (shading_info.ddgi_debug_view == DDGI_DEBUG_IRRADIANCE_ATLAS)
    {
        environmentIrradiance = sampleDdgiIrradianceAtlasDebug(screenUv);
        environmentCaptureIrradiance = environmentIrradiance;
        color = environmentIrradiance;
        depth = 0.0;
        return;
    }
    if (shading_info.ddgi_debug_view == DDGI_DEBUG_VISIBILITY_ATLAS)
    {
        environmentIrradiance = sampleDdgiVisibilityAtlasDebug(
            shading_info, screenUv);
        environmentCaptureIrradiance = environmentIrradiance;
        color = environmentIrradiance;
        depth = 0.0;
        return;
    }

    MarchingResult result = generalSceneMarching(ray, localInvocationIndex);
    if (!result.is_hit)
    {
        depth = 1.0;
        color = float3(0.0);
        environmentIrradiance = float3(0.0);
        return;
    }

    terrainHit = 1.0;
    depth = depthFromWorldPosition(result.position);
    float exactDirectSunVisibility = 0.0;
    if (shading_info.environment_irradiance_capture_enabled != 0u)
    {
        float3 sunDirection = normalize(sun_info.sun_dir);
        exactDirectSunVisibility = ddgiExactTerrainSunVisibility(
            contree_node_data, contree_leaf_data, scene_tex,
            result, sunDirection, gui_input.terrain_ray_origin_offset_world,
            localInvocationIndex);
    }
    environmentCaptureWorld = float4(
        result.position, exactDirectSunVisibility);

    float3 albedo;
    parseTraceResult(albedo, result);
    // The path-tracing reference is terrain-only. Raster consumers continue
    // to sample DDGI, while this branch never queries the probe field.
    if (gui_input.path_tracing_reference != 0u &&
        shading_info.ddgi_debug_view == DDGI_DEBUG_FINAL)
    {
        color = pathTraceTerrainReference(
            result, albedo, seed, localInvocationIndex,
            environmentIrradiance, directLight);
        environmentCaptureIrradiance = environmentIrradiance;
        color = applyTerrainEditPreviewTint(terrain_edit_preview, color,
                                            result.center_position);
        return;
    }
    if (shading_info.ddgi_debug_view == DDGI_DEBUG_FINAL ||
        shading_info.environment_irradiance_capture_enabled != 0u)
    {
        directLight = directLighting(albedo, result.normal, result.position);
    }
    // DDGI is part of the flat voxel material contract. Every pixel that hits
    // this voxel must query from the same receiver, independent of the camera
    // ray's exact intersection point on the visible face.
    float3 ddgiReceiverPosition = terrainVoxelSurfacePositionAlongNormal(
        result.center_position, result.normal);
    float3 ddgiHardVisibilityOrigin = terrainDdgiHardVisibilityOrigin(
        result.center_position, ddgiReceiverPosition, result.normal);
    DdgiQueryResult consumerResult = makeEmptyDdgiQueryResult();
    bool canUseTerrainCache =
        shading_info.ddgi_debug_view == DDGI_DEBUG_FINAL &&
        shading_info.environment_irradiance_capture_enabled == 0u &&
        shading_info.environment_irradiance_capture_unpublished == 0u;
    if (canUseTerrainCache)
    {
        consumerResult.irradiance = sampleTerrainDdgiEnvironmentCached(
            result.center_position, ddgiReceiverPosition, result.normal,
            ddgiHardVisibilityOrigin);
    }
    else
    {
        consumerResult = sampleDdgiDiffuseEnvironment(
            shading_info, ddgiReceiverPosition, result.normal,
            ddgiHardVisibilityOrigin);
    }
    DdgiQueryResult captureResult = consumerResult;
    if (shading_info.environment_irradiance_capture_unpublished != 0u)
    {
        captureResult = sampleDdgiUnpublishedCaptureEnvironment(
            shading_info, ddgiReceiverPosition, result.normal,
            ddgiHardVisibilityOrigin);
    }
    environmentIrradiance = consumerResult.irradiance;
    environmentCaptureIrradiance = captureResult.irradiance;
    if (shading_info.ddgi_debug_view != DDGI_DEBUG_FINAL)
    {
        environmentIrradiance = ddgiTerrainDebugValue(
            screenUv, ddgiReceiverPosition, result.normal,
            ddgiHardVisibilityOrigin, consumerResult);
        environmentCaptureIrradiance = ddgiTerrainDebugValue(
            screenUv, ddgiReceiverPosition, result.normal,
            ddgiHardVisibilityOrigin, captureResult);
    }
    if (shading_info.ddgi_debug_view != DDGI_DEBUG_FINAL)
    {
        color = environmentIrradiance;
        return;
    }
    color = environmentIrradiance * albedo;
    color += directLight;
    color = applyTerrainEditPreviewTint(terrain_edit_preview, color,
                                        result.center_position);
}

[shader("compute")]
[numthreads(8, 8, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID,
          uint localInvocationIndex : SV_GroupIndex)
{
    uint width;
    uint height;
    compute_output_tex.GetDimensions(width, height);
    if (any(dispatchThreadId.xy >= uint2(width, height))) return;

    int3 seed = getBlueNoiseSeed(dispatchThreadId.xy, env_info.frame_serial_idx);
    float2 screenUv = (float2(dispatchThreadId.xy) + 0.5) / float2(width, height);
    Ray ray = generateCameraRay(screenUv, camera_info.view_proj_mat_inv);

    float3 color;
    float3 environmentIrradiance;
    float3 environmentCaptureIrradiance;
    float3 directLight;
    float terrainHit;
    float4 environmentCaptureWorld;
    float depth;
    getPixelColor(color, depth, environmentIrradiance,
                  environmentCaptureIrradiance, directLight, terrainHit,
                  environmentCaptureWorld, ray, screenUv, seed,
                  localInvocationIndex);

    if (shading_info.environment_irradiance_capture_enabled != 0u)
    {
        uint captureIndex = dispatchThreadId.x + width * dispatchThreadId.y;
        environment_irradiance_capture[captureIndex] = float4(
            environmentCaptureIrradiance, terrainHit);
        environment_irradiance_capture[captureIndex + width * height] =
            environmentCaptureWorld;
        environment_irradiance_capture[captureIndex + 2 * width * height] =
            float4(directLight, terrainHit);
    }

    compute_output_tex[dispatchThreadId.xy] = packRgbe(color);
    compute_depth_tex[dispatchThreadId.xy] = depth;
}
