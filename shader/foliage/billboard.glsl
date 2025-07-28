#ifndef BILLBOARD_GLSL
#define BILLBOARD_GLSL

vec3 get_vert_pos_with_billboard(mat4 view_mat, vec3 voxel_pos, uvec3 vert_offset_in_vox,
                                 float scaling_factor) {
    // after this, the vert_offset_in_vox is in the range of [-0.5, 0.5] for each component
    vec2 mapped_vert_offset_in_vox = vec2(vert_offset_in_vox.xy) - 0.5;

    mapped_vert_offset_in_vox *= scaling_factor;

    // this constant is an approximation of sqrt(3/2) ≈ 1.225.
    // for a cube with side length A, its expected (average) projected area over all
    // possible random orientations is (3/2) * A^2. A square with this same area
    // would have a side length of sqrt(3/2) * A.
    // this scaling factor ensures that the 2D billboard's area statistically
    // matches the projected area of the 3D voxel it represents, making it appear
    // more volumetrically consistent from any viewing angle.
    const float multiplier = 1.225;

    mapped_vert_offset_in_vox *= multiplier;

    vec3 right = normalize(vec3(view_mat[0][0], view_mat[1][0], view_mat[2][0]));
    vec3 up    = normalize(vec3(view_mat[0][1], view_mat[1][1], view_mat[2][1]));

    return voxel_pos + right * mapped_vert_offset_in_vox.x + up * mapped_vert_offset_in_vox.y;
}

#endif // BILLBOARD_GLSL
