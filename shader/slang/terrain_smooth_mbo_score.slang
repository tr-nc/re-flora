import chunk_writer_types;
import terrain_smooth_mbo;

static const uint TERRAIN_SMOOTH_MBO_GROUP_SIZE = 8u;

[[vk::binding(0, 0)]]
ConstantBuffer<U_TerrainSmoothMboInfo> terrain_smooth_mbo_info;

[[vk::binding(1, 0)]]
readonly GLSLShaderStorageBuffer<B_TerrainSmoothMboDensityA, Std430DataLayout>
    terrain_smooth_mbo_density_a;

[[vk::binding(2, 0)]]
writeonly GLSLShaderStorageBuffer<B_TerrainSmoothMboScores, Std430DataLayout>
    terrain_smooth_mbo_scores;

[[vk::binding(3, 0)]]
GLSLShaderStorageBuffer<B_TerrainSmoothMboHistogram, Std430DataLayout>
    terrain_smooth_mbo_histogram;

[[vk::binding(4, 0)]]
GLSLShaderStorageBuffer<B_TerrainSmoothMboResult, Std430DataLayout>
    terrain_smooth_mbo_result;

[[vk::binding(5, 0)]]
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
    float original = terrainSmoothMboOriginalDensity(voxelType);
    float score = original;

    if (terrainSmoothMboIsCandidateVoxel(
        terrain_smooth_mbo_info, local, voxelType))
    {
        float falloff = terrainSmoothMboBrushFalloff(
            terrain_smooth_mbo_info, local);
        float localStrength = clamp(
            terrain_smooth_mbo_info.params.x * falloff * 2.25,
            0.0, 1.0);
        score = lerp(
            original, terrain_smooth_mbo_density_a.data[index], localStrength);
        if (abs(score - original) <=
            terrain_smooth_mbo_info.params.y *
            TERRAIN_SMOOTH_MBO_DEAD_SCORE_SCALE)
        {
            score = original;
        }

        uint ignored;
        InterlockedAdd(
            terrain_smooth_mbo_result.counts.x, 1u, ignored);
        if (terrainSmoothMboIsTerrainVoxel(voxelType))
        {
            InterlockedAdd(
                terrain_smooth_mbo_result.counts.y, 1u, ignored);
        }
        uint bin = terrainSmoothMboScoreBin(
            terrain_smooth_mbo_info, score);
        InterlockedAdd(
            terrain_smooth_mbo_histogram.bins[bin], 1u, ignored);
    }

    terrain_smooth_mbo_scores.data[index] = score;
}
