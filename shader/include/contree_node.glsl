#ifndef CONTREE_NODE_GLSL
#define CONTREE_NODE_GLSL

#extension GL_ARB_gpu_shader_int64 : enable

struct ContreeNode {
    uint packed_0; // [0]=is_leaf, [1..31]=child_ptr
    uint64_t child_mask;
};

#endif // CONTREE_NODE_GLSL
