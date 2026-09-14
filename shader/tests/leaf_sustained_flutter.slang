import leaf_flutter;
import vegetation_response_solver;

export __extern_cpp int main()
{
    float4 coarse = float4(0.0);
    float4 fine = float4(0.0);
    float low = 1.0;
    float high = -1.0;
    float error = 0.0;
    uint reversals = 0u;
    float previousVelocity = 0.0;
    for (uint frame = 0; frame < 3600u; ++frame)
    {
        float t = float(frame) / 60.0;
        advanceLeafTorsion(coarse, float2(0.6, 0.0), 1.0 / 60.0, 11u, 1.0, t + 0.5 / 60.0);
        for (uint substep = 0; substep < 2u; ++substep)
            advanceLeafTorsion(fine, float2(0.6, 0.0), 1.0 / 120.0, 11u, 1.0, t + (float(substep) + 0.5) / 120.0);
        error = max(error, abs(coarse.x - fine.x));
        if (abs(coarse.x) > 1.0 || length(leafLocalDisplacement(11u, coarse.x, 5.0)) > 5.00001) return 1;
        if (frame > 600u)
        {
            low = min(low, coarse.x);
            high = max(high, coarse.x);
            if (coarse.z * previousVelocity < 0.0) ++reversals;
        }
        previousVelocity = coarse.z;
    }
    float4 noWind = float4(0.0);
    float4 disabled = float4(0.0);
    for (uint frame = 0; frame < 600u; ++frame)
    {
        float t = 60.0 + (float(frame) + 0.5) / 60.0;
        advanceLeafTorsion(coarse, float2(0.0), 1.0 / 60.0, 11u, 1.0, t);
        advanceLeafTorsion(noWind, float2(0.0), 1.0 / 60.0, 47u, 1.0, t);
        advanceLeafTorsion(disabled, float2(10.0, 0.0), 1.0 / 60.0, 47u, 0.0, t);
    }
    printf("sustained_min=%.6f sustained_max=%.6f reversals=%u dt_error=%.8f calm=%.8f\n",
        low, high, reversals, error, coarse.x);
    return low < -0.02 && high > 0.02 && reversals > 20u && error < 0.01
        && abs(coarse.x) < 0.00001 && all(noWind == float4(0.0)) && all(disabled == float4(0.0)) ? 0 : 2;
}
