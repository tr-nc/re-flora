module tree_display_material;
import tracer_types;
import tracer_material;
import voxel_types;
// Both displays interpolate the same spatially evaluated mesh irradiance.
public float3 shadeTreeWood(float3 irradiance,U_VoxelColors colors) {
    return voxelColorByType(colors,VOXEL_TYPE_CHERRY_WOOD)*irradiance;
}
