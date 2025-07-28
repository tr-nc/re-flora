#ifndef BILLBOARD_GLSL
#define BILLBOARD_GLSL

vec3 get_vert_pos_with_billboard(mat4 view_mat, vec3 voxel_pos, uvec3 vert_offset_in_vox,
                                 float scaling_factor) {
    vec3 mapped_vert_offset_in_vox = vec3(vert_offset_in_vox) - 0.5;
    mapped_vert_offset_in_vox *= scaling_factor;

    vec3 right = normalize(vec3(view_mat[0][0], view_mat[1][0], view_mat[2][0]));
    vec3 up    = normalize(vec3(view_mat[0][1], view_mat[1][1], view_mat[2][1]));

    return voxel_pos + right * mapped_vert_offset_in_vox.x + up * mapped_vert_offset_in_vox.y;
}

#endif // BILLBOARD_GLSL
