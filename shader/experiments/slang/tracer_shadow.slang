import contree_marching;
import contree_node;
import marching_result;
import ray;
import scene_marching;

struct U_ShadowCameraInfo
{
    float4 position;
    float4x4 view_matrix;
    float4x4 inverse_view_matrix;
    float4x4 projection_matrix;
    float4x4 inverse_projection_matrix;
    float4x4 view_projection_matrix;
    float4x4 inverse_view_projection_matrix;
};

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

float tracePixelDepth(Ray ray, uint localInvocationIndex)
{
    MarchingResult result = marchScene(
        contree_node_data, contree_leaf_data, scene_tex, ray.origin, ray.direction,
        ray.inverse_direction, localInvocationIndex);
    if (!result.is_hit)
    {
        return 1.0;
    }

    float4 pointNdc = mul(shadow_camera_info.view_projection_matrix,
                          float4(result.position, 1.0));
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
                                shadow_camera_info.inverse_view_projection_matrix);
    float terrainDepth = tracePixelDepth(ray, localInvocationIndex);
    float existingDepth = shadow_map_tex[dispatchThreadId.xy];
    shadow_map_tex[dispatchThreadId.xy] = min(terrainDepth, existingDepth);
}
