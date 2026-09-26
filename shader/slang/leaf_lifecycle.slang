module leaf_lifecycle;

struct B_TreeLeafState { float growth[]; };
[[vk::binding(3, 1)]] readonly GLSLShaderStorageBuffer<B_TreeLeafState, Std430DataLayout> tree_leaf_state;

public float treeLeafGrowth(uint index) { return saturate(tree_leaf_state.growth[index]); }
