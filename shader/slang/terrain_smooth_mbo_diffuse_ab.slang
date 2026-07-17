import chunk_writer_types;
import terrain_smooth_mbo;

static const uint TERRAIN_SMOOTH_MBO_GROUP_SIZE = 8u;

[[vk::binding(0, 0)]]
ConstantBuffer<U_TerrainSmoothMboInfo> terrain_smooth_mbo_info;

[[vk::binding(1, 0)]]
readonly GLSLShaderStorageBuffer<B_TerrainSmoothMboDensityA, Std430DataLayout>
    terrain_smooth_mbo_density_a;

[[vk::binding(2, 0)]]
writeonly GLSLShaderStorageBuffer<B_TerrainSmoothMboDensityB, Std430DataLayout>
    terrain_smooth_mbo_density_b;

[[vk::binding(3, 0)]]
[[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;

float sampleDensity(int3 local, float fallback)
{
    if (!terrainSmoothMboInLocalBounds(terrain_smooth_mbo_info, local))
    {
        return fallback;
    }
    return terrain_smooth_mbo_density_a.data[
        terrainSmoothMboLocalIndex(terrain_smooth_mbo_info, uint3(local))];
}

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
    float center = terrain_smooth_mbo_density_a.data[index];
    uint voxelType = terrainSmoothMboLoadVoxelType(
        terrain_smooth_mbo_info, chunk_atlas, local);
    if (!terrainSmoothMboIsMutableVoxel(voxelType) ||
        terrainSmoothMboBrushFalloff(terrain_smooth_mbo_info, local) <= 0.0)
    {
        terrain_smooth_mbo_density_b.data[index] = center;
        return;
    }

    int3 position = int3(local);
    float sum = center * 2.0;
    sum += sampleDensity(position + int3(1, 0, 0), center);
    sum += sampleDensity(position + int3(-1, 0, 0), center);
    sum += sampleDensity(position + int3(0, 1, 0), center);
    sum += sampleDensity(position + int3(0, -1, 0), center);
    sum += sampleDensity(position + int3(0, 0, 1), center);
    sum += sampleDensity(position + int3(0, 0, -1), center);
    terrain_smooth_mbo_density_b.data[index] = sum * 0.125;
}
