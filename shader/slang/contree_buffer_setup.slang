import contree_build;

[[vk::binding(0, 0)]]
ConstantBuffer<U_ContreeBuildInfo> contree_build_info;

[[vk::binding(1, 0)]]
writeonly GLSLShaderStorageBuffer<B_ContreeBuildState, Std430DataLayout> contree_build_state;

struct B_LevelDispatchIndirect
{
    uint dispatch_x;
    uint dispatch_y;
    uint dispatch_z;
};

[[vk::binding(2, 0)]]
writeonly GLSLShaderStorageBuffer<B_LevelDispatchIndirect, Std430DataLayout>
    level_dispatch_indirect;

[[vk::binding(3, 0)]]
writeonly GLSLShaderStorageBuffer<B_CounterForLevels, Std430DataLayout> counter_for_levels;

[[vk::binding(4, 0)]]
writeonly GLSLShaderStorageBuffer<B_NodeOffsetForLevels, Std430DataLayout>
    node_offset_for_levels;

[[vk::binding(5, 0)]]
writeonly GLSLShaderStorageBuffer<B_ContreeBuildResult, Std430DataLayout> contree_build_result;

[[vk::binding(6, 0)]]
readonly GLSLShaderStorageBuffer<B_MakeSurfaceResult, Std430DataLayout> make_surface_result;

uint groupX64(uint count)
{
    return (count + 63u) / 64u;
}

uint log4(uint value)
{
    return uint(firstbithigh(value)) >> 1u;
}

[shader("compute")]
[numthreads(1, 1, 1)]
void main()
{
    uint previousDimension = contree_build_info.dim;
    uint currentDimension = previousDimension / 4u;

    contree_build_state.prev_dim = previousDimension;
    contree_build_state.curr_dim = currentDimension;
    contree_build_state.level = log4(currentDimension);

    level_dispatch_indirect.dispatch_x = max(groupX64(make_surface_result.active_brick_len), 1u);
    level_dispatch_indirect.dispatch_y = 1u;
    level_dispatch_indirect.dispatch_z = 1u;

    for (uint level = 0u; level < contree_build_info.max_level - 1u; ++level)
    {
        counter_for_levels.data[level] = 0u;
    }

    uint currentOffset = 0u;
    uint levelSize = 1u;
    for (uint level = 0u; level < contree_build_info.max_level - 1u; ++level)
    {
        node_offset_for_levels.data[level] = currentOffset;
        currentOffset += levelSize;
        levelSize *= 64u;
    }

    contree_build_result.node_len = 0u;
    contree_build_result.leaf_len = 0u;
}
