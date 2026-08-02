import flora_shadow;
import tracer_types;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2, 0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4, 0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(5, 0)]] Sampler2D shadow_map_tex_for_vsm_ping;
[[vk::binding(7, 0)]] Sampler2D cloud_shadow_tex;
[[vk::binding(9, 0)]] Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(10, 0)]] Sampler2D leaf_shadow_mask_tex;

static const uint SPRITE_FLIP_BIT = 0x80000000u;

struct VertexInput
{
    [[vk::location(0)]] uint packed_data : TEXCOORD0;
    [[vk::location(1)]] float3 instance_position : POSITION0;
    [[vk::location(2)]] float instance_size : TEXCOORD1;
    [[vk::location(3)]] float4 instance_color : COLOR0;
    [[vk::location(4)]] uint instance_texture_index : TEXCOORD2;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float4 color : COLOR0;
    [[vk::location(1)]] float2 uv : TEXCOORD0;
    [[vk::location(2)]] nointerpolation uint texture_index : TEXCOORD1;
};

void unpackVertexData(uint packedData, out int3 voxelLocalPosition,
                      out uint3 vertexOffsetInVoxel)
{
    const uint bitsPerPosition = 7u;
    const uint positionMask = (1u << bitsPerPosition) - 1u;
    voxelLocalPosition = int3(
        packedData & positionMask,
        (packedData >> bitsPerPosition) & positionMask,
        (packedData >> (bitsPerPosition * 2u)) & positionMask) - 64;

    uint packedOffset = (packedData >> (bitsPerPosition * 3u)) & 0x7u;
    vertexOffsetInVoxel = uint3(
        packedOffset & 1u,
        (packedOffset >> 1u) & 1u,
        (packedOffset >> 2u) & 1u);
}

float3 billboardVertexPosition(float3 instancePosition, uint3 vertexOffset,
                               float scale)
{
    float2 mappedOffset = (float2(vertexOffset.xy) - 0.5) * scale * 1.225;
    float3 right = normalize(float3(
        camera_info.view_mat[0][0],
        camera_info.view_mat[0][1],
        camera_info.view_mat[0][2]));
    float3 up = normalize(float3(
        camera_info.view_mat[1][0],
        camera_info.view_mat[1][1],
        camera_info.view_mat[1][2]));
    return instancePosition + right * mappedOffset.x + up * mappedOffset.y;
}

uint instanceHash(float3 instancePosition)
{
    uint3 quantizedPosition = uint3(round(instancePosition * 256.0));
    return quantizedPosition.x * 73856093u ^
           quantizedPosition.y * 19349663u ^
           quantizedPosition.z * 83492791u;
}

float4 applyDepthOffset(float3 worldPosition, float3 instancePosition)
{
    float depthOffset = frac(float(instanceHash(instancePosition)) * 0.0001) * 5.0e-4;
    float4 viewPosition = mul(camera_info.view_mat, float4(worldPosition, 1.0));
    viewPosition.z -= depthOffset;
    return mul(camera_info.proj_mat, viewPosition);
}

[shader("vertex")]
VertexOutput main(VertexInput input)
{
    int3 voxelLocalPosition;
    uint3 vertexOffset;
    unpackVertexData(input.packed_data, voxelLocalPosition, vertexOffset);

    float scale = max(input.instance_size, 0.001) * 1.25;
    float3 vertexPosition = billboardVertexPosition(
        input.instance_position, vertexOffset, scale);

    VertexOutput output;
    output.position = applyDepthOffset(vertexPosition, input.instance_position);

    float shadowWeight = stylizedVoxelShadowWeight(
        gui_input,
        sun_info,
        shadow_camera_info,
        shadow_map_tex_for_vsm_ping,
        leaf_shadow_opacity_blended_tex,
        leaf_shadow_mask_tex,
        cloud_shadow_tex,
        input.instance_position,
        float3(voxelLocalPosition));
    output.color = float4(
        applyStylizedVoxelLighting(
            gui_input, sun_info, shading_info, input.instance_color.rgb,
            shadowWeight, input.instance_position, float3(voxelLocalPosition)),
        input.instance_color.a);

    output.uv = float2(float(vertexOffset.x), 1.0 - float(vertexOffset.y));
    if ((input.instance_texture_index & SPRITE_FLIP_BIT) != 0u)
    {
        output.uv.x = 1.0 - output.uv.x;
    }
    output.texture_index = input.instance_texture_index & ~SPRITE_FLIP_BIT;
    return output;
}
