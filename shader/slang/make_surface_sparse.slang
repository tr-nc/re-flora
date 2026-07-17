import sparse_surface_build;
import surface_build;
import surface_extraction;

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

[[vk::binding(6, 0)]]
readonly GLSLShaderStorageBuffer<B_SurfaceSolidWorkgroupIndices, Std430DataLayout>
    surface_solid_workgroup_indices;

uint3 decodeSurfaceWorkgroupCoordinate(uint localWorkgroupIndex)
{
    uint3 gridDimension =
        (make_surface_info.atlas_read_dim + uint3(SURFACE_GROUP_SIZE - 1u)) /
        SURFACE_GROUP_SIZE;
    return uint3(localWorkgroupIndex % gridDimension.x,
                 localWorkgroupIndex / (gridDimension.x * gridDimension.z),
                 (localWorkgroupIndex / gridDimension.x) % gridDimension.z);
}

[shader("compute")]
[numthreads(SURFACE_GROUP_SIZE, SURFACE_GROUP_SIZE, SURFACE_GROUP_SIZE)]
void main(uint3 groupId : SV_GroupID,
          uint3 localInvocationId : SV_GroupThreadID,
          uint localInvocationIndex : SV_GroupIndex)
{
    uint workgroupListIndex = groupId.x;
    if (workgroupListIndex >= make_surface_result.solid_workgroup_len)
    {
        return;
    }

    uint localWorkgroupIndex = surface_solid_workgroup_indices.data[workgroupListIndex];
    int3 workgroupBase =
        int3(decodeSurfaceWorkgroupCoordinate(localWorkgroupIndex) * SURFACE_GROUP_SIZE);

    preloadSurfaceData(chunk_atlas, int3(make_surface_info.atlas_read_offset),
                       int3(make_surface_info.atlas_read_dim),
                       make_surface_info.is_crossing_boundary != 0u,
                       workgroupBase, localInvocationIndex);
    GroupMemoryBarrierWithGroupSync();

    int3 voxelIndex = workgroupBase + int3(localInvocationId);
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
