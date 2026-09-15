import tracer_types;
import packer;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
struct VertexInput {
    [[vk::location(0)]] float3 position : POSITION0;
    [[vk::location(1)]] float3 center : TEXCOORD0;
    [[vk::location(2)]] float3 normal : NORMAL0;
};
struct VertexOutput {
    float4 position : SV_Position;
    [[vk::location(0)]] float3 world_position : TEXCOORD0;
    [[vk::location(1)]] nointerpolation float3 center : TEXCOORD1;
    [[vk::location(2)]] nointerpolation float3 normal : NORMAL0;
};
[shader("vertex")]
VertexOutput main(VertexInput input) {
    VertexOutput output;
    output.position = mul(camera_info.view_proj_mat, float4(input.position,1.0));
    output.world_position = input.position;
    output.center = input.center;
    output.normal = unpackNormalOct16(packNormalOct16(input.normal));
    return output;
}
