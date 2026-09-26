import tracer_types;
import voxel_types;
import tracer_material;
[[vk::binding(11,0)]] ConstantBuffer<U_VoxelColors> voxel_colors;
struct FragmentInput { [[vk::location(0)]] float3 irradiance : COLOR0; };
[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0 {
    return float4(voxelColorByType(voxel_colors,VOXEL_TYPE_CHERRY_WOOD)*input.irradiance,1);
}
