import tracer_types;

[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;

struct VertexInput
{
    [[vk::location(0)]] float3 position : POSITION0;
    [[vk::location(1)]] float3 normal : NORMAL0;
    [[vk::location(2)]] float4 color : COLOR0;
    [[vk::location(3)]] float3 base_position : TEXCOORD0;
    [[vk::location(4)]] float4 rotation : TEXCOORD1;
};
struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float3 normal : NORMAL0;
    [[vk::location(1)]] float4 color : COLOR0;
};
float3 rotateModel(float3 v, float4 rotation)
{
    return v + 2.0 * cross(rotation.xyz, cross(rotation.xyz, v) + rotation.w * v);
}
[shader("vertex")]
VertexOutput main(VertexInput input)
{
    VertexOutput output;
    float3 world = input.base_position + rotateModel(input.position, input.rotation);
    output.position = mul(camera_info.view_proj_mat, float4(world, 1.0));
    output.normal = rotateModel(input.normal, input.rotation);
    output.color = input.color;
    return output;
}
