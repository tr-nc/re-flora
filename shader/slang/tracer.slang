import aabb;
import contree_node;
import marching_result;
import noise_tex;
import packer;
import projection;
import ray;
import scene_marching;
import terrain_edit_preview;
import tracer_material;
import tracer_shadowing;
import tracer_types;
import transform;
import voxel_data;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2, 0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4, 0)]] ConstantBuffer<U_CameraInfoPrevFrame> camera_info_prev_frame;
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

[[vk::binding(0, 1)]] [[vk::image_format("r32ui")]]
RWTexture2D<uint> compute_output_tex;
[[vk::binding(1, 1)]] [[vk::image_format("r32f")]]
RWTexture2D<float> compute_depth_tex;

[[vk::binding(0, 2)]] Sampler2DArray scalar_bn;
[[vk::binding(1, 2)]] Sampler2DArray unit_vec2_bn;
[[vk::binding(2, 2)]] Sampler2DArray unit_vec3_bn;
[[vk::binding(3, 2)]] Sampler2DArray weighted_cosine_bn;
[[vk::binding(4, 2)]] Sampler2DArray fast_unit_vec3_bn;
[[vk::binding(5, 2)]] Sampler2DArray fast_weighted_cosine_bn;

[[vk::binding(0, 3)]] [[vk::image_format("r32ui")]]
RWTexture2D<uint> denoiser_normal_tex;
[[vk::binding(2, 3)]] [[vk::image_format("rgba32f")]]
RWTexture2D<float4> denoiser_position_tex;
[[vk::binding(4, 3)]] [[vk::image_format("r32ui")]]
RWTexture2D<uint> denoiser_vox_id_tex;
[[vk::binding(8, 3)]] [[vk::image_format("rg16f")]]
RWTexture2D<float2> denoiser_motion_tex;

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

float3 shadowRayColor(Ray ray, int3 seed)
{
    float sunAboveHorizon = step(0.0, sun_info.sun_dir.y);
    DirectSunShadowReceiver receiver = makeDirectSunShadowReceiver(
        float4(ray.origin, 1.0), seed, DIRECT_SUN_SHADOW_SOURCE_ALL,
        DIRECT_TERRAIN_SHADOW_GUI, DIRECT_SUN_SHADOW_SOURCE_ALL,
        DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE);
    float shadow = directSunShadowTransmittance(
        receiver, gui_input, shadow_camera_info.view_proj_mat,
        shadow_map_tex, leaf_shadow_opacity_blended_tex, leaf_shadow_mask_tex,
        cloud_shadow_tex, shadow_map_tex_for_vsm_ping, scalar_bn, unit_vec2_bn);
    return shadow * sun_info.sun_color * sun_info.sun_luminance * sunAboveHorizon;
}

