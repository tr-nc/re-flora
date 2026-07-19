import tracer_types;

[[vk::binding(3, 0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;

struct VertexInput
{
    [[vk::location(0)]] float3 position : POSITION0;
    [[vk::location(1)]] float3 voxel_center : TEXCOORD0;
    [[vk::location(2)]] float3 shading_normal : NORMAL0;
    [[vk::location(3)]] float3 color_srgb : COLOR0;
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
    // Both passes deliberately share one compact vertex/instance layout. This
    // sub-pixel bias keeps every attribute reflected without changing the
    // visible shadow at the 2048x2048 shadow-map resolution.
    float layoutBias =
        dot(input.voxel_center, float3(1.0, 2.0, 3.0)) +
        dot(input.shading_normal, float3(3.0, 2.0, 1.0)) +
        dot(input.color_srgb, float3(1.0, 1.0, 1.0)) + input.tint.a;
    output.position.z += layoutBias * 1.0e-10;
    return output;
}
