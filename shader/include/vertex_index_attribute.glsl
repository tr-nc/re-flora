#ifndef VERTEX_INDEX_ATTRIBUTE_GLSL
#define VERTEX_INDEX_ATTRIBUTE_GLSL

struct Vertex {
    vec3 position;
    uint compressed_voxel_type_and_normal;
};

struct Index {
    uint index;
};

#endif // VERTEX_INDEX_ATTRIBUTE_GLSL
