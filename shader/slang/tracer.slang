import contree_node;
import ddgi_exact_sun_visibility;
import ddgi_query;
import environment_lighting;
import local_lighting;
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
[[vk::binding(42, 0)]] ConstantBuffer<U_LocalLightInfo> local_light_info;
[[vk::binding(43, 0)]] StructuredBuffer<U_LightGpu> local_lights;

[[vk::binding(0, 1)]] [[vk::image_format("r32ui")]]
RWTexture2D<uint> compute_output_tex;
[[vk::binding(1, 1)]] [[vk::image_format("r32f")]]
RWTexture2D<float> compute_depth_tex;

[[vk::binding(2, 1)]]
RWStructuredBuffer<float4> environment_irradiance_capture;

[[vk::binding(41, 0)]]
RWStructuredBuffer<float4> ddgi_spatial_weight_readback;

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
static const uint DDGI_DEBUG_SPATIAL_WEIGHT_CURRENT = 15u;
static const uint DDGI_DEBUG_SPATIAL_WEIGHT_NOMINAL = 16u;
static const uint DDGI_DEBUG_SPATIAL_WEIGHT_WRAP = 17u;
static const uint DDGI_DEBUG_SPATIAL_WEIGHT_NOMINAL_WRAP = 18u;
static const uint DDGI_DEBUG_SPATIAL_WEIGHT_READBACK = 19u;
static const uint DDGI_DEBUG_SPATIAL_WEIGHT_CURRENT_NO_SURFACE = 20u;
static const uint DDGI_DEBUG_SPATIAL_WEIGHT_NOMINAL_NO_SURFACE = 21u;
static const uint DDGI_DEBUG_MOMENT_SUPPORT = 22u;
static const uint DDGI_TERRAIN_HARD_ORIGIN_CENTER_FIXED = 1u;
static const uint DDGI_TERRAIN_HARD_ORIGIN_SURFACE_FIXED = 2u;
float3 terrainDdgiHardVisibilityOrigin(float3 voxelCenter,
                                       float3 surfacePosition,
                                       float3 normal)
{
    float3 normalDirection = normalize(normal);
    uint mode = shading_info.ddgi_terrain_hard_origin;
    if (mode == DDGI_TERRAIN_HARD_ORIGIN_CENTER_FIXED)
    {
        return terrainRayOriginFromPosition(
            voxelCenter, normalDirection,
            gui_input.terrain_ray_origin_offset_world);
    }
    if (mode == DDGI_TERRAIN_HARD_ORIGIN_SURFACE_FIXED)
    {
        return terrainRayOriginFromPosition(
            surfacePosition, normalDirection,
            gui_input.terrain_ray_origin_offset_world);
    }
    return terrainRayOriginFromPosition(
        surfacePosition, normalDirection,
        ddgiVisibilityBiasWorld(
            shading_info.environment_probe_visibility_bias_world));
}

float3 terrainShadowReceiverPosition(float3 voxelCenter, float3 normal)
{
    return terrainRayOriginAlongNormal(
        voxelCenter, normal, gui_input.terrain_ray_origin_offset_world);
}

float3 terrainShadowReceiverPositionFromSurface(float3 surfacePosition,
                                                float3 normal)
{
    return terrainRayOriginFromPosition(
        surfacePosition, normal, gui_input.terrain_ray_origin_offset_world);
}

