import surface_build;
import surface_extraction;
import surface_result;

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

void flushSurfaceWorkgroupAggregation(int3 workgroupBase,
                                      uint localInvocationIndex)
{
    if (localInvocationIndex != 0u)
    {
        return;
    }

    uint ignored;
    InterlockedAdd(make_surface_result.active_voxel_len,
                   surfaceWorkgroupActiveVoxelCount(), ignored);
    uint brickMask = surfaceWorkgroupActiveBrickMask();
    uint brickCount = countbits(brickMask);
    if (brickCount == 0u)
    {
        return;
    }

    uint activeBrickBase;
    InterlockedAdd(make_surface_result.active_brick_len, brickCount,
                   activeBrickBase);
    uint brickDimension = make_surface_info.atlas_read_dim.x / 4u;
    uint3 workgroupBrickBase = uint3(workgroupBase) / 4u;
    uint activeBrickOffset = 0u;
    for (uint localBrickIndex = 0u; localBrickIndex < 8u; ++localBrickIndex)
    {
        uint localMask = 1u << localBrickIndex;
        if ((brickMask & localMask) == 0u)
        {
            continue;
        }

        uint3 localBrick;
        localBrick.x = localBrickIndex % 2u;
        localBrick.y = localBrickIndex / 4u;
        localBrick.z = (localBrickIndex / 2u) % 2u;
        uint3 brickCoordinate = workgroupBrickBase + localBrick;
        uint brickIndex = brickCoordinate.x + brickCoordinate.z * brickDimension +
                          brickCoordinate.y * brickDimension * brickDimension;
        uint flagWord = brickIndex >> 5u;
        uint flagMask = 1u << (brickIndex & 31u);
        InterlockedOr(surface_active_brick_flags.data[flagWord], flagMask, ignored);
        surface_active_brick_indices.data[activeBrickBase + activeBrickOffset] =
            brickIndex;
        ++activeBrickOffset;
    }
}

[shader("compute")]
[numthreads(SURFACE_GROUP_SIZE, SURFACE_GROUP_SIZE, SURFACE_GROUP_SIZE)]
void main(uint3 groupId : SV_GroupID,
          uint3 dispatchThreadId : SV_DispatchThreadID,
          uint localInvocationIndex : SV_GroupIndex)
{
    int3 workgroupBase = int3(groupId * SURFACE_GROUP_SIZE);
    resetSurfaceWorkgroupAggregation(localInvocationIndex);
    preloadSurfaceData(chunk_atlas, int3(make_surface_info.atlas_read_offset),
                       int3(make_surface_info.atlas_read_dim),
                       make_surface_info.is_crossing_boundary != 0u,
                       workgroupBase, localInvocationIndex);
    GroupMemoryBarrierWithGroupSync();

    int3 voxelIndex = int3(dispatchThreadId);
    uint properties;
    if (extractSurfaceVoxel(workgroupBase, voxelIndex,
                            int3(make_surface_info.atlas_read_offset),
                            make_surface_info.atlas_read_dim,
                            properties))
    {
        recordActiveSurfaceVoxel(uint3(dispatchThreadId) % SURFACE_GROUP_SIZE);
        surface.Store(uint3(voxelIndex), properties);
    }

    GroupMemoryBarrierWithGroupSync();
    flushSurfaceWorkgroupAggregation(workgroupBase, localInvocationIndex);
}
