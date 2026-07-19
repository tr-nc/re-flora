import color;
import flora_shadow;
import terrain_edit_preview;
import tracer_types;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2, 0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4, 0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(5, 0)]] Sampler2D shadow_map_tex_for_vsm_ping;
[[vk::binding(9, 0)]] Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(10, 0)]] Sampler2D leaf_shadow_mask_tex;
[[vk::binding(11, 0)]] Sampler2D cloud_shadow_tex;
[[vk::binding(15, 0)]] ConstantBuffer<U_TerrainEditPreview> terrain_edit_preview;

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
    [[vk::location(0)]] float3 color : COLOR0;
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
    float3 voxelCenter = input.base_position +
        rotateByQuaternion(input.voxel_center * input.tint.a, input.rotation);
    float3 shadingNormal = rotateByQuaternion(
        input.shading_normal, input.rotation);
    output.position = mul(
        camera_info.view_proj_mat, float4(worldPosition, 1.0));

    float3 baseColor = srgbToLinear(input.color_srgb * input.tint.rgb);
    // Sample just over half a voxel toward the light. The caster is rasterized
    // from its exact faces, while lighting samples voxel centers; this removes
    // front-layer self-shadow acne without skipping a one-voxel external blocker.
    float3 shadowSampleCenter = voxelCenter -
        normalize(sun_info.sun_dir) * (0.55 / 256.0);
    float shadowWeight = stylizedVoxelShadowWeight(
        gui_input,
        sun_info,
        shadow_camera_info,
        shadow_map_tex_for_vsm_ping,
        leaf_shadow_opacity_blended_tex,
        leaf_shadow_mask_tex,
        cloud_shadow_tex,
        shadowSampleCenter,
        shadingNormal);
    float3 litColor = applyStylizedVoxelLighting(
        sun_info, shading_info, baseColor, shadowWeight);
    output.color = applyTerrainEditPreviewTint(
        terrain_edit_preview, litColor, voxelCenter);
    return output;
}
