import solid_workgroups;
import sparse_surface_build;
import terrain_smooth;
import voxel_data;
import voxel_types;

[[vk::binding(0, 0)]]
ConstantBuffer<U_TerrainSmoothInfo> terrain_smooth_info;

[[vk::binding(1, 0)]]
readonly GLSLShaderStorageBuffer<B_TerrainSmoothColumns, Std430DataLayout>
    terrain_smooth_columns;

[[vk::binding(2, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

[[vk::binding(3, 0)]]
GLSLShaderStorageBuffer<B_TerrainSmoothResult, Std430DataLayout>
    terrain_smooth_result;

[[vk::binding(4, 0)]]
GLSLShaderStorageBuffer<B_SolidWorkgroupFlags, Std430DataLayout>
    solid_workgroup_flags;

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

void recordChanged()
{
    uint ignored;
    InterlockedAdd(
        terrain_smooth_result.changed_voxel_count, 1u, ignored);
}

[shader("compute")]
[numthreads(8, 8, 8)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint3 local = dispatchThreadId;
    if (local.x >= terrain_smooth_info.dim.x ||
        local.y >= terrain_smooth_info.dim.y ||
        local.z >= terrain_smooth_info.dim.z)
    {
        return;
    }

    uint2 column = local.xz;
    float4 entry = terrain_smooth_columns.data[
        terrainSmoothColumnIndex(terrain_smooth_info, column)];
    if (entry.z <= 0.0 ||
        abs(entry.y - entry.x) <= terrain_smooth_info.deadband_vox)
    {
        return;
    }

    float2 atlasXZ = float2(
        terrain_smooth_info.offset.x + column.x,
        terrain_smooth_info.offset.z + column.y) + float2(0.5);
    float centerDistance = length(
        atlasXZ - terrain_smooth_info.center_xz_vox);
    if (centerDistance > terrain_smooth_info.brush_radius_vox)
    {
        return;
    }

    int3 worldVoxelPosition = int3(local + terrain_smooth_info.offset);
    uint atlasData = chunk_atlas[worldVoxelPosition];
    uint voxelType = voxelTypeFromAtlasData(atlasData);
    float voxelCenterY = float(worldVoxelPosition.y) + 0.5;
    float oldHeight = entry.x;
    float targetHeight = entry.y;

    if (targetHeight < oldHeight - terrain_smooth_info.deadband_vox)
    {
        if (voxelCenterY > targetHeight && voxelCenterY <= oldHeight &&
            terrainSmoothIsTerrainVoxel(voxelType))
        {
            chunk_atlas[worldVoxelPosition] =
                packVoxelAtlasTypeClearState(VOXEL_TYPE_EMPTY);
            recordChanged();
        }
        return;
    }

    if (targetHeight > oldHeight + terrain_smooth_info.deadband_vox &&
        voxelCenterY > oldHeight && voxelCenterY <= targetHeight)
    {
        if (voxelType == VOXEL_TYPE_EMPTY)
        {
            chunk_atlas[worldVoxelPosition] =
                packVoxelAtlasTypeClearState(VOXEL_TYPE_DIRT);
            markSolidWorkgroup(uint3(worldVoxelPosition));
            recordChanged();
        }
        else if (terrainSmoothIsTerrainVoxel(voxelType) &&
                 voxelType != VOXEL_TYPE_DIRT)
        {
            chunk_atlas[worldVoxelPosition] =
                packVoxelAtlasTypePreserveState(atlasData, VOXEL_TYPE_DIRT);
            markSolidWorkgroup(uint3(worldVoxelPosition));
            recordChanged();
        }
    }
}
