import leaf_particle_pose;
import leaf_lighting;
import tracer_types;

export __extern_cpp int main()
{
    U_SunInfo sun = {};
    sun.sun_dir = normalize(float3(1.0, 1.0, -1.0));
    sun.sun_color = float3(1.0);
    sun.sun_luminance = 1.0;
    for (uint step = 0u; step <= 80u; ++step)
    {
        float angle = float(step) * 6.2831853 / 80.0;
        float4 q = float4(normalize(float3(1.0, 0.4, 0.2)) * sin(angle * 0.5), cos(angle * 0.5));
        float3 n = rotateLeafPlate(q, float3(0.0, 0.0, 1.0));
        float3 p0 = rotateLeafPlate(q, float3(-0.5, -0.5, 0.0));
        float3 p1 = rotateLeafPlate(q, float3(0.5, -0.5, 0.0));
        float3 p2 = rotateLeafPlate(q, float3(0.5, 0.5, 0.0));
        if (abs(length(p1-p0)-1.0) > 0.00001 || abs(length(p2-p1)-1.0) > 0.00001) return 1;
        if (abs(dot(p1-p0, p2-p1)) > 0.00001) return 2;
        if (length(cross(p1-p0, p2-p1)-n) > 0.00001) return 3;
        if (length(rotateLeafPlate(-q, float3(0.0, 0.0, 1.0))-n) > 0.00001) return 4;
        float3 center = float3(0.0, 1.0, 0.0);
        float3 rgb = shadeLeafWithEnvironment(float3(0.2, 0.4, 0.1), float3(0.1),
            1.0, center, n, float3(0.0, 1.0, 2.0), sun, 0.35);
        // Both geometric sides and all corners consume this one center evaluation.
        if (any(rgb < float3(0.0)) || any(isnan(rgb)) || any(isinf(rgb))) return 5;
    }
    printf("detached square: 81 poses, equal sides, orthogonal edges, shared optical normal passed\n");
    return 0;
}
