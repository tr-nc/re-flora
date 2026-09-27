import tree_skin;
[[vk::binding(49,0)]] StructuredBuffer<uint4> tree_scene_info;
[[vk::binding(52,0)]] RWStructuredBuffer<float4> tree_scene_vertices;
[[vk::binding(60,0)]] StructuredBuffer<float4> tree_skin_rest;
[[vk::binding(61,0)]] StructuredBuffer<uint4> tree_skin_bindings;
[[vk::binding(62,0)]] StructuredBuffer<float4> tree_skin_poses;

[shader("compute")]
[numthreads(64,1,1)]
void main(uint3 id : SV_DispatchThreadID) {
    uint4 info = tree_scene_info[0];
    uint i = id.x;
    if (i >= info.w) return;
    float3 position = tree_skin_rest[i*3u].xyz;
    float3 normal = tree_skin_rest[i*3u+2u].xyz;
    if (info.x != 0u) {
        uint4 binding = tree_skin_bindings[i];
        float weight = asfloat(binding.z);
        float4 child = tree_skin_poses[binding.x*2u];
        float3 childTranslation = tree_skin_poses[binding.x*2u+1u].xyz;
        float4 parent = tree_skin_poses[binding.y*2u];
        float3 parentTranslation = tree_skin_poses[binding.y*2u+1u].xyz;
        position = treeSkinPoint(parent, parentTranslation, child, childTranslation, weight, position);
        normal = treeSkinNormal(parent, child, weight, normal);
    }
    tree_scene_vertices[i*2u] = float4(position, 1);
    // Reliability belongs to rest occupancy, not camera/wind-dependent geometry.
    tree_scene_vertices[i*2u+1u] = float4(normal, tree_skin_rest[i*3u+2u].w);
}
