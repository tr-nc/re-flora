import tracer_types;

[[vk::binding(3, 0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;

struct VertexInput
{
    [[vk::location(0)]] float3 position : POSITION0;
    [[vk::location(4)]] float3 base_position : TEXCOORD1;
    [[vk::location(5)]] float4 tint : COLOR1;
    [[vk::location(6)]] float4 rotation : TEXCOORD2;
};

struct VertexOutput
{
    float4 position : SV_Position;
};

float3 rotateByQuaternion(float3 value, float4 rotation)
{
    return value + 2.0 * cross(
        rotation.xyz,
        cross(rotation.xyz, value) + rotation.w * value);
}

[shader("vertex")]
VertexOutput main(VertexInput input)
{
    VertexOutput output;
    float3 worldPosition = input.base_position +
        rotateByQuaternion(input.position * input.tint.a, input.rotation);
    output.position = mul(
        shadow_camera_info.view_proj_mat, float4(worldPosition, 1.0));
    return output;
}
