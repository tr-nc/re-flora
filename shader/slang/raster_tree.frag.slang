import tracer_types;
import tree_display_material;
[[vk::binding(11,0)]] ConstantBuffer<U_VoxelColors> voxel_colors;
struct FragmentInput { [[vk::location(0)]] float3 irradiance : COLOR0; };
[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0 {
    return float4(shadeTreeWood(input.irradiance,voxel_colors),1);
}
