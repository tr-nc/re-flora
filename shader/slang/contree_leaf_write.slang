import bits;
import contree_build;
import contree_node;

static const uint WORKGROUP_SIZE = 64u;

[[vk::binding(0, 0)]]
ConstantBuffer<U_ContreeBuildInfo> contree_build_info;

[[vk::binding(1, 0)]]
readonly GLSLShaderStorageBuffer<B_ContreeBuildState, Std430DataLayout> contree_build_state;

[[vk::binding(2, 0)]]
[[vk::image_format("r32ui")]]
readonly RWTexture3D<uint> surface;

[[vk::binding(3, 0)]]
readonly GLSLShaderStorageBuffer<B_NodeOffsetForLevels, Std430DataLayout>
    node_offset_for_levels;

[[vk::binding(4, 0)]]
writeonly GLSLShaderStorageBuffer<B_SparseNodes, Std430DataLayout> sparse_nodes;

struct B_ContreeLeafData
{
    uint data[];
};

[[vk::binding(5, 0)]]
writeonly GLSLShaderStorageBuffer<B_ContreeLeafData, Std430DataLayout> contree_leaf_data;

[[vk::binding(6, 0)]]
GLSLShaderStorageBuffer<B_ContreeBuildResult, Std430DataLayout> contree_build_result;

[[vk::binding(7, 0)]]
readonly GLSLShaderStorageBuffer<B_MakeSurfaceResult, Std430DataLayout> make_surface_result;

struct B_SurfaceActiveBrickIndices
{
    uint data[];
};

[[vk::binding(8, 0)]]
readonly GLSLShaderStorageBuffer<B_SurfaceActiveBrickIndices, Std430DataLayout>
    surface_active_brick_indices;

struct B_SurfaceLeafCoords
{
    uint data[];
};

[[vk::binding(9, 0)]]
writeonly GLSLShaderStorageBuffer<B_SurfaceLeafCoords, Std430DataLayout> surface_leaf_coords;

groupshared uint shared_invocation_data[WORKGROUP_SIZE];
groupshared uint group_base;

uint packSurfaceLeafLocalCoord(int3 coordinate)
{
    return (uint(coordinate.x) & 0x3FFu) |
           ((uint(coordinate.y) & 0x3FFu) << 10u) |
           ((uint(coordinate.z) & 0x3FFu) << 20u);
}

void cleanupSharedInvocationData(uint localInvocationIndex)
{
    shared_invocation_data[localInvocationIndex] = 0u;
    GroupMemoryBarrierWithGroupSync();
}

uint registerAndGetLocalBase(uint invocationCount, uint localInvocationIndex)
{
    shared_invocation_data[localInvocationIndex] = invocationCount;
    GroupMemoryBarrierWithGroupSync();

    if (localInvocationIndex == 0u)
    {
        uint currentPrefixSum = 0u;
        for (uint index = 0u; index < WORKGROUP_SIZE; ++index)
        {
            uint countAtIndex = shared_invocation_data[index];
            shared_invocation_data[index] = currentPrefixSum;
            currentPrefixSum += countAtIndex;
        }
        InterlockedAdd(contree_build_result.leaf_len, currentPrefixSum, group_base);
    }

    GroupMemoryBarrierWithGroupSync();
    return group_base + shared_invocation_data[localInvocationIndex];
}

[shader("compute")]
[numthreads(WORKGROUP_SIZE, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID,
          uint localInvocationIndex : SV_GroupIndex)
{
    cleanupSharedInvocationData(localInvocationIndex);

    uint activeBrickListIndex = dispatchThreadId.x;
    if (activeBrickListIndex >= make_surface_result.active_brick_len)
    {
        return;
    }

    uint activeBrickIndex = surface_active_brick_indices.data[activeBrickListIndex];
    uint brickDimension = contree_build_state.curr_dim;
    int3 brickIndex3D = int3(int(activeBrickIndex % brickDimension),
                             int(activeBrickIndex / (brickDimension * brickDimension)),
                             int((activeBrickIndex / brickDimension) % brickDimension));

    uint temporaryVoxels[64];
    uint childMaskLow = 0u;
    uint childMaskHigh = 0u;
    for (uint y = 0u; y < 4u; ++y)
    {
        for (uint z = 0u; z < 4u; ++z)
        {
            for (uint x = 0u; x < 4u; ++x)
            {
                uint childIndex = x + z * 4u + y * 16u;
                int3 voxelPosition = brickIndex3D * 4 + int3(x, y, z);
                uint voxel = surface[uint3(voxelPosition)];
                temporaryVoxels[childIndex] = voxel;

                if (voxel != 0u)
                {
                    if (childIndex < 32u)
                    {
                        childMaskLow |= 1u << childIndex;
                    }
                    else
                    {
                        childMaskHigh |= 1u << (childIndex - 32u);
                    }
                }
            }
        }
    }

    uint childCount = bitCountDual(childMaskLow, childMaskHigh);
    uint base = registerAndGetLocalBase(childCount, localInvocationIndex);

    uint sparseNodeWriteOffset = node_offset_for_levels.data[contree_build_state.level];
    uint brickIndex = uint(brickIndex3D.x) + uint(brickIndex3D.z) * brickDimension +
                      uint(brickIndex3D.y) * brickDimension * brickDimension;
    brickIndex += sparseNodeWriteOffset;

    if (childCount == 0u)
    {
        sparse_nodes.data[brickIndex].packed_0 = 0u;
        sparse_nodes.data[brickIndex].child_mask_lo = 0u;
        sparse_nodes.data[brickIndex].child_mask_hi = 0u;
        return;
    }

    uint writeIndex = 0u;
    for (uint childIndex = 0u; childIndex < 64u; ++childIndex)
    {
        bool isSet = childIndex < 32u
            ? (childMaskLow & (1u << childIndex)) != 0u
            : (childMaskHigh & (1u << (childIndex - 32u))) != 0u;
        if (isSet)
        {
            uint globalLeafIndex = contree_build_info.leaf_write_offset + base + writeIndex;
            contree_leaf_data.data[globalLeafIndex] = temporaryVoxels[childIndex];

            uint x = childIndex & 3u;
            uint z = (childIndex >> 2u) & 3u;
            uint y = (childIndex >> 4u) & 3u;
            int3 localVoxelPosition = brickIndex3D * 4 + int3(x, y, z);
            surface_leaf_coords.data[globalLeafIndex] =
                packSurfaceLeafLocalCoord(localVoxelPosition);
            ++writeIndex;
        }
    }

    sparse_nodes.data[brickIndex].packed_0 = 1u | (base << 1u);
    sparse_nodes.data[brickIndex].child_mask_lo = childMaskLow;
    sparse_nodes.data[brickIndex].child_mask_hi = childMaskHigh;
}
