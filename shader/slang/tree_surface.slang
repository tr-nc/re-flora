module tree_surface;
[[vk::binding(49,0)]] StructuredBuffer<uint4> tree_scene_info;
[[vk::binding(52,0)]] StructuredBuffer<float4> tree_scene_vertices;
[[vk::binding(60,0)]] StructuredBuffer<float4> tree_skin_rest;

public uint treeSurfaceVertex(uint vertex, uint instance) {
    return tree_scene_info[0].z != 0u ? instance*8u+vertex : vertex;
}
public float3 treeSurfacePosition(uint vertex) {
    return tree_scene_vertices[vertex*2u].xyz;
}
public float3 treeSurfaceNormal(uint vertex) {
    return tree_scene_vertices[vertex*2u+1u].xyz;
}
public float3 treeSurfaceRestCenter(uint vertex) {
    uint element = tree_scene_info[0].z != 0u ? vertex/8u : vertex;
    return tree_skin_rest[element*3u+1u].xyz;
}
