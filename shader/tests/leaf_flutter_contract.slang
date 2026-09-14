import leaf_flutter;
import vegetation_response_solver;

export __extern_cpp int main()
{
    float4 a = float4(0.0);
    float4 b = float4(0.0);
    float peak = 0.0;
    float lateVariation = 0.0;
    float previous = 0.0;
    float independent = 0.0;
    for (uint frame = 0; frame < 600u; ++frame)
    {
        advanceLeafTorsion(a, float2(0.3, 0.0), 1.0 / 60.0, 11u, 1.0, (float(frame) + 0.5) / 60.0);
        advanceLeafTorsion(b, float2(0.3, 0.0), 1.0 / 60.0, 47u, 1.0, (float(frame) + 0.5) / 60.0);
        peak = max(peak, abs(a.x));
        independent = max(independent, abs(a.x - b.x));
        if (frame > 300u) lateVariation = max(lateVariation, abs(a.x - previous));
        previous = a.x;
        if (length(leafLocalOffset(11u, a.x)) > 0.5) return 1;
        if (abs(length(leafOpticalNormal(11u, a.x)) - 1.0) > 0.0001) return 2;
    }
    for (uint frame = 0; frame < 600u; ++frame)
        advanceLeafTorsion(a, float2(0.0), 1.0 / 60.0, 11u, 1.0, 10.0 + (float(frame) + 0.5) / 60.0);
    float3 base = float3(0.1, 0.3, 0.04);
    float3 view = normalize(float3(1.0, 0.4, 0.1));
    float3 light = normalize(float3(-0.8, 1.0, 0.2));
    float3 n = leafOpticalNormal(11u, 0.25);
    float3 front = leafOpticalColor(base, float3(0.15), float3(1.0), 1.0, view, light, n, 0.6);
    float3 back = leafOpticalColor(base, float3(0.15), float3(1.0), 1.0, view, light, -n, 0.6);
    float3 dark = leafOpticalColor(base, float3(0.0), float3(1.0), 0.0, view, light, n, 0.6);
    float3 turned = leafOpticalColor(base, float3(0.15), float3(1.0), 1.0, view, light, leafOpticalNormal(11u, -0.25), 0.6);
    printf("peak_angle=%.5f continuous_variation=%.5f independent=%.5f quiet_angle=%.8f side_color_delta=%.5f turn_color_delta=%.5f\n", peak, lateVariation, independent, a.x, length(front-back), length(front-turned));
    // Sustained wind must keep driving local flutter; calm must still settle.
    return peak > 0.005 && peak < 1.0 && lateVariation > 0.001 && independent > 0.01 && abs(a.x) < 0.00001
        && length(front-back) > 0.001 && length(front-turned) > 0.001 && all(dark == float3(0.0)) ? 0 : 3;
}
