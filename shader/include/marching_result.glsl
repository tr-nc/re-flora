#ifndef DDA_SCENE_MARCHING_RESULT_GLSL
#define DDA_SCENE_MARCHING_RESULT_GLSL

struct MarchingResult {
    uint iter_count;
    bool is_hit;
    vec3 pos;
    vec3 center_pos;
    float t;
    vec3 normal;
    uint voxel_type;
    uint hash_id;
    uint voxel_addr;
};

#endif // DDA_SCENE_MARCHING_RESULT_GLSL
