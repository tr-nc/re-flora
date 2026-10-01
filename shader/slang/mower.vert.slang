import tracer_types;

[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;

struct VertexInput
{
    [[vk::location(0)]] float3 position : POSITION0;
    [[vk::location(1)]] float3 normal : NORMAL0;
    [[vk::location(2)]] float4 color : COLOR0;
    [[vk::location(3)]] float3 base_position : TEXCOORD0;
    [[vk::location(4)]] float yaw : TEXCOORD1;
};
struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float3 normal : NORMAL0;
    [[vk::location(1)]] float4 color : COLOR0;
};
float3 rotateYaw(float3 v, float yaw)
{
    float s = sin(yaw);
    float c = cos(yaw);
    return float3(c * v.x + s * v.z, v.y, -s * v.x + c * v.z);
}
[shader("vertex")]
VertexOutput main(VertexInput input)
{
    VertexOutput output;
    float3 world = input.base_position + rotateYaw(input.position, input.yaw);
    output.position = mul(camera_info.view_proj_mat, float4(world, 1.0));
    output.normal = rotateYaw(input.normal, input.yaw);
    output.color = input.color;
    return output;
}
