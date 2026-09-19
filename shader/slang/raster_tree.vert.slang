import tracer_types;
import packer;
import tree_surface;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
struct VertexOutput {
    float4 position : SV_Position;
    [[vk::location(0)]] float3 world_position : TEXCOORD0;
    [[vk::location(1)]] nointerpolation float3 center : TEXCOORD1;
    [[vk::location(2)]] float3 normal : NORMAL0;
};
[shader("vertex")]
VertexOutput main(uint index : SV_VulkanVertexID) {
    VertexOutput output;
    output.world_position = treeSurfacePosition(index);
    output.position = mul(camera_info.view_proj_mat, float4(output.world_position,1.0));
    output.center = treeSurfaceRestCenter(index);
    output.normal = unpackNormalOct16(packNormalOct16(treeSurfaceNormal(index)));
    return output;
}
