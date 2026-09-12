import leaf_particle_pose;
import leaf_lighting;
import tracer_types;

export __extern_cpp int main()
{
    U_SunInfo sun = {};
    sun.sun_dir = normalize(float3(1.0, 1.0, -1.0));
    sun.sun_color = float3(1.0);
    sun.sun_luminance = 1.0;
    if (LEAF_FLIGHT_BIT != (1u << 30u)) return 1;
    float3 axis = normalize(float3(1.0, 0.4, 0.2));
    float3 restNormal = float3(0.0, 0.0, 1.0);
    float3 center = float3(0.0, 1.0, 0.0);
    float3 firstColor = float3(0.0);
    float maxColorChange = 0.0;
    for (uint step = 0u; step <= 80u; ++step)
    {
        float angle = float(step) * 6.2831853 / 80.0;
        float4 q = float4(axis * sin(angle * 0.5), cos(angle * 0.5));
        float3 n = leafParticleOpticalNormal(q, LEAF_FLIGHT_BIT);
        // Independent axis-angle reference for the simulation-driven optical normal.
        float3 expected = restNormal * cos(angle) + cross(axis, restNormal) * sin(angle)
            + axis * dot(axis, restNormal) * (1.0 - cos(angle));
        if (length(n - expected) > 0.00001 || abs(length(n) - 1.0) > 0.00001) return 2;
        if (length(leafParticleOpticalNormal(-q, LEAF_FLIGHT_BIT) - n) > 0.00001) return 3;

        float3 rgb = shadeLeafWithEnvironment(float3(0.2, 0.4, 0.1), float3(0.1),
            1.0, center, n, float3(0.0, 1.0, 2.0), sun, 0.35);
        if (any(rgb < float3(0.0)) || any(isnan(rgb)) || any(isinf(rgb))) return 4;
        if (step == 0u) firstColor = rgb;
        maxColorChange = max(maxColorChange, length(rgb - firstColor));

        for (uint view = 0u; view < 3u; ++view)
        {
            float cameraAngle = float(view) * 1.1;
            float3 right = float3(cos(cameraAngle), 0.0, sin(cameraAngle));
            float3 up = float3(0.0, 1.0, 0.0);
            float3 p0 = screenFacingParticleOffset(float2(-0.5, -0.5), right, up);
            float3 p1 = screenFacingParticleOffset(float2(0.5, -0.5), right, up);
            float3 p2 = screenFacingParticleOffset(float2(0.5, 0.5), right, up);
            if (abs(length(p1-p0)-1.0) > 0.00001 || abs(length(p2-p1)-1.0) > 0.00001) return 5;
            if (abs(dot(p1-p0, p2-p1)) > 0.00001) return 6;
            if (length(cross(p1-p0, p2-p1) - cross(right, up)) > 0.00001) return 7;
            for (uint corner = 0u; corner < 4u; ++corner)
            {
                float2 offset = float2(float(corner & 1u), float((corner >> 1u) & 1u)) - 0.5;
                // Production geometry has no quaternion input and remains camera-facing.
                if (length(screenFacingParticleOffset(offset, right, up)
                    - (right * offset.x + up * offset.y)) > 0.00001) return 8;
                float3 cornerRgb = shadeLeafWithEnvironment(float3(0.2, 0.4, 0.1), float3(0.1),
                    1.0, center, leafParticleOpticalNormal(q, LEAF_FLIGHT_BIT),
                    float3(0.0, 1.0, 2.0), sun, 0.35);
                if (any(cornerRgb != rgb)) return 9;
            }
        }
    }
    float4 proxy = float4(0.0, 1.0, 0.0, 1.0);
    if (any(leafParticleOpticalNormal(proxy, 0u) != proxy.xyz)) return 10;
    if (maxColorChange < 0.01) return 11;
    printf("detached square: 81 optical poses, 3 camera bases, screen-facing geometry and shared changing RGB passed\n");
    return 0;
}
