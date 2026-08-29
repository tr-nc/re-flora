import contree_marching;
import contree_node;
import marching_result;
import ray;
import scene_query;
import scene_marching;
import tracer_types;
import voxel_types;

static const float TERRAIN_VOXEL_SIZE = 1.0 / 256.0;

[[vk::binding(0, 0)]]
ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;

[[vk::binding(1, 0)]]
StructuredBuffer<ContreeNode> contree_node_data;

[[vk::binding(2, 0)]]
StructuredBuffer<uint> contree_leaf_data;

[[vk::binding(3, 0)]]
[[vk::image_format("rg32ui")]]
readonly RWTexture3D<uint2> scene_tex;

[[vk::binding(4, 0)]]
[[vk::image_format("r32f")]]
RWTexture2D<float> shadow_map_tex;

[[vk::binding(5, 0)]]
ConstantBuffer<U_GuiInput> gui_input;

[[vk::binding(6, 0)]] ConstantBuffer<U_ShadingInfo> shading_info;

[[vk::binding(7, 0)]] [[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;

bool isTerrainVoxel(uint voxelType)
{
    return voxelType == VOXEL_TYPE_DIRT ||
           voxelType == VOXEL_TYPE_SAND ||
           voxelType == VOXEL_TYPE_ROCK ||
           voxelType == VOXEL_TYPE_EMISSIVE;
}

float tracePixelDepth(Ray ray, uint localInvocationIndex)
{
    MarchingResult result;
    if (shading_info.glass_experiment_enabled != 0u)
    {
        ScenePrimaryQuery query = tracePrimaryVoxelSurfaces(
            chunk_atlas, ray.origin, ray.direction, 256.0, true);
        result = sceneQueryMarchingResultFromOpaqueEvent(
            query.opaque_hit, ray.origin);
    }
    else
    {
        result = marchScene(
            contree_node_data, contree_leaf_data, scene_tex, ray.origin,
            ray.direction, ray.inverse_direction, localInvocationIndex);
    }
    if (!result.is_hit)
    {
        return 1.0;
    }

    float3 casterPosition = result.position;
    if (isTerrainVoxel(result.voxel_type))
    {
        float casterBias = clamp(gui_input.terrain_self_shadow_tolerance_voxels, 0.0, 2.0) *
                           TERRAIN_VOXEL_SIZE;
        casterPosition += ray.direction * casterBias;
    }

    float4 pointNdc = mul(shadow_camera_info.view_proj_mat,
                          float4(casterPosition, 1.0));
    return pointNdc.z / pointNdc.w;
}

[shader("compute")]
[numthreads(8, 8, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID,
          uint localInvocationIndex : SV_GroupIndex)
{
    uint width;
    uint height;
    shadow_map_tex.GetDimensions(width, height);
    if (any(dispatchThreadId.xy >= uint2(width, height)))
    {
        return;
    }

    float2 screenUv = (float2(dispatchThreadId.xy) + 0.5) / float2(width, height);
    Ray ray = generateCameraRay(screenUv,
                                shadow_camera_info.view_proj_mat_inv);
    float terrainDepth = tracePixelDepth(ray, localInvocationIndex);
    float existingDepth = shadow_map_tex[dispatchThreadId.xy];
    shadow_map_tex[dispatchThreadId.xy] = min(terrainDepth, existingDepth);
}
