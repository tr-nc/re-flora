import chunk_writer_types;
import solid_workgroups;
import sparse_surface_build;
import terrain_smooth_mbo;
import voxel_data;
import voxel_types;

static const uint TERRAIN_SMOOTH_MBO_GROUP_SIZE = 8u;

[[vk::binding(0, 0)]]
ConstantBuffer<U_TerrainSmoothMboInfo> terrain_smooth_mbo_info;

[[vk::binding(1, 0)]]
readonly GLSLShaderStorageBuffer<B_TerrainSmoothMboScores, Std430DataLayout>
    terrain_smooth_mbo_scores;

[[vk::binding(2, 0)]]
GLSLShaderStorageBuffer<B_TerrainSmoothMboResult, Std430DataLayout>
    terrain_smooth_mbo_result;

[[vk::binding(3, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

[[vk::binding(4, 0)]]
GLSLShaderStorageBuffer<B_SolidWorkgroupFlags, Std430DataLayout>
    solid_workgroup_flags;

bool wantsSolidFromScore(uint3 local, float score)
{
    uint bin = terrainSmoothMboScoreBin(terrain_smooth_mbo_info, score);
    uint thresholdBin = terrain_smooth_mbo_info.threshold.x;
    if (bin > thresholdBin)
    {
        return true;
    }
    if (bin < thresholdBin)
    {
        return false;
    }
    return terrainSmoothMboHash(
        terrain_smooth_mbo_info.offset.xyz + local) <=
        terrain_smooth_mbo_info.threshold.y;
}

void markSolidWorkgroup(uint3 voxelPosition)
{
    uint width;
    uint height;
    uint depth;
    chunk_atlas.GetDimensions(width, height, depth);
    uint workgroupIndex = solidWorkgroupIndexFromVoxel(
        voxelPosition, uint3(width, height, depth));
    uint flagWord = workgroupIndex >> 5u;
    uint flagMask = 1u << (workgroupIndex & 31u);
    uint ignored;
    InterlockedOr(solid_workgroup_flags.data[flagWord], flagMask, ignored);
}

void recordChanged(uint3 worldPosition, bool added)
{
    uint ignored;
    InterlockedAdd(terrain_smooth_mbo_result.counts.z, 1u, ignored);
    if (added)
    {
        InterlockedAdd(terrain_smooth_mbo_result.counts.w, 1u, ignored);
    }
    else
    {
        InterlockedAdd(terrain_smooth_mbo_result.counts_extra.x, 1u, ignored);
    }
    InterlockedMin(
        terrain_smooth_mbo_result.changed_min.x, worldPosition.x, ignored);
    InterlockedMin(
        terrain_smooth_mbo_result.changed_min.y, worldPosition.y, ignored);
    InterlockedMin(
        terrain_smooth_mbo_result.changed_min.z, worldPosition.z, ignored);
    InterlockedMax(
        terrain_smooth_mbo_result.changed_max.x, worldPosition.x, ignored);
    InterlockedMax(
        terrain_smooth_mbo_result.changed_max.y, worldPosition.y, ignored);
    InterlockedMax(
        terrain_smooth_mbo_result.changed_max.z, worldPosition.z, ignored);
    markSolidWorkgroup(worldPosition);
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

    uint voxelType = terrainSmoothMboLoadVoxelType(
        terrain_smooth_mbo_info, chunk_atlas, local);
    if (!terrainSmoothMboIsCandidateVoxel(
        terrain_smooth_mbo_info, local, voxelType))
    {
        return;
    }

    uint index = terrainSmoothMboLocalIndex(
        terrain_smooth_mbo_info, local);
    bool oldSolid = terrainSmoothMboIsTerrainVoxel(voxelType);
    bool newSolid = wantsSolidFromScore(
        local, terrain_smooth_mbo_scores.data[index]);
    if (oldSolid == newSolid)
    {
        return;
    }

    uint3 worldPosition = terrain_smooth_mbo_info.offset.xyz + local;
    if (newSolid)
    {
        if (voxelType != VOXEL_TYPE_EMPTY)
        {
            return;
        }
        uint fillType = terrainSmoothMboChooseFillVoxelType(
            terrain_smooth_mbo_info, chunk_atlas, local);
        chunk_atlas[worldPosition] = packVoxelAtlasTypeClearState(fillType);
        recordChanged(worldPosition, true);
    }
    else
    {
        if (!oldSolid)
        {
            return;
        }
        chunk_atlas[worldPosition] =
            packVoxelAtlasTypeClearState(VOXEL_TYPE_EMPTY);
        recordChanged(worldPosition, false);
    }
}
