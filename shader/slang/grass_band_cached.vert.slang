import flora_types;
import tracer_types;
import flora_lighting_cache;
import stem_band_geometry;
import grass_band_cache;
import grass_band_world;

// This consumer uses prepared color, not DDGI resources or live wind fields.
// Keeping its bindings separate avoids redundant raster-stage resource barriers.
[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::push_constant]] ConstantBuffer<PushConstantFlora> flora_pc;

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

    float3 world = grassBandWorldPoint(index, band, height, voxelCount,
        input.vertex);
    output.position = mul(camera_info.view_proj_mat, float4(world, 1));
    output.color = pose.color_valid.xyz;
    return output;
}
