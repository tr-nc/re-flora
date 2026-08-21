import solid_workgroups;
import sparse_surface_build;
import terrain_height;
import terrain_implicit;
import voxel_data;
import voxel_types;

struct TerrainGradePushConstants
{
    uint4 offset;
    uint4 dim;
    float4 center_half_extent;
    float4 height_feather;
};

[[vk::push_constant]]
ConstantBuffer<TerrainGradePushConstants> pc;

[[vk::binding(0, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

[[vk::binding(1, 0)]]
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

[shader("compute")]
[numthreads(8, 8, 8)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    if (any(dispatchThreadId >= pc.dim.xyz))
    {
        return;
    }

    uint3 atlasPosition = pc.offset.xyz + dispatchThreadId;
    float3 voxelCenter = float3(atlasPosition) + 0.5;
    float2 center = pc.center_half_extent.xy;
    float2 halfExtent = pc.center_half_extent.zw;
    float feather = pc.height_feather.y;
    float2 innerHalfExtent = max(halfExtent - feather, 0.0);
    float distanceFromInner = signedDistanceBox2D(
        voxelCenter.xz - center,
        innerHalfExtent);
    float gradeWeight = 1.0 - smoothstep(0.0, feather, distanceFromInner);
    if (gradeWeight <= 0.0)
    {
        return;
    }

    float2 worldXZ = voxelCenter.xz * (1.0 / 256.0);
    float naturalHeightVoxels = computeSurfaceHeight2D(worldXZ) * 256.0;
    float targetHeightVoxels = lerp(
        naturalHeightVoxels,
        pc.height_feather.x,
        gradeWeight);
    bool wantsSolid = voxelCenter.y <= targetHeightVoxels;

    uint oldAtlasData = chunk_atlas[atlasPosition];
    uint oldVoxelType = voxelTypeFromAtlasData(oldAtlasData);
    bool oldIsTerrain = oldVoxelType == VOXEL_TYPE_DIRT ||
                        oldVoxelType == VOXEL_TYPE_SAND ||
                        oldVoxelType == VOXEL_TYPE_ROCK;

    if (wantsSolid)
    {
        if (oldVoxelType == VOXEL_TYPE_EMPTY)
        {
            chunk_atlas[atlasPosition] =
                packVoxelAtlasTypeClearState(VOXEL_TYPE_DIRT);
            markSolidWorkgroup(atlasPosition);
        }
        return;
    }

    if (oldIsTerrain)
    {
        chunk_atlas[atlasPosition] =
            packVoxelAtlasTypeClearState(VOXEL_TYPE_EMPTY);
    }
}
