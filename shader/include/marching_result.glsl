#ifndef DDA_SCENE_MARCHING_RESULT_GLSL
#define DDA_SCENE_MARCHING_RESULT_GLSL

struct MarchingResult {
    uint iter_count;
    bool is_hit;
    bool is_normal_valid;
    vec3 pos;
    vec3 center_pos;
    float t;
    vec3 normal;
    uint voxel_type;
};

#endif // DDA_SCENE_MARCHING_RESULT_GLSL
