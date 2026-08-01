import aabb;
import contree_node;
import ddgi_query;
import environment_lighting;
import marching_result;
import packer;
import ray;
import scene_marching;
import terrain_edit_preview;
import tracer_material;
import tracer_shadowing;
import tracer_types;
import voxel_data;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2, 0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(5, 0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(7, 0)]] StructuredBuffer<ContreeNode> contree_node_data;
[[vk::binding(8, 0)]] StructuredBuffer<uint> contree_leaf_data;
[[vk::binding(9, 0)]] [[vk::image_format("rg32ui")]]
readonly RWTexture3D<uint2> scene_tex;
[[vk::binding(11, 0)]] ConstantBuffer<U_VoxelColors> voxel_colors;
[[vk::binding(12, 0)]] Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(13, 0)]] Sampler2D leaf_shadow_mask_tex;
[[vk::binding(14, 0)]] Sampler2D cloud_shadow_tex;
[[vk::binding(15, 0)]] ConstantBuffer<U_TerrainEditPreview> terrain_edit_preview;
[[vk::binding(16, 0)]] [[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;
[[vk::binding(17, 0)]] Sampler2D shadow_map_tex_for_vsm_ping;

[[vk::binding(0, 1)]] [[vk::image_format("r32ui")]]
RWTexture2D<uint> compute_output_tex;
[[vk::binding(1, 1)]] [[vk::image_format("r32f")]]
RWTexture2D<float> compute_depth_tex;

[[vk::binding(2, 1)]]
RWStructuredBuffer<float4> environment_irradiance_capture;

MarchingResult generalSceneMarching(Ray ray, uint localInvocationIndex)
{
    return marchScene(contree_node_data, contree_leaf_data, scene_tex,
                      ray.origin, ray.direction, ray.inverse_direction,
                      localInvocationIndex);
}

static const float TERRAIN_VOXEL_SIZE = 1.0 / 256.0;
static const float TERRAIN_SHADOW_NORMAL_BIAS_VOXELS = 0.25;
// A binary voxel surface can sit almost one voxel in front of its smooth
// tangent plane. Keep that quantization thickness, not a world-sized depth
// epsilon, while preserving most of a four-voxel apple's contact depth.
static const float TERRAIN_SHADOW_LIGHT_BIAS_VOXELS = 0.75;
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

float3 voxelSurfacePositionAlongNormal(float3 voxelCenter, float3 normal)
{
    float3 normalDirection = normalize(normal);
    float2 distance = intersectSlabs(float3(-1.0), float3(1.0),
                                     float3(0.0), 1.0 / normalDirection);
    const float halfVoxelSize = 1.0 / 512.0;
    return voxelCenter + normalDirection * (distance.y * halfVoxelSize);
}

float3 terrainShadowReceiverPosition(float3 voxelCenter, float3 normal,
                                     float3 lightDirection)
{
    float3 normalDirection = normalize(normal);
    float3 lightDirectionNormalized = normalize(lightDirection);
    float3 stableSurfacePosition = voxelSurfacePositionAlongNormal(
        voxelCenter, normalDirection);
    return stableSurfacePosition +
        normalDirection * (TERRAIN_SHADOW_NORMAL_BIAS_VOXELS * TERRAIN_VOXEL_SIZE) +
        lightDirectionNormalized * (TERRAIN_SHADOW_LIGHT_BIAS_VOXELS * TERRAIN_VOXEL_SIZE);
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

float3 directLighting(float3 albedo, float3 normal, float3 voxelCenter)
{
    Ray shadowRay;
    shadowRay.direction = normalize(sun_info.sun_dir);
    shadowRay.inverse_direction = 1.0 / shadowRay.direction;
    shadowRay.origin = terrainShadowReceiverPosition(
        voxelCenter, normal, shadowRay.direction);
    float cosine = dot(shadowRay.direction, normal);
    if (cosine <= 0.0) return float3(0.0);
    return shadowRayColor(shadowRay, normal) * cosine * albedo;
}

float depthFromWorldPosition(float3 worldPosition)
{
    float4 ndc = mul(camera_info.view_proj_mat, float4(worldPosition, 1.0));
    return ndc.z / ndc.w;
}

float ddgiExactSegmentVisibility(float3 worldPosition,
                                 float3 surfaceOutward,
                                 DdgiProbeContribution contribution,
                                 uint localInvocationIndex)
{
    // Move one quarter voxel along the primary visibility direction to avoid
    // a self hit. Keep the exact oracle independent of the shading normal and
    // of the surface-to-probe direction whose visibility it is measuring.
    float3 rayOrigin = worldPosition + normalize(surfaceOutward) * (0.25 / 256.0);
    float3 segment = contribution.actual_position - rayOrigin;
    float segmentLength = length(segment);
    if (segmentLength <= 1.0e-6) return 1.0;
    float3 direction = segment / segmentLength;
    MarchingResult obstruction = marchScene(
        contree_node_data, contree_leaf_data, scene_tex,
        rayOrigin, direction, 1.0 / direction, localInvocationIndex);
    return (!obstruction.is_hit || obstruction.distance >= segmentLength - 1.0e-5)
        ? 1.0
        : 0.0;
}

DdgiQueryResult sampleDdgiExactTerrainReference(
    float3 worldPosition, float3 surfaceNormal, float3 surfaceOutward,
    uint localInvocationIndex)
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
                    getDdgiProbeContribution(
                        shading_info, worldPosition, normal, uint3(x, y, z));
                float visibility = contribution.trustworthy
                    ? ddgiExactSegmentVisibility(
                          worldPosition, surfaceOutward, contribution,
                          localInvocationIndex)
                    : 0.0;
                accumulateDdgiContribution(result, contribution, visibility);
            }
        }
    }
    finalizeDdgiQueryResult(result);
    return result;
}

