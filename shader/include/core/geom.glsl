#ifndef GEOM_GLSL
#define GEOM_GLSL

// input: theta is the azimuthal angle
//        phi is the polar angle (from the y-axis)
vec3 getSphericalDir(float theta, float phi) {
  float sinPhi = sin(phi);
  return vec3(sinPhi * sin(theta), cos(phi), sinPhi * cos(theta));
}

#endif // GEOM_GLSL
