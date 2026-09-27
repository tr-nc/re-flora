module tree_surface;
[[vk::binding(52,0)]] StructuredBuffer<float4> tree_scene_vertices;
[[vk::binding(60,0)]] StructuredBuffer<float4> tree_skin_rest;

public float3 treeSurfacePosition(uint vertex) {
    return tree_scene_vertices[vertex*2u].xyz;
}
public float3 treeSurfaceNormal(uint vertex) {
    return tree_scene_vertices[vertex*2u+1u].xyz;
}
public float3 treeSurfaceRestCenter(uint vertex) {
    return tree_skin_rest[vertex*3u+1u].xyz;
}
