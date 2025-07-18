#ifndef TRANSFORM_GLSL
#define TRANSFORM_GLSL

/// Constructs a TBN matrix, given that world up is y-axis
/// TBN is orthonormal basis, so transpose is the same as inverse
/// This is used to transform a vector from tangent space to world space
/// Usage: TBN * <tangent_space_vector> = <world_space_vector>
/// Usage: transpose(TBN) * <world_space_vector> = <tangent_space_vector>
mat3 make_tbn(vec3 normal) {
    vec3 up = abs(normal.y) < 0.999 ? vec3(0, 1, 0) : vec3(1, 0, 0);
    vec3 t  = normalize(cross(up, normal));
    vec3 b  = cross(normal, t);
    return mat3(t, b, normal);
}

#endif // TRANSFORM_GLSL
