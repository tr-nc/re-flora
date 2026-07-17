import color;
import flora_shadow;
import flora_types;
import terrain_edit_preview;
import tracer_types;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2, 0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4, 0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(5, 0)]] Sampler2D shadow_map_tex_for_vsm_ping;
[[vk::binding(6, 0)]] ConstantBuffer<U_FloraGrowthInfo> flora_growth_info;
[[vk::binding(9, 0)]] Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(10, 0)]] Sampler2D leaf_shadow_mask_tex;
[[vk::binding(11, 0)]] Sampler2D cloud_shadow_tex;
[[vk::binding(15, 0)]] ConstantBuffer<U_TerrainEditPreview> terrain_edit_preview;

static const float CAP_MOTION_SECONDS = 1.0;
static const float ANIMATION_DISTANCE = 1.0 / 256.0;

struct VertexInput
{
    [[vk::location(0)]] float3 position : POSITION0;
    [[vk::location(1)]] float3 voxel_center : TEXCOORD0;
    [[vk::location(2)]] float3 shading_normal : NORMAL0;
    [[vk::location(3)]] float3 color_srgb : COLOR0;
    [[vk::location(4)]] float3 animation_direction : TEXCOORD1;
    [[vk::location(5)]] float3 base_position : TEXCOORD2;
    [[vk::location(6)]] float animation_phase : TEXCOORD3;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float3 color : COLOR0;
};

void sprinklerAnimation(float animationPhase,
                         out float extension,
                         out bool animateXArms)
{
    float tickSeconds = max(gui_input.world_tick_seconds, 1.0 / 240.0);
    uint pairCycleTicks = max(
        uint(round(CAP_MOTION_SECONDS / tickSeconds)), 2u);
    uint fullCycleTicks = pairCycleTicks * 2u;
    uint phaseOffsetTicks = uint(round(
        frac(animationPhase) * float(fullCycleTicks)));
    uint cycleTick =
        (flora_growth_info.flora_tick + phaseOffsetTicks) % fullCycleTicks;
    animateXArms = cycleTick >= pairCycleTicks;
    uint pairTick = cycleTick % pairCycleTicks;
    float pairPhase = float(pairTick) / float(pairCycleTicks);
    extension = 0.5 - 0.5 * cos(pairPhase * 6.28318530718);
}

[shader("vertex")]
VertexOutput main(VertexInput input)
{
    VertexOutput output;
    float extension;
    bool animateXArms;
    sprinklerAnimation(input.animation_phase, extension, animateXArms);
    bool isXArm = abs(input.animation_direction.x) > 0.0;
    bool isZArm = abs(input.animation_direction.z) > 0.0;
    bool armIsActive = (!isXArm && !isZArm) ||
        (animateXArms ? isXArm : isZArm);
    float3 activeDirection = input.animation_direction;
    if (!armIsActive)
    {
        activeDirection.xz = float2(0.0);
    }

    float3 animationOffset = activeDirection * extension * ANIMATION_DISTANCE;
    float3 worldPosition =
        input.base_position + input.position + animationOffset;
    float3 voxelCenter =
        input.base_position + input.voxel_center + animationOffset;
    output.position = mul(
        camera_info.view_proj_mat, float4(worldPosition, 1.0));

    float3 baseColor = srgbToLinear(input.color_srgb);
    float shadowWeight = stylizedVoxelShadowWeight(
        gui_input,
        sun_info,
        shadow_camera_info,
        shadow_map_tex_for_vsm_ping,
        leaf_shadow_opacity_blended_tex,
        leaf_shadow_mask_tex,
        cloud_shadow_tex,
        voxelCenter,
        input.shading_normal);
    float3 litColor = applyStylizedVoxelLighting(
        sun_info, shading_info, baseColor, shadowWeight);
    output.color = applyTerrainEditPreviewTint(
        terrain_edit_preview, litColor, voxelCenter);
    return output;
}
