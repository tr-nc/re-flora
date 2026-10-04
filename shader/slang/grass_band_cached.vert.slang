import flora_types;
import tracer_types;
import flora_lighting_cache;
import stem_band_geometry;
import grass_band_cache;

// This consumer uses prepared color, not DDGI resources or live wind fields.
// Keeping its bindings separate avoids redundant raster-stage resource barriers.
[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::push_constant]] ConstantBuffer<PushConstantFlora> flora_pc;
[[vk::binding(9, 1)]] StructuredBuffer<GrassBandPose> grass_band_pose_cache;

struct VertexInput
{
    [[vk::location(0)]] uint vertex : TEXCOORD0;
    uint instance : SV_VulkanInstanceID;
};

[shader("vertex")]
StemBandVertex main(VertexInput input)
{
    StemBandVertex output;
    output.position = float4(2, 2, 2, 1);
    output.color = float3(0);
    uint band = input.vertex >> 3u;
    PushConstantFlora pc = flora_pc;
    uint voxelCount = floraLightingCacheVoxelCount(pc);
    if (band >= voxelCount) return output;

    uint species = pc.instance_ty & FLORA_INSTANCE_TYPE_MASK;
    uint localIndex = input.instance - species * MAX_FLORA_INSTANCES_PER_SPECIES;
    uint index = floraLightingCacheIndex(pc, localIndex, band);
    GrassBandPose pose = grass_band_pose_cache[index];
    uint height = uint(pose.position_height.w);
    if (band >= height || pose.color_valid.w == 0.0) return output;

    float3 center = pose.position_height.xyz;
    float3 previous = center, next = center;
    bool hasNext = false;
    if (band > 0u)
        previous = grass_band_pose_cache[index - 1u].position_height.xyz;
    if (band + 1u < height && band + 1u < voxelCount)
    {
        GrassBandPose nextPose = grass_band_pose_cache[index + 1u];
        next = nextPose.position_height.xyz;
        hasNext = nextPose.color_valid.w != 0.0;
    }
    float3 a, b, tangentA, tangentB;
    stemBandCenterFrames(center, previous, next, band > 0u, hasNext,
        0.5 / 256.0, a, b, tangentA, tangentB);
    float ra = 0.5 / 256.0 * stemBandTaper(gui_input.stem_band_mode, float(band) / float(height));
    float rb = 0.5 / 256.0 * stemBandTaper(gui_input.stem_band_mode, float(band + 1u) / float(height));
    float3 world = stemBandPoint(a, b, tangentA, tangentB,
        float3(1, 0, 0), float3(1, 0, 0), float2(ra), float2(rb),
        stemBandCorner(input.vertex));
    output.position = mul(camera_info.view_proj_mat, float4(world, 1));
    output.color = pose.color_valid.xyz;
    return output;
}
