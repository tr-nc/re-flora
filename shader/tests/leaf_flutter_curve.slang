import leaf_flutter;
import wind_response_curve;

export __extern_cpp int main()
{
    if (windResponseCurve(0.1, 0.2, 1.2, 0.0) != 0.0) return 1;
    if (windResponseCurve(1.3, 0.2, 1.2, 0.0) != 1.0) return 2;
    if (abs(windResponseCurve(0.7, 0.2, 1.2, 0.0) - 0.5) > 0.00001) return 3;
    if (windResponseCurve(0.7, 0.2, 1.2, -1.0) <= 0.5) return 4;
    if (windResponseCurve(0.7, 0.2, 1.2, 1.0) >= 0.5) return 5;
    for (int knee = -6; knee <= 6; ++knee) {
        float previous = 0.0;
        for (int i = 0; i <= 400; ++i) {
            float value = windResponseCurve(float(i) * 0.01, 0.2, 1.2, float(knee));
            if (value < previous - 0.00001 || value < -0.00001 || value > 1.00001) return 6;
            previous = value;
        }
    }
    if (windResponseCurve(0.7, 1.2, 0.2, 0.0) != windResponseCurve(0.7, 0.2, 1.2, 0.0)) return 7;
    if (leafTorqueTarget(float2(0.0), float2(0.25), uint2(3u), 11u, 2.0, float4(0.0)) != 0.0) return 8;
    float4 curve = float4(0.2, 1.2, 0.0, 0.0);
    float half = leafTorqueTarget(float2(0.7, 0.0), float2(0.25), uint2(3u), 11u, 2.0, curve);
    float full = leafTorqueTarget(float2(1.2, 0.0), float2(0.25), uint2(3u), 11u, 2.0, curve);
    if (abs(half - full * 0.5) > 0.00001 || abs(full) < 0.00001) return 9;
    if (abs(half - 2.0 * leafTorqueTarget(float2(0.7, 0.0), float2(0.25), uint2(3u), 11u, 1.0, curve)) > 0.00001) return 10;
    if (windResponseCurve(0.4, 0.5, 0.5, 0.0) != 0.0 || windResponseCurve(0.5, 0.5, 0.5, 0.0) != 1.0) return 11;
    if (leafTorqueTarget(float2(0.1, 0.0), float2(0.25), uint2(3u), 11u, 2.0, curve) != 0.0) return 12;
    // Nonzero low endpoint allows a falling response, but calm stays silent.
    float4 falling = float4(0.2, 1.2, 0.0, 2.0);
    if (leafTorqueTarget(float2(0.0), float2(0.25), uint2(3u), 11u, 0.0, falling) != 0.0) return 13;
    float weak = leafTorqueTarget(float2(0.1, 0.0), float2(0.25), uint2(3u), 11u, 0.0, falling);
    float strong = leafTorqueTarget(float2(1.3, 0.0), float2(0.25), uint2(3u), 11u, 0.0, falling);
    if (abs(weak) < 0.00001 || strong != 0.0) return 14;
    // Old strength/2 is now the curve high endpoint: exact migrated forcing.
    for (uint i = 1u; i < 400u; ++i) {
        float speed = float(i) * 0.01;
        float response = windResponseCurve(speed, 0.5, 1.5, 0.0);
        float actual = leafTorqueTarget(float2(speed, 0.0), float2(0.25), uint2(3u), 11u, 1.0, float4(0.5, 1.5, 0.0, 0.0));
        float maximum = leafTorqueTarget(float2(speed, 0.0), float2(0.25), uint2(3u), 11u, 1.0, float4(0.0, 0.0, 0.0, 0.0));
        if (abs(actual - maximum * response) > 0.000001) return 15;
    }
    return 0;
}
