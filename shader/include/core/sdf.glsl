//! https://iquilezles.org/articles/distfunctions/

#ifndef SDF_GLSL
#define SDF_GLSL

float dot2(in vec2 v) { return dot(v, v); }
float dot2(in vec3 v) { return dot(v, v); }
float ndot(in vec2 a, in vec2 b) { return a.x * b.x - a.y * b.y; }

/// Round cone, untransformed, the center of r1 is at (0, 0, 0) and the center of r2 is at (0, h,
/// 0).
float sd_round_cone(vec3 p, float r1, float r2, float h) {
    // sampling independent computations (only depend on shape)
    float b = (r1 - r2) / h;
    float a = sqrt(1.0 - b * b);

    // sampling dependant computations
    vec2 q  = vec2(length(p.xz), p.y);
    float k = dot(q, vec2(-b, a));
    if (k < 0.0) return length(q) - r1;
    if (k > a * h) return length(q - vec2(0.0, h)) - r2;
    return dot(q, vec2(a, b)) - r1;
}

/// Round cone, the center of r1 is at a and the center of r2 is at b.
float sd_round_cone(vec3 p, vec3 a, vec3 b, float r1, float r2) {
    // sampling independent computations (only depend on shape)
    vec3 ba   = b - a;
    float l2  = dot(ba, ba);
    float rr  = r1 - r2;
    float a2  = l2 - rr * rr;
    float il2 = 1.0 / l2;

    // sampling dependant computations
    vec3 pa  = p - a;
    float y  = dot(pa, ba);
    float z  = y - l2;
    float x2 = dot2(pa * l2 - ba * y);
    float y2 = y * y * l2;
    float z2 = z * z * l2;

    // single square root!
    float k = sign(rr) * rr * rr * x2;
    if (sign(z) * a2 * z2 > k) return sqrt(x2 + z2) * il2 - r2;
    if (sign(y) * a2 * y2 < k) return sqrt(x2 + y2) * il2 - r1;
    return (sqrt(x2 * a2 * il2) + y * rr) * il2 - r1;
}

#endif // SDF_GLSL
