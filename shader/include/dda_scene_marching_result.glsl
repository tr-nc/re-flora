#ifndef DDA_SCENE_MARCHING_RESULT_GLSL
#define DDA_SCENE_MARCHING_RESULT_GLSL

struct DdaSceneMarchingResult {
    uint iter_count;
    bool is_hit;
    vec3 pos;
    vec3 center_pos;
    float t;
    vec3 normal;
    uint voxel_data;
};

#endif // DDA_SCENE_MARCHING_RESULT_GLSL
