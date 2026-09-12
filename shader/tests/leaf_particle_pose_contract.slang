import leaf_particle_pose;
import leaf_lighting;
import tracer_types;

export __extern_cpp int main()
{
    U_SunInfo sun = {};
    sun.sun_dir = normalize(float3(1.0, 1.0, -1.0));
    sun.sun_color = float3(1.0);
    sun.sun_luminance = 1.0;
    // Keep the CPU upload and shader flag ABI explicit; no instance fields were added.
    if (LEAF_FLIGHT_BIT != (1u << 30u) || LEAF_ROTATING_PLATE_BIT != (1u << 29u)) return 6;
    uint rotatingFlags = LEAF_FLIGHT_BIT | LEAF_ROTATING_PLATE_BIT;
    float3 right = normalize(float3(0.7, 0.0, 0.7));
    float3 up = float3(0.0, 1.0, 0.0);
    float3 firstColor = float3(0.0);
    float maxColorChange = 0.0;
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
        if (step == 0u) firstColor = rgb;
        maxColorChange = max(maxColorChange, length(rgb - firstColor));
        for (uint corner = 0u; corner < 4u; ++corner)
        {
            float2 offset = float2(float(corner & 1u), float((corner >> 1u) & 1u)) - 0.5;
            LeafParticlePose billboard = evaluateLeafParticlePose(q, LEAF_FLIGHT_BIT,
                offset, right, up);
            LeafParticlePose plate = evaluateLeafParticlePose(q, rotatingFlags,
                offset, right, up);
            if (!billboard.hasFlightPose || !plate.hasFlightPose) return 7;
            // Production selector: B geometry can ignore q, B optics must never do so.
            if (length(billboard.vertexOffset - (right * offset.x + up * offset.y)) > 0.00001) return 8;
            if (length(plate.vertexOffset - rotateLeafPlate(q, float3(offset, 0.0))) > 0.00001) return 9;
            if (length(billboard.opticalNormal - n) > 0.00001
                || length(plate.opticalNormal - n) > 0.00001) return 10;
            float3 billboardRgb = shadeLeafWithEnvironment(float3(0.2, 0.4, 0.1), float3(0.1),
                1.0, center, billboard.opticalNormal, float3(0.0, 1.0, 2.0), sun, 0.35);
            float3 plateRgb = shadeLeafWithEnvironment(float3(0.2, 0.4, 0.1), float3(0.1),
                1.0, center, plate.opticalNormal, float3(0.0, 1.0, 2.0), sun, 0.35);
            if (any(billboardRgb != plateRgb) || any(billboardRgb != rgb)) return 11;
            // A ignores the inactive B geometry preference and retains proxy optics.
            float4 proxy = float4(0.0, 1.0, 0.0, 1.0);
            LeafParticlePose legacy = evaluateLeafParticlePose(proxy, LEAF_ROTATING_PLATE_BIT,
                offset, right, up);
            if (legacy.hasFlightPose || any(legacy.opticalNormal != proxy.xyz)
                || any(legacy.vertexOffset != billboard.vertexOffset)) return 12;
        }
    }
    if (maxColorChange < 0.01) return 13;
    printf("detached square: 81 poses, billboard/plate selector, shared changing RGB at all corners, legacy isolation passed\n");
    return 0;
}
