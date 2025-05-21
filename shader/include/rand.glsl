#ifndef RAND_GLSL
#define RAND_GLSL

// input: theta is the azimuthal angle
//        phi is the polar angle (from the y-axis)
vec3 get_spherical_dir(float theta, float phi) {
    float sin_phi = sin(phi);
    return vec3(sin_phi * sin(theta), cos(phi), sin_phi * cos(theta));
}

#endif // RAND_GLSL