float3 nextTracingPosition(float3 voxelCenter, float3 voxelNormal)
{
    float2 distance = intersectSlabs(float3(-1.0), float3(1.0),
                                     float3(0.0), 1.0 / voxelNormal);
    const float halfVoxelSize = 1.0 / 512.0;
    return voxelCenter + voxelNormal * (distance.y * halfVoxelSize + 0.005);
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

void parseTraceResult(out float3 albedo, out float3 nextPosition, MarchingResult result)
{
    albedo = voxelColorByTypeAndHash(voxel_colors, result.voxel_type, result.hash_id);
    uint atlasData = terrainAtlasData(result.center_position);
    albedo = applyTerrainMoisture(albedo, result.voxel_type,
                                  voxelMoistureFromAtlasData(atlasData));
    albedo = applyTerrainFertility(albedo, result.voxel_type,
                                   voxelFertilityFromAtlasData(atlasData),
                                   result.center_position);
    nextPosition = nextTracingPosition(result.center_position, result.normal);
}

float3 directLighting(float3 albedo, float3 normal, float3 nextPosition, int3 seed)
{
    Ray shadowRay;
    shadowRay.direction = sampleSunDisk(seed, sun_info.sun_size, sun_info.sun_dir);
    shadowRay.inverse_direction = 1.0 / shadowRay.direction;
    shadowRay.origin = nextPosition;
    float cosine = dot(shadowRay.direction, normal);
    if (cosine <= 0.0) return float3(0.0);
    return shadowRayColor(shadowRay, seed) * cosine * albedo;
}

float3 indirectLighting(float3 albedo, float3 primaryNormal,
                        float3 nextPosition, int3 seed, uint localInvocationIndex)
{
    int3 indirectSeed = int3((seed.xy + int2(13, 37)) % 128, seed.z);
    Ray indirectRay;
    indirectRay.direction = sampleDiffuseBounce(indirectSeed, primaryNormal);
    indirectRay.inverse_direction = 1.0 / indirectRay.direction;
    indirectRay.origin = nextPosition;

    MarchingResult result = generalSceneMarching(indirectRay, localInvocationIndex);
    if (!result.is_hit) return float3(0.0);

    float3 sampleAlbedo;
    float3 sampleNextPosition;
    parseTraceResult(sampleAlbedo, sampleNextPosition, result);

    Ray shadowRay;
    shadowRay.direction = sampleSunDisk(indirectSeed, sun_info.sun_size, sun_info.sun_dir);
    shadowRay.inverse_direction = 1.0 / shadowRay.direction;
    shadowRay.origin = sampleNextPosition;
    float cosine = dot(shadowRay.direction, result.normal);
    if (cosine <= 0.0) return float3(0.0);
    return shadowRayColor(shadowRay, indirectSeed) * cosine * sampleAlbedo * albedo;
}

float depthFromWorldPosition(float3 worldPosition)
{
    float4 ndc = mul(camera_info.view_proj_mat, float4(worldPosition, 1.0));
    return ndc.z / ndc.w;
}

void getPixelColor(out float3 color, out float depth, out float3 primaryNormal,
                   out float3 primaryPosition, out uint primaryVoxelId,
                   out bool primaryIsHit, Ray ray, int3 seed,
                   uint localInvocationIndex)
{
    MarchingResult result = generalSceneMarching(ray, localInvocationIndex);
    if (!result.is_hit)
    {
        depth = 1.0;
        color = float3(0.0);
        primaryNormal = float3(0.0);
        primaryPosition = float3(0.0);
        primaryVoxelId = 0u;
        primaryIsHit = false;
        return;
    }

    depth = depthFromWorldPosition(result.position);
    primaryNormal = result.normal;
    primaryPosition = result.position;
    primaryVoxelId = result.voxel_address;
    primaryIsHit = true;

    float3 albedo;
    float3 nextPosition;
    parseTraceResult(albedo, nextPosition, result);
    color = shading_info.ambient_light * albedo;
    color += directLighting(albedo, result.normal, nextPosition, seed);
    if (gui_input.debug_bool != 0u)
        color += indirectLighting(albedo, result.normal, nextPosition, seed,
                                  localInvocationIndex);
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
    float depth;
    float3 primaryNormal;
    float3 primaryPosition;
    uint primaryVoxelId;
    bool primaryIsHit;
    getPixelColor(color, depth, primaryNormal, primaryPosition,
                  primaryVoxelId, primaryIsHit, ray, seed, localInvocationIndex);

    float2 previousUv = projectWorldToScreen(
        primaryPosition, camera_info_prev_frame.view_proj_mat);
    float2 currentUv = projectWorldToScreen(primaryPosition, camera_info.view_proj_mat);
    float2 motion = previousUv - currentUv;

    compute_output_tex[dispatchThreadId.xy] = packRgbe(color);
    compute_depth_tex[dispatchThreadId.xy] = depth;
    denoiser_normal_tex[dispatchThreadId.xy] = packNormalV2(primaryNormal);
    denoiser_position_tex[dispatchThreadId.xy] =
        float4(primaryPosition, primaryIsHit ? 1.0 : 0.0);
    denoiser_vox_id_tex[dispatchThreadId.xy] = primaryVoxelId;
    denoiser_motion_tex[dispatchThreadId.xy] = motion;
}
