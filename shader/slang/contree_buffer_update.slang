import contree_build;

[[vk::binding(0, 0)]]
GLSLShaderStorageBuffer<B_ContreeBuildState, Std430DataLayout> contree_build_state;

[[vk::binding(1, 0)]]
writeonly GLSLShaderStorageBuffer<B_LevelDispatchIndirect, Std430DataLayout>
    level_dispatch_indirect;

[shader("compute")]
[numthreads(1, 1, 1)]
void main()
{
    uint previousDimension = contree_build_state.curr_dim;
    uint currentDimension = previousDimension / 4u;

    contree_build_state.prev_dim = previousDimension;
    contree_build_state.curr_dim = currentDimension;
    contree_build_state.level = contreeLog4(currentDimension);

    uint dispatchCount = contreeGroupX(currentDimension, 4u);
    level_dispatch_indirect.dispatch_x = dispatchCount;
    level_dispatch_indirect.dispatch_y = dispatchCount;
    level_dispatch_indirect.dispatch_z = dispatchCount;
}
