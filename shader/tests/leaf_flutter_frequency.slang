import leaf_flutter;
import vegetation_response_solver;
import wind_response_curve;

export __extern_cpp int main()
{
    float4 amp = float4(0.05, 1.0, 0.0, 0.0);
    float4 curve = float4(0.2, 1.2, 0.0, 0.0);
    float4 tuning = float4(2.0, 4.0, 1.0, 0.0);
    if (leafFlutterFrequency(float2(0.0), tuning, curve) != 2.0) return 1;
    if (leafFlutterFrequency(float2(2.0, 0.0), tuning, curve) != 4.0) return 2;
    if (abs(leafFlutterFrequency(float2(0.7, 0.0), tuning, curve) - 3.0) > 0.00001) return 3;
    tuning.y = 1.0;
    if (leafFlutterFrequency(float2(2.0, 0.0), tuning, curve) != 1.0) return 4;
    tuning.z = 2.0;
    if (leafFlutterFrequency(float2(0.0), tuning, curve) != 4.0 || leafFlutterFrequency(float2(2.0, 0.0), tuning, curve) != 2.0) return 12;
    // Migrated user settings reproduce the old base * wind multiplier curve.
    for (uint i = 0u; i <= 400u; ++i) {
        float wind = float(i) * 0.01;
        float expected = 1.5 * (1.0 + windResponseCurve(wind, 0.05, 1.0, 0.8));
        float actual = leafFlutterFrequency(float2(wind, 0.0), float4(1.5, 3.0, 1.0, 0.0), float4(0.05, 1.0, 0.8, 0.0));
        if (abs(expected - actual) > 0.000001) return 13;
    }

    // Identical normalized time must yield identical motion across an octave.
    // This catches changing only the noise clock or only mechanical stiffness.
    float4 slow = float4(0.0), fast = float4(0.0);
    float2 ps = float2(0.0), pf = float2(0.0);
    uint2 cs = uint2(0u), cf = uint2(0u);
    float maximumError = 0.0;
    for (uint i = 0u; i < 4096u; ++i) {
        advanceLeafTorsion(slow, ps, cs, float2(1.0, 0.0), 1.0 / 256.0, 11u, 1.0, amp, float4(2.0, 2.0, 1.0, 0.0));
        advanceLeafTorsion(fast, pf, cf, float2(1.0, 0.0), 1.0 / 512.0, 11u, 1.0, amp, float4(4.0, 4.0, 1.0, 0.0));
        maximumError = max(maximumError, abs(slow.x - fast.x));
    }
    if (maximumError > 0.00001 || any(cs != cf) || any(abs(ps - pf) > 0.00001)) return 5;

    // Swept output spectra, not merely uploaded tuning values. Each run covers
    // the same normalized time so the bounded envelope should stay unchanged.
    float referenceRms = 0.0, referencePeakHz = 0.0;
    for (uint run = 0u; run < 4u; ++run) {
        float hz = run == 3u ? 24.0 : 1.5 * exp2(float(run));
        float dt = 1.0 / (hz * 64.0);
        float4 state = float4(0.0);
        float2 phase = float2(0.0);
        uint2 cells = uint2(0u);
        float samples[2048];
        float sumSquares = 0.0;
        for (uint i = 0u; i < 2304u; ++i) {
            float4 f = run == 3u ? float4(24.0, 24.0, 1.0, 0.0) : float4(hz, hz, 1.0, 0.0);
            advanceLeafTorsion(state, phase, cells, float2(1.0, 0.0), dt, 11u, 1.0, amp, f);
            if (abs(state.x) > 1.0) return 6;
            if (i >= 256u) { samples[i - 256u] = state.x; sumSquares += state.x * state.x; }
        }
        float peakPower = -1.0, peakHz = 0.0;
        for (uint bin = 1u; bin <= 64u; ++bin) {
            float coefficient = 2.0 * cos(6.28318530718 * float(bin) / 2048.0);
            float a = 0.0, b = 0.0;
            for (uint i = 0u; i < 2048u; ++i) {
                float next = samples[i] + coefficient * a - b;
                b = a; a = next;
            }
            float power = a * a + b * b - coefficient * a * b;
            if (power > peakPower) { peakPower = power; peakHz = float(bin) / (2048.0 * dt); }
        }
        float rms = sqrt(sumSquares / 2048.0);
        if (run == 0u) { referenceRms = rms; referencePeakHz = peakHz; }
        if (abs(rms - referenceRms) > 0.0001 || abs(peakHz / hz - referencePeakHz / 1.5) > 0.001) return 7;
        printf("target_hz=%.2f measured_spectral_peak_hz=%.4f rms_angle=%.6f\n", hz, peakHz, rms);
    }

    // Changing settings at zero elapsed time must not change angle or phase.
    float4 before = slow;
    float2 beforePhase = ps;
    uint2 beforeCells = cs;
    advanceLeafTorsion(slow, ps, cs, float2(1.0, 0.0), 0.0, 11u, 1.0, amp, float4(24.0, 24.0, 1.0, 0.0));
    if (any(abs(slow - before) > 0.000001) || any(ps != beforePhase) || any(cs != beforeCells)) return 8;

    // Clock rollover: both value and future progression remain continuous,
    // including at uint wrap. No large floating world-time coordinate exists.
    float2 p = float2(0.9999);
    uint2 c = uint2(0xffffffffu);
    float left = leafTorqueTarget(float2(1.0, 0.0), p, c, 11u, 1.0);
    advanceFlutterClock(p, c, float2(0.0002));
    float right = leafTorqueTarget(float2(1.0, 0.0), p, c, 11u, 1.0);
    if (any(c != uint2(0u)) || any(p < 0.0) || any(p >= 1.0) || abs(left - right) > 0.00001) return 9;

    // Abrupt tempo changes retain bounded continuous state, and calm decays.
    for (uint i = 0u; i < 4096u; ++i) {
        float4 f = i % 64u < 32u ? float4(24.0, 24.0, 1.0, 0.0) : float4(0.25, 0.25, 1.0, 0.0);
        advanceLeafTorsion(slow, ps, cs, float2(1.0, 0.0), 1.0 / 256.0, 11u, 2.0, amp, f);
        if (abs(slow.x) > 1.0 || !all(isfinite(slow))) return 10;
    }
    float4 coarse = float4(0.0), fine = float4(0.0);
    float2 coarsePhase = float2(0.0), finePhase = float2(0.0);
    uint2 coarseCells = uint2(0u), fineCells = uint2(0u);
    float dtError = 0.0;
    for (uint frame = 0u; frame < 600u; ++frame) {
        float4 f = float4(24.0, 24.0, 1.0, 0.0);
        advanceLeafTorsion(coarse, coarsePhase, coarseCells, float2(1.0, 0.0), 1.0 / 60.0, 11u, 1.0, amp, f);
        for (uint substep = 0u; substep < 4u; ++substep)
            advanceLeafTorsion(fine, finePhase, fineCells, float2(1.0, 0.0), 1.0 / 240.0, 11u, 1.0, amp, f);
        dtError = max(dtError, abs(coarse.x - fine.x));
    }
    printf("max_24hz_60vs240_dt_error=%.8f\n", dtError);
    if (dtError > 0.003) return 11;
    printf("octave_trajectory_error=%.8f phase_rollover=passed live_retuning=passed\n", maximumError);
    return 0;
}
