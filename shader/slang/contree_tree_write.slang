import bits;
import contree_build;
import contree_node;

static const uint WORKGROUP_SIZE = 64u;

[[vk::binding(0, 0)]]
readonly GLSLShaderStorageBuffer<B_ContreeBuildState, Std430DataLayout> contree_build_state;

[[vk::binding(1, 0)]]
readonly GLSLShaderStorageBuffer<B_NodeOffsetForLevels, Std430DataLayout>
    node_offset_for_levels;

[[vk::binding(2, 0)]]
GLSLShaderStorageBuffer<B_SparseNodes, Std430DataLayout> sparse_nodes;

[[vk::binding(3, 0)]]
writeonly GLSLShaderStorageBuffer<B_DenseNodes, Std430DataLayout> dense_nodes;

[[vk::binding(4, 0)]]
GLSLShaderStorageBuffer<B_CounterForLevels, Std430DataLayout> counter_for_levels;

[[vk::binding(5, 0)]]
GLSLShaderStorageBuffer<B_ContreeBuildResult, Std430DataLayout> contree_build_result;

groupshared uint shared_invocation_data[WORKGROUP_SIZE];
groupshared uint group_base;

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

        uint previousNodeLength;
        InterlockedAdd(contree_build_result.node_len, currentPrefixSum, previousNodeLength);
        InterlockedAdd(counter_for_levels.data[contree_build_state.level + 1u],
                       currentPrefixSum,
                       group_base);
    }

    GroupMemoryBarrierWithGroupSync();
    return group_base + shared_invocation_data[localInvocationIndex];
}

[shader("compute")]
[numthreads(4, 4, 4)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID,
          uint localInvocationIndex : SV_GroupIndex)
{
    cleanupSharedInvocationData(localInvocationIndex);

    // Inactive edge invocations contribute zero but must reach the prefix-allocation barriers.
    bool active = all(dispatchThreadId < contree_build_state.curr_dim);
    ContreeNode temporaryNodes[64];
    uint childMaskLow = 0u;
    uint childMaskHigh = 0u;

    if (active)
    {
        for (uint y = 0u; y < 4u; ++y)
        {
            for (uint z = 0u; z < 4u; ++z)
            {
                for (uint x = 0u; x < 4u; ++x)
                {
                    uint childIndex = x + z * 4u + y * 16u;
                    uint3 voxelPosition = dispatchThreadId * 4u + uint3(x, y, z);

                    uint previousSparseNodeOffset =
                        node_offset_for_levels.data[contree_build_state.level + 1u];
                    uint previousIndex = voxelPosition.x +
                                         voxelPosition.z * contree_build_state.prev_dim +
                                         voxelPosition.y * contree_build_state.prev_dim *
                                             contree_build_state.prev_dim;
                    ContreeNode node = sparse_nodes.data[previousSparseNodeOffset + previousIndex];
                    temporaryNodes[childIndex] = node;

                    bool exists = node.child_mask_lo != 0u || node.child_mask_hi != 0u ||
                                  (node.packed_0 & 1u) != 0u;
                    if (exists)
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
    }

    uint childCount = bitCountDual(childMaskLow, childMaskHigh);
    uint base = registerAndGetLocalBase(childCount, localInvocationIndex);
    if (!active)
    {
        return;
    }

    uint sparseNodeWriteOffset = node_offset_for_levels.data[contree_build_state.level];
    uint currentDimension = contree_build_state.curr_dim;
    uint brickIndex = dispatchThreadId.x + dispatchThreadId.z * currentDimension +
                      dispatchThreadId.y * currentDimension * currentDimension;
    brickIndex += sparseNodeWriteOffset;

    if (childCount == 0u)
    {
        sparse_nodes.data[brickIndex].packed_0 = 0u;
        sparse_nodes.data[brickIndex].child_mask_lo = 0u;
        sparse_nodes.data[brickIndex].child_mask_hi = 0u;
        return;
    }

    uint denseNodeOffset = node_offset_for_levels.data[contree_build_state.level + 1u];
    uint writeIndex = 0u;
    for (uint childIndex = 0u; childIndex < 64u; ++childIndex)
    {
        bool isSet = childIndex < 32u
            ? (childMaskLow & (1u << childIndex)) != 0u
            : (childMaskHigh & (1u << (childIndex - 32u))) != 0u;
        if (isSet)
        {
            dense_nodes.data[denseNodeOffset + base + writeIndex] = temporaryNodes[childIndex];
            ++writeIndex;
        }
    }

    sparse_nodes.data[brickIndex].packed_0 = base << 1u;
    sparse_nodes.data[brickIndex].child_mask_lo = childMaskLow;
    sparse_nodes.data[brickIndex].child_mask_hi = childMaskHigh;
}