float3 ddgiTerrainDebugValue(float2 screenUv, float3 worldPosition,
                             float3 surfaceNormal,
                             float3 surfaceOutward,
                             DdgiQueryResult momentResult,
                             uint localInvocationIndex)
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
    if (view == DDGI_DEBUG_PROBE_STATE)
        return ddgiProbeStateDebugColor(shading_info, worldPosition);
    if (view == DDGI_DEBUG_RELOCATION)
        return ddgiRelocationDebugColor(shading_info, worldPosition);

    DdgiQueryResult exactResult = sampleDdgiExactTerrainReference(
        worldPosition, surfaceNormal, surfaceOutward, localInvocationIndex);
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
                   out float3 environmentIrradiance, Ray ray,
                   float2 screenUv, uint localInvocationIndex)
{
    if (shading_info.ddgi_debug_view == DDGI_DEBUG_IRRADIANCE_ATLAS)
    {
        environmentIrradiance = sampleDdgiIrradianceAtlasDebug(screenUv);
        color = environmentIrradiance;
        depth = 0.0;
        return;
    }
    if (shading_info.ddgi_debug_view == DDGI_DEBUG_VISIBILITY_ATLAS)
    {
        environmentIrradiance = sampleDdgiVisibilityAtlasDebug(
            shading_info, screenUv);
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

    depth = depthFromWorldPosition(result.position);

    float3 albedo;
    parseTraceResult(albedo, result);
    environmentIrradiance = sampleDiffuseEnvironment(
        shading_info, result.position, result.normal);
    if (shading_info.ddgi_debug_view != DDGI_DEBUG_FINAL)
    {
        DdgiQueryResult ddgiResult = sampleDdgiDiffuseEnvironment(
            shading_info, result.position, result.normal);
        environmentIrradiance = ddgiTerrainDebugValue(
            screenUv, result.position, result.normal, -ray.direction, ddgiResult,
            localInvocationIndex);
    }
    if (shading_info.ddgi_debug_view != DDGI_DEBUG_FINAL)
    {
        color = environmentIrradiance;
        return;
    }
    color = environmentIrradiance * albedo;
    color += directLighting(albedo, result.normal, result.center_position);
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

    float2 screenUv = (float2(dispatchThreadId.xy) + 0.5) / float2(width, height);
    Ray ray = generateCameraRay(screenUv, camera_info.view_proj_mat_inv);

    float3 color;
    float3 environmentIrradiance;
    float depth;
    getPixelColor(color, depth, environmentIrradiance, ray, screenUv,
                  localInvocationIndex);

    if (shading_info.environment_irradiance_capture_enabled != 0u)
    {
        uint captureIndex = dispatchThreadId.x + width * dispatchThreadId.y;
        environment_irradiance_capture[captureIndex] = float4(
            environmentIrradiance, depth < 1.0 ? 1.0 : 0.0);
    }

    compute_output_tex[dispatchThreadId.xy] = packRgbe(color);
    compute_depth_tex[dispatchThreadId.xy] = depth;
}
