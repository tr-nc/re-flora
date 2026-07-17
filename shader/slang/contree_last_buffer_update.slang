import contree_build;

[[vk::binding(0, 0)]]
GLSLShaderStorageBuffer<B_ContreeBuildResult, Std430DataLayout> contree_build_result;

[[vk::binding(1, 0)]]
writeonly GLSLShaderStorageBuffer<B_ConcatDispatchIndirect, Std430DataLayout>
    concat_dispatch_indirect;

[[vk::binding(2, 0)]]
GLSLShaderStorageBuffer<B_SparseNodes, Std430DataLayout> sparse_nodes;

[[vk::binding(3, 0)]]
writeonly GLSLShaderStorageBuffer<B_DenseNodes, Std430DataLayout> dense_nodes;

[[vk::binding(4, 0)]]
writeonly GLSLShaderStorageBuffer<B_CounterForLevels, Std430DataLayout> counter_for_levels;

[shader("compute")]
[numthreads(1, 1, 1)]
void main()
{
    dense_nodes.data[0] = sparse_nodes.data[0];

    counter_for_levels.data[0] = 1u;
    contree_build_result.node_len += 1u;

    concat_dispatch_indirect.dispatch_x = contreeGroupX(contree_build_result.node_len, 256u);
    concat_dispatch_indirect.dispatch_y = 1u;
    concat_dispatch_indirect.dispatch_z = 1u;
}