float3 shadowRayColor(float3 terrainLeafReceiverPosition,
                      float3 cloudReceiverPosition,
                      float3 receiverNormal,
                      out DirectSunShadowTransmittances shadow)
{
    float sunAboveHorizon = step(0.0, sun_info.sun_dir.y);
    DirectSunShadowReceiver receiver = makeDirectSunShadowReceiver(
        float4(terrainLeafReceiverPosition, 1.0),
        float4(terrainLeafReceiverPosition, 1.0),
        float4(cloudReceiverPosition, 1.0),
        int3(0), DIRECT_SUN_SHADOW_SOURCE_ALL,
        DIRECT_TERRAIN_SHADOW_VSM,
        DIRECT_SUN_SHADOW_SOURCE_ALL,
        DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE);
    receiver = withReceiverPlane(receiver, receiverNormal);
    shadow = directSunShadowTransmittancesVsmLeafCloud(
        receiver, gui_input, shadow_camera_info.view_proj_mat,
        shadow_map_tex_for_vsm_ping, leaf_shadow_opacity_blended_tex,
        leaf_shadow_mask_tex, cloud_shadow_tex);
    return shadow.combined * sun_info.sun_color * sun_info.sun_luminance *
        sunAboveHorizon;
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

float3 directLighting(float3 albedo, float3 normal, float3 voxelCenter,
                      float3 surfacePosition,
                      uint localInvocationIndex,
                      out DirectSunShadowTransmittances shadow)
{
    float3 sunDirection = normalize(sun_info.sun_dir);
    float3 terrainLeafReceiverPosition = terrainShadowReceiverPosition(
        voxelCenter, normal);
    float3 cloudReceiverPosition = terrainShadowReceiverPositionFromSurface(
        surfacePosition, normal);
    float cosine = max(0.0, dot(sunDirection, normal));
    shadow.terrain = 1.0;
    shadow.leaf = 1.0;
    shadow.cloud = 1.0;
    shadow.combined = 1.0;
    float3 sunIrradiance = float3(0.0);
    if (cosine > 0.0)
    {
        sunIrradiance = shadowRayColor(
            terrainLeafReceiverPosition, cloudReceiverPosition, normal,
            shadow) * cosine;
    }
    float3 localIrradiance = evaluateVoxelVisibleLocalLightIrradiance(
        local_light_info, local_lights, contree_node_data, contree_leaf_data,
        scene_tex, surfacePosition, normal,
        gui_input.terrain_ray_origin_offset_world, localInvocationIndex);
    return (sunIrradiance + localIrradiance) * albedo;
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
    float3 sunIrradiance = float3(0.0);
    if (sun_info.sun_dir.y > 0.0)
    {
        Ray shadowRay;
        shadowRay.direction = sampleSunDisk(
            seed, sun_info.sun_size, normalize(sun_info.sun_dir));
        shadowRay.inverse_direction = 1.0 / shadowRay.direction;
        shadowRay.origin = terrainShadowReceiverPosition(
            receiver.center_position, receiver.normal);
        float cosine = max(0.0, dot(shadowRay.direction, receiver.normal));
        if (cosine > 0.0)
        {
            MarchingResult shadowHit = generalSceneMarching(shadowRay,
                                                             localInvocationIndex);
            float visibility = shadowHit.is_hit ? 0.0 : 1.0;
            sunIrradiance = sun_info.sun_color * sun_info.sun_luminance *
                cosine * visibility;
        }
    }
    float3 receiverPosition = terrainVoxelSurfacePositionAlongNormal(
        receiver.center_position, receiver.normal);
    float3 localIrradiance = evaluateVoxelVisibleLocalLightIrradiance(
        local_light_info, local_lights, contree_node_data, contree_leaf_data,
        scene_tex, receiverPosition, receiver.normal,
        gui_input.terrain_ray_origin_offset_world, localInvocationIndex);
    return sunIrradiance + localIrradiance;
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
        irradiance += throughput * voxelSurfaceEmission(
            voxel_colors, result.voxel_type);
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
    return voxelSurfaceEmission(voxel_colors, primary.voxel_type) +
        directLight + albedo * environmentIrradiance;
}

float depthFromWorldPosition(float3 worldPosition)
{
    float4 ndc = mul(camera_info.view_proj_mat, float4(worldPosition, 1.0));
    return ndc.z / ndc.w;
}

uint ddgiSpatialWeightReadbackReceiver(uint2 pixel)
{
    if (all(pixel == uint2(824u, 370u))) return 0u;
    if (all(pixel == uint2(824u, 376u))) return 1u;
    if (all(pixel == uint2(824u, 382u))) return 2u;
    if (all(pixel == uint2(824u, 395u))) return 3u;
    if (all(pixel == uint2(824u, 401u))) return 4u;
    if (all(pixel == uint2(824u, 407u))) return 5u;
    return DDGI_SPATIAL_WEIGHT_READBACK_PROBE_COUNT;
}

void writeDdgiSpatialWeightReadback(Ray ray, uint2 pixel,
                                    uint localInvocationIndex)
{
    uint receiverIndex = ddgiSpatialWeightReadbackReceiver(pixel);
    if (receiverIndex >= DDGI_SPATIAL_WEIGHT_READBACK_RECEIVER_COUNT) return;

    uint receiverBase = receiverIndex * DDGI_SPATIAL_WEIGHT_READBACK_STRIDE;
    [unroll]
    for (uint slot = 0u; slot < DDGI_SPATIAL_WEIGHT_READBACK_STRIDE; ++slot)
    {
        ddgi_spatial_weight_readback[receiverBase + slot] = float4(0.0);
    }

    MarchingResult result = generalSceneMarching(ray, localInvocationIndex);
    if (!result.is_hit) return;

    float3 ddgiReceiverPosition = terrainVoxelSurfacePositionAlongNormal(
        result.center_position, result.normal);
    float3 ddgiHardVisibilityOrigin = terrainDdgiHardVisibilityOrigin(
        result.center_position, ddgiReceiverPosition, result.normal);
    writeDdgiSpatialWeightDiagnostics(
        ddgi_spatial_weight_readback, receiverBase, shading_info,
        ddgiReceiverPosition, result.normal, ddgiHardVisibilityOrigin);
}

float3 ddgiTerrainDebugValue(float2 screenUv, float3 worldPosition,
                             float3 positionWeightWorldPosition,
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
    // Raw normalization inputs pair with weight-sum's contributor count.
    if (view == DDGI_DEBUG_MOMENT_SUPPORT)
        return float3(momentResult.accumulated_base_weight,
                      momentResult.accumulated_weight,
                      momentResult.dominant_probe_weight);
    if (view == DDGI_DEBUG_DOMINANT_PROBE)
        return ddgiDominantProbeDebugColor(momentResult);
    if (view == DDGI_DEBUG_UNOCCLUDED_IRRADIANCE)
    {
        DdgiQueryResult unoccludedResult =
            sampleDdgiUnoccludedTerrainReference(
                shading_info, worldPosition, surfaceNormal);
        return unoccludedResult.irradiance;
    }
    if (view == DDGI_DEBUG_EQUAL_WEIGHT_IRRADIANCE)
    {
        DdgiQueryResult equalWeightResult =
            sampleDdgiEqualWeightTerrainReference(
                shading_info, worldPosition, surfaceNormal);
        return equalWeightResult.irradiance;
    }
    if (view == DDGI_DEBUG_RAW_CAGE_IRRADIANCE)
        return sampleDdgiRawCageIrradiance(
            shading_info, worldPosition, surfaceNormal);
    if (view >= DDGI_DEBUG_SPATIAL_WEIGHT_CURRENT &&
        view <= DDGI_DEBUG_SPATIAL_WEIGHT_NOMINAL_WRAP)
    {
        uint spatialWeightMode =
            view - DDGI_DEBUG_SPATIAL_WEIGHT_CURRENT;
        DdgiQueryResult spatialWeightResult =
            sampleDdgiSpatialWeightTerrainReference(
                shading_info, worldPosition, surfaceNormal,
                hardVisibilityWorldPosition, spatialWeightMode);
        return spatialWeightResult.irradiance;
    }
    if (view == DDGI_DEBUG_SPATIAL_WEIGHT_READBACK)
        return momentResult.irradiance;
    if (view == DDGI_DEBUG_SPATIAL_WEIGHT_CURRENT_NO_SURFACE ||
        view == DDGI_DEBUG_SPATIAL_WEIGHT_NOMINAL_NO_SURFACE)
    {
        uint spatialWeightMode = view == DDGI_DEBUG_SPATIAL_WEIGHT_CURRENT_NO_SURFACE
            ? DDGI_SPATIAL_WEIGHT_RELOCATION_NO_SURFACE
            : DDGI_SPATIAL_WEIGHT_NOMINAL_NO_SURFACE;
        DdgiQueryResult noSurfaceResult =
            sampleDdgiSpatialWeightTerrainReference(
                shading_info, worldPosition, surfaceNormal,
                hardVisibilityWorldPosition, spatialWeightMode);
        return noSurfaceResult.irradiance;
    }
    if (view == DDGI_DEBUG_PROBE_STATE)
        return ddgiProbeStateDebugColor(shading_info, worldPosition);
    if (view == DDGI_DEBUG_RELOCATION)
        return ddgiRelocationDebugColor(shading_info, worldPosition);

    DdgiQueryResult exactResult = sampleDdgiExactTerrainReference(
        shading_info, worldPosition, positionWeightWorldPosition,
        surfaceNormal, hardVisibilityWorldPosition);
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
                   out float4 environmentCaptureWorld,
                   out float4 environmentCaptureReceiver,
                   out float4 environmentCaptureDirectSunShadows, Ray ray,
                   float2 screenUv, int3 seed, uint localInvocationIndex)
{
    terrainHit = 0.0;
    environmentCaptureIrradiance = float3(0.0);
    directLight = float3(0.0);
    environmentCaptureWorld = float4(0.0);
    environmentCaptureReceiver = float4(0.0, 0.0, 0.0, 1.0);
    environmentCaptureDirectSunShadows = float4(1.0);
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
    environmentCaptureReceiver = float4(result.center_position, -1.0);
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
        DirectSunShadowTransmittances shadow;
        directLight = directLighting(albedo, result.normal,
                                     result.center_position, result.position,
                                     localInvocationIndex,
                                     shadow);
        environmentCaptureReceiver.w = shadow.terrain;
        environmentCaptureDirectSunShadows = float4(
            shadow.terrain, shadow.leaf, shadow.cloud, shadow.combined);
    }
    // The moment-visibility receiver remains fixed for every pixel of the voxel.
    // Only the nominal-grid position basis follows the exact surface hit so a
    // smooth probe field is not quantized into one irradiance step per voxel.
    float3 ddgiReceiverPosition = terrainVoxelSurfacePositionAlongNormal(
        result.center_position, result.normal);
    DdgiQueryResult consumerResult = sampleDdgiTerrainSmoothEnvironment(
        shading_info, ddgiReceiverPosition, result.position,
        result.normal);
    environmentIrradiance = consumerResult.irradiance;
    environmentCaptureIrradiance = consumerResult.irradiance;
    if (shading_info.ddgi_debug_view != DDGI_DEBUG_FINAL)
    {
        float3 ddgiHardVisibilityOrigin = terrainDdgiHardVisibilityOrigin(
            result.center_position, ddgiReceiverPosition, result.normal);
        environmentIrradiance = ddgiTerrainDebugValue(
            screenUv, ddgiReceiverPosition, result.position, result.normal,
            ddgiHardVisibilityOrigin, consumerResult);
        environmentCaptureIrradiance = ddgiTerrainDebugValue(
            screenUv, ddgiReceiverPosition, result.position, result.normal,
            ddgiHardVisibilityOrigin, consumerResult);
    }
    if (shading_info.ddgi_debug_view != DDGI_DEBUG_FINAL)
    {
        color = environmentIrradiance;
        return;
    }
    color = environmentIrradiance * albedo;
    color += directLight;
    color += voxelSurfaceEmission(voxel_colors, result.voxel_type);
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
    float4 environmentCaptureReceiver;
    float4 environmentCaptureDirectSunShadows;
    float depth;
    getPixelColor(color, depth, environmentIrradiance,
                  environmentCaptureIrradiance, directLight, terrainHit,
                  environmentCaptureWorld, environmentCaptureReceiver,
                  environmentCaptureDirectSunShadows,
                  ray, screenUv, seed,
                  localInvocationIndex);

    if (shading_info.ddgi_debug_view == DDGI_DEBUG_SPATIAL_WEIGHT_READBACK)
    {
        writeDdgiSpatialWeightReadback(
            ray, dispatchThreadId.xy, localInvocationIndex);
    }

    if (shading_info.environment_irradiance_capture_enabled != 0u)
    {
        uint captureIndex = dispatchThreadId.x + width * dispatchThreadId.y;
        environment_irradiance_capture[captureIndex] = float4(
            environmentCaptureIrradiance, terrainHit);
        environment_irradiance_capture[captureIndex + width * height] =
            environmentCaptureWorld;
        environment_irradiance_capture[captureIndex + 2 * width * height] =
            float4(directLight, terrainHit);
        environment_irradiance_capture[captureIndex + 3 * width * height] =
            environmentCaptureReceiver;
        environment_irradiance_capture[captureIndex + 4 * width * height] =
            environmentCaptureDirectSunShadows;
    }

    compute_output_tex[dispatchThreadId.xy] = packRgbe(color);
    compute_depth_tex[dispatchThreadId.xy] = depth;
}
