module grass_sway_sample;
import leaf_flutter;
import hash;

// Sampling seam shared by detailed meshes, LOD and lighting-cache vertices.
public float2 grassLocalResponse(float4 motion, float2 phase, uint2 cells, uint seed)
{
    // A stable stem seed shifts both fractional and integer noise coordinates.
    // Share the integrated wind-frequency clock, never the sampled trajectory.
    float2 offset = float2(wellonsHash(seed ^ 0xB5297A4Du) & 65535u,
                           wellonsHash(seed ^ 0x68E31DA4u) & 65535u) / 65536.0;
    advanceFlutterClock(phase, cells, offset);
    float2 local = float2(leafFlutterNoise(phase.x, cells.x, seed),
        leafFlutterNoise(phase.y, cells.y, seed ^ 0xAD90777Du));
    return local * (max(motion.x, 0.0) * 0.70710678);
}
