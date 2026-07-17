import contree_build;
import contree_node;

static const uint BUFFER_SIZE = 10u;

[[vk::binding(0, 0)]]
ConstantBuffer<U_ContreeBuildInfo> contree_build_info;

[[vk::binding(1, 0)]]
readonly GLSLShaderStorageBuffer<B_NodeOffsetForLevels, Std430DataLayout>
    node_offset_for_levels;

[[vk::binding(2, 0)]]
readonly GLSLShaderStorageBuffer<B_DenseNodes, Std430DataLayout> dense_nodes;

[[vk::binding(3, 0)]]
readonly GLSLShaderStorageBuffer<B_CounterForLevels, Std430DataLayout> counter_for_levels;

[[vk::binding(4, 0)]]
writeonly GLSLShaderStorageBuffer<B_ContreeNodeData, Std430DataLayout> contree_node_data;

[[vk::binding(5, 0)]]
readonly GLSLShaderStorageBuffer<B_ContreeBuildResult, Std430DataLayout> contree_build_result;

[shader("compute")]
[numthreads(256, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint nodeIndex = dispatchThreadId.x;
    if (nodeIndex >= contree_build_result.node_len)
    {
        return;
    }

    uint upperBounds[BUFFER_SIZE];
    uint nodeLevelCount = contree_build_info.max_level - 1u;

    upperBounds[0] = counter_for_levels.data[0];
    for (uint level = 1u; level < nodeLevelCount; ++level)
    {
        upperBounds[level] = upperBounds[level - 1u] + counter_for_levels.data[level];
    }

    for (uint level = 0u; level < nodeLevelCount; ++level)
    {
        if (nodeIndex < upperBounds[level])
        {
            uint denseOffset = node_offset_for_levels.data[level];
            uint previousUpperBound = level == 0u ? 0u : upperBounds[level - 1u];
            uint localIndex = nodeIndex - previousUpperBound;
            ContreeNode fetched = dense_nodes.data[denseOffset + localIndex];

            uint relativeOffset = fetched.packed_0 >> 1u;
            uint absoluteOffset = relativeOffset + upperBounds[level];

            ContreeNode node;
            if (level == nodeLevelCount - 1u)
            {
                node.packed_0 = (fetched.packed_0 & 1u) | (relativeOffset << 1u);
            }
            else
            {
                node.packed_0 = (fetched.packed_0 & 1u) | (absoluteOffset << 1u);
            }
            node.child_mask_lo = fetched.child_mask_lo;
            node.child_mask_hi = fetched.child_mask_hi;

            contree_node_data.data[contree_build_info.node_write_offset + nodeIndex] = node;
            break;
        }
    }
}
