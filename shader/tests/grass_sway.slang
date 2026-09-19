import vegetation_response_solver;
import leaf_flutter;
import grass_sway_sample;

export __extern_cpp int main()
{
    float4 amplitude = float4(0.0, 2.0, 0.05, 2.0);
    float4 frequency = float4(1.0, 1.0, 0.05, 2.0);
    float4 state = float4(0.0);
    float2 phase = float2(0.0);
    uint2 cells = uint2(0u);
    float low = 100.0, high = -100.0;
    for (uint i = 0u; i < 2400u; ++i) {
        advanceGrassSway(state, phase, cells, float2(2.0, 0.0), 1.0 / 120.0, 23u, amplitude, frequency, float4(0.0));
        if (any(!isfinite(state)) || length(state.xy) > 2.00001) return 1;
        float2 local = grassLocalResponse(state, phase, cells, 23u);
        if (length(local) > 2.00001) return 7;
        if (i > 1200u) { low = min(low, local.x); high = max(high, local.x); }
    }
    if (high - low < 0.05) return 2; // constant mean wind must still supply fine forcing
    for (uint i = 0u; i < 2400u; ++i)
        advanceGrassSway(state, phase, cells, float2(0.0), 1.0 / 120.0, 23u, amplitude, frequency, float4(0.0));
    if (length(state.xy) > 0.00001 || length(state.zw) > 0.00001) return 3;
    float4 slow = float4(0.0), fast = float4(0.0);
    float2 ps = float2(0.0), pf = float2(0.0);
    uint2 cs = uint2(0u), cf = uint2(0u);
    for (uint i = 0u; i < 1200u; ++i) {
        advanceGrassSway(slow, ps, cs, float2(1.0), 1.0 / 120.0, 13u, amplitude, frequency, float4(0.0));
        for (uint j = 0u; j < 2u; ++j)
            advanceGrassSway(fast, pf, cf, float2(1.0), 1.0 / 240.0, 13u, amplitude, frequency, float4(0.0));
    }
    if (length(slow.xy-fast.xy) > 0.002) return 4;
    if (length(grassLocalResponse(slow,ps,cs,13u)-grassLocalResponse(fast,pf,cf,13u)) > 0.002) return 8;
    // Retuning the frequency at zero dt does not teleport the plant or its clock.
    float4 saved = slow;
    float2 savedPhase = ps;
    advanceGrassSway(slow, ps, cs, float2(1.0), 0.0, 13u, amplitude, float4(24.0,24.0,0.0,1.0), float4(0.0));
    if (length(slow-saved) > 0.000001 || any(ps != savedPhase)) return 5;
    // Frequency ceiling 1 Hz is not silently clamped to an old multiplier minimum.
    if (abs(leafFlutterFrequency(float2(4.0,0.0), float4(24.0,24.0,1.0/24.0,0.0), float4(0.0,4.0,0.0,0.0))-1.0) > 0.000001) return 6;
    return 0;
}
