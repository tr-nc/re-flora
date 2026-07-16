import surface_build;
import surface_extraction;

struct B_MakeSurfaceResult
{
    uint active_voxel_len;
    uint active_brick_len;
};

[[vk::binding(0, 0)]]
ConstantBuffer<U_MakeSurfaceInfo> make_surface_info;

[[vk::binding(1, 0)]]
GLSLShaderStorageBuffer<B_MakeSurfaceResult, Std430DataLayout> make_surface_result;

[[vk::binding(2, 0)]]
[[vk::image_format("r32ui")]]
WTexture3D<uint> surface;

[[vk::binding(3, 0)]]
[[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;

[[vk::binding(4, 0)]]
GLSLShaderStorageBuffer<B_SurfaceActiveBrickFlags, Std430DataLayout>
    surface_active_brick_flags;

[[vk::binding(5, 0)]]
writeonly GLSLShaderStorageBuffer<B_SurfaceActiveBrickIndices, Std430DataLayout>
    surface_active_brick_indices;

[shader("compute")]
[numthreads(SURFACE_GROUP_SIZE, SURFACE_GROUP_SIZE, SURFACE_GROUP_SIZE)]
void main(uint3 groupId : SV_GroupID,
          uint3 dispatchThreadId : SV_DispatchThreadID,
          uint localInvocationIndex : SV_GroupIndex)
{
    int3 workgroupBase = int3(groupId * SURFACE_GROUP_SIZE);
    preloadSurfaceData(chunk_atlas, int3(make_surface_info.atlas_read_offset),
                       int3(make_surface_info.atlas_read_dim),
                       make_surface_info.is_crossing_boundary != 0u,
                       workgroupBase, localInvocationIndex);
    GroupMemoryBarrierWithGroupSync();

    int3 voxelIndex = int3(dispatchThreadId);
    uint properties;
    uint brickIndex;
    if (!extractSurfaceVoxel(workgroupBase, voxelIndex,
                             int3(make_surface_info.atlas_read_offset),
                             make_surface_info.atlas_read_dim,
                             properties, brickIndex))
    {
        return;
    }

    uint oldActiveVoxelLength;
    InterlockedAdd(make_surface_result.active_voxel_len, 1u, oldActiveVoxelLength);

    uint flagWord = brickIndex >> 5u;
    uint flagMask = 1u << (brickIndex & 31u);
    uint oldFlags;
    InterlockedOr(surface_active_brick_flags.data[flagWord], flagMask, oldFlags);
    if ((oldFlags & flagMask) == 0u)
    {
        uint activeBrickIndex;
        InterlockedAdd(make_surface_result.active_brick_len, 1u, activeBrickIndex);
        surface_active_brick_indices.data[activeBrickIndex] = brickIndex;
    }

    surface.Store(uint3(voxelIndex), properties);
}
