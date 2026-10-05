// Both stone adapters reuse live garden palette and environment lighting.
module stone_lighting;
import tracer_types;
import tracer_material;
import flora_shadow;
import model_mesh_lighting;
[[vk::binding(11,0)]] public ConstantBuffer<U_VoxelColors> voxel_colors;
public float3 stoneShade(uint material, float3 position, float3 normal)
{
    StylizedVoxelShadowSample shadow = sampleStylizedVoxelShadow(
        gui_input, sun_info, shadow_camera_info, position, normal);
    return applyStylizedVoxelLighting(gui_input, sun_info, shading_info,
        voxelColorByType(voxel_colors, material), shadow.weight, position, normal);
}
