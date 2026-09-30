import tracer_types;

[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;

struct VertexInput
{
    [[vk::location(0)]] float3 position : POSITION0;
    [[vk::location(1)]] float3 normal : NORMAL0;
    [[vk::location(2)]] float4 color : COLOR0;
};
struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float3 normal : NORMAL0;
    [[vk::location(1)]] float4 color : COLOR0;
};
[shader("vertex")]
VertexOutput main(VertexInput input)
{
    VertexOutput output;
    output.position = mul(camera_info.view_proj_mat, float4(input.position, 1.0));
    output.normal = input.normal;
    output.color = input.color;
    return output;
}
