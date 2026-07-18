import tracer_types;

[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;

struct VertexInput
{
    [[vk::location(0)]] float3 position : POSITION0;
    [[vk::location(1)]] float4 color_alpha : COLOR0;
    [[vk::location(2)]] float3 base_position : TEXCOORD0;
    [[vk::location(3)]] float4 tint : COLOR1;
    [[vk::location(4)]] float4 rotation : TEXCOORD1;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float4 color_alpha : COLOR0;
};

[shader("vertex")]
VertexOutput main(VertexInput input)
{
    VertexOutput output;
    float3 quaternionVector = input.rotation.xyz;
    float3 rotatedPosition = input.position + 2.0 * cross(
        quaternionVector,
        cross(quaternionVector, input.position) + input.rotation.w * input.position);
    float3 worldPosition = input.base_position + rotatedPosition;
    output.position = mul(camera_info.view_proj_mat, float4(worldPosition, 1.0));
    output.color_alpha = input.color_alpha * input.tint;
    return output;
}
