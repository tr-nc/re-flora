// Both stone adapters reuse live garden palette and environment lighting.
module stone_lighting;
import tracer_types;
import tracer_material;
import flora_shadow;
[[vk::binding(0,0)]] public ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1,0)]] public ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] public ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] public ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4,0)]] public ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(11,0)]] public ConstantBuffer<U_VoxelColors> voxel_colors;
public float3 stoneShade(uint material, float3 position, float3 normal)
{
    StylizedVoxelShadowSample shadow = sampleStylizedVoxelShadow(
        gui_input, sun_info, shadow_camera_info, position, normal);
    return applyStylizedVoxelLighting(gui_input, sun_info, shading_info,
        voxelColorByType(voxel_colors, material), shadow.weight, position, normal);
}
