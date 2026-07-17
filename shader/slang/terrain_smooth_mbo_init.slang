import chunk_writer_types;
import terrain_smooth_mbo;

static const uint TERRAIN_SMOOTH_MBO_GROUP_SIZE = 8u;

[[vk::binding(0, 0)]]
ConstantBuffer<U_TerrainSmoothMboInfo> terrain_smooth_mbo_info;

[[vk::binding(1, 0)]]
writeonly GLSLShaderStorageBuffer<B_TerrainSmoothMboDensityA, Std430DataLayout>
    terrain_smooth_mbo_density_a;

[[vk::binding(2, 0)]]
writeonly GLSLShaderStorageBuffer<B_TerrainSmoothMboDensityB, Std430DataLayout>
    terrain_smooth_mbo_density_b;

[[vk::binding(3, 0)]]
[[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;

[shader("compute")]
[numthreads(TERRAIN_SMOOTH_MBO_GROUP_SIZE,
            TERRAIN_SMOOTH_MBO_GROUP_SIZE,
            TERRAIN_SMOOTH_MBO_GROUP_SIZE)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint3 local = dispatchThreadId;
    if (any(local >= terrain_smooth_mbo_info.dim.xyz))
    {
        return;
    }

    uint index = terrainSmoothMboLocalIndex(
        terrain_smooth_mbo_info, local);
    uint voxelType = terrainSmoothMboLoadVoxelType(
        terrain_smooth_mbo_info, chunk_atlas, local);
    float density = terrainSmoothMboOriginalDensity(voxelType);
    terrain_smooth_mbo_density_a.data[index] = density;
    terrain_smooth_mbo_density_b.data[index] = density;
}
