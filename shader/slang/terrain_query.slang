import contree_node;
import marching_result;
import ray;
import scene_marching;

static const uint TERRAIN_QUERY_GROUP_SIZE = 64u;

struct U_TerrainQueryCount
{
    uint valid_query_count;
};

struct TerrainRayQuery
{
    float4 origin;
    float4 direction;
};

[[vk::binding(0, 0)]]
ConstantBuffer<U_TerrainQueryCount> terrain_query_count;

[[vk::binding(1, 0)]]
StructuredBuffer<TerrainRayQuery> terrain_query_info;

[[vk::binding(2, 0)]]
StructuredBuffer<ContreeNode> contree_node_data;

[[vk::binding(3, 0)]]
StructuredBuffer<uint> contree_leaf_data;

[[vk::binding(4, 0)]]
[[vk::image_format("rg32ui")]]
readonly RWTexture3D<uint2> scene_tex;

[[vk::binding(5, 0)]]
RWStructuredBuffer<float4> terrain_query_result;

[shader("compute")]
[numthreads(TERRAIN_QUERY_GROUP_SIZE, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID,
          uint localInvocationIndex : SV_GroupIndex)
{
    uint queryIndex = dispatchThreadId.x;
    if (queryIndex >= terrain_query_count.valid_query_count)
    {
        return;
    }

    float3 rayOrigin = terrain_query_info[queryIndex].origin.xyz;
    float3 rayDirection = terrain_query_info[queryIndex].direction.xyz;
    if (dot(rayDirection, rayDirection) < 1.0e-8)
    {
        terrain_query_result[queryIndex] = float4(0.0);
        return;
    }
    rayDirection = normalize(rayDirection);

    Ray ray;
    ray.origin = rayOrigin;
    ray.direction = rayDirection;
    ray.inverse_direction = 1.0 / ray.direction;

    MarchingResult result = marchScene(
        contree_node_data, contree_leaf_data, scene_tex,
        ray.origin, ray.direction, ray.inverse_direction,
        localInvocationIndex);
    terrain_query_result[queryIndex] = result.is_hit
        ? float4(result.position, 1.0)
        : float4(0.0);
}
