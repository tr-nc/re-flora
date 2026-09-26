import tracer_types;
import tree_surface;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(48,0)]] StructuredBuffer<float4> raster_tree_light_cache;
struct VertexOutput {
    float4 position : SV_Position;
    [[vk::location(0)]] float3 irradiance : COLOR0;
};
[shader("vertex")]
VertexOutput main(uint index : SV_VulkanVertexID) {
    VertexOutput output;
    output.position=mul(camera_info.view_proj_mat,float4(treeSurfacePosition(index),1));
    output.irradiance=raster_tree_light_cache[index].xyz;
    return output;
}
