#ifndef SKYLIGHT_GLSL
#define SKYLIGHT_GLSL

// vec3 getRandomShadowRay(uvec3 seed) {
//     vec3 randInSphere = randomPointInSphere(seed);
//     return normalize(environmentUbo.data.sunDir + randInSphere * kTanSunAngleReal);
// }

vec3 get_sky_color(vec3 dir) { return vec3(0.5, 0.7, 1.0); }

#endif // SKYLIGHT_GLSL
