import grass_sway_sample;
import leaf_flutter;

export __extern_cpp int main()
{
    // Two stems sharing identical wind/grid state must not share local motion.
    float4 motion = float4(0.5, 0.1, 0.0, 0.0);
    float2 phase = float2(0.31, 0.73);
    uint2 cells = uint2(17u, 41u);
    float2 a = grassLocalResponse(motion, phase, cells, 13u);
    float2 b = grassLocalResponse(motion, phase, cells, 29u);
    if (length(a-b) < 0.01) return 1;
    if (any(a != grassLocalResponse(motion, phase, cells, 13u))) return 2;
    // A whole neighbourhood must have independent traces, not merely a shared
    // displacement rotated in space. Use identical grid clocks for every stem.
    float totalDifference = 0.0;
    for (uint i = 0u; i < 240u; ++i) {
        advanceFlutterClock(phase, cells, float2(0.025, 0.05));
        a = grassLocalResponse(motion, phase, cells, 13u);
        for (uint seed = 14u; seed < 30u; ++seed)
            totalDifference += length(a-grassLocalResponse(motion,phase,cells,seed));
    }
    if (totalDifference < 100.0) return 3;
    // Fraction/cell wrap remains continuous, including uint overflow.
    phase = float2(0.999999);
    cells = uint2(0xffffffffu);
    a = grassLocalResponse(motion,phase,cells,13u);
    advanceFlutterClock(phase,cells,float2(0.000002));
    b = grassLocalResponse(motion,phase,cells,13u);
    if (length(a-b) > 0.0001) return 4;
    return 0;
}
