module vegetation_response_solver;
import flora_types;
import leaf_flutter;
import wind_response_curve;

// Unresolved wind excitation around the mean bend, not another wind field.
// Stored in the existing grass grid's otherwise-unused local motion/clock.
public void advanceGrassSway(inout float4 motion, inout float2 phase, inout uint2 cells,
    float2 wind, float dt, uint seed, float4 amplitude, float4 frequency, float4 curve)
{
    float speed = length(wind);
    float gain = speed > 0.0 ? lerp(amplitude.x, amplitude.y,
        windResponseCurve(speed, amplitude.z, amplitude.w, curve.x)) : 0.0;
    float hz = max(0.0001, lerp(frequency.x, frequency.y,
        windResponseCurve(speed, frequency.z, frequency.w, curve.y)));
    // Grid/instance state owns only a smooth wind-powered envelope and clock.
    // Each rendered stem evaluates its own phase; sharing sampled positions here
    // would synchronize every stem inside a grid cell.
    advanceMechanicalResponse(motion, float2(gain, 0.0), max(dt, 0.0),
        float2(max(hz, 0.5), 0.9), float4(1.0, 1.0, 1.0, 0.0));
    float limit = max(amplitude.x, amplitude.y);
    if (motion.x < 0.0 || motion.x > limit) {
        motion.x = clamp(motion.x, 0.0, limit);
        motion.z = 0.0;
    }
    advanceFlutterClock(phase, cells, leafFlutterRates(seed, hz) * max(dt, 0.0));
}

// One solver for field nodes and lifetime-owned plants. State ownership is not
// a solver concern: all callers use world seconds, voxel displacement and velocity.
public float2 vegetationMechanicalProfile(uint species)
{
    if (species < 2u) return float2(1.0, 0.24);
    if (species == FLORA_SPECIES_LAVENDER) return float2(0.65, 0.30);
    if (species == FLORA_SPECIES_EMBER_BLOOM) return float2(0.52, 0.34);
    if (species == FLORA_SPECIES_TREE_LEAF) return float2(1.4, 0.28); // attached leaf tissue
    return float2(0.75, 0.45); // heavier hanging fruit
}

void advanceMechanicalResponse(inout float4 motion, float2 force,
                               float dt, float2 profile, float4 controls)
{
    float frequency = profile.x * clamp(controls.x, 0.25, 3.0);
    float damping = clamp(profile.y * clamp(controls.y, 0.25, 2.0), 0.02, 0.95);
    force *= clamp(controls.z, 0.0, 3.0);
    float omega = 6.28318530718 * frequency;
    float decay = damping * omega;
    float oscillation = omega * sqrt(1.0 - damping * damping);
    float2 displacement = motion.xy - force;
    float2 velocity = motion.zw;
    float c = cos(oscillation * dt);
    float s = sin(oscillation * dt) / oscillation;
    float envelope = exp(-decay * dt);
    motion.xy = force + envelope * (displacement * c + (velocity + decay * displacement) * s);
    motion.zw = envelope * (velocity * c - (decay * velocity + omega * omega * displacement) * s);
}

public void advanceVegetationResponse(inout float4 motion, float2 force,
                                     float dt, uint species, float4 controls)
{
    advanceMechanicalResponse(motion, force, dt, vegetationMechanicalProfile(species), controls);
}

public void advanceLeafResponse(inout float4 motion, float2 force,
                                float dt, uint seed, float4 controls)
{
    // Fixed tissue variation, not time-varying random force. All leaves still
    // follow the same local wind; their compliance and settling differ slightly.
    uint hash = seed * 747796405u + 2891336453u;
    hash = ((hash >> ((hash >> 28u) + 4u)) ^ hash) * 277803737u;
    hash = (hash >> 22u) ^ hash;
    float3 variation = float3(hash & 255u, (hash >> 8u) & 255u, (hash >> 16u) & 255u) / 255.0;
    float2 profile = vegetationMechanicalProfile(FLORA_SPECIES_TREE_LEAF) * lerp(float2(0.75, 0.8), float2(1.25, 1.2), variation.xy);
    advanceMechanicalResponse(motion, force * lerp(0.85, 1.15, variation.z), dt, profile, controls);
}

public void advanceLeafTorsion(inout float4 torsion, inout float2 phase, inout uint2 cells,
                              float2 wind, float dt, uint seed, float gain,
                              float4 curve = float4(0.05, 1.0, 0.0, 0.0),
                              float4 frequency = float4(1.8, 1.8, 1.0, 0.0),
                              float4 frequencyCurve = float4(0.05, 1.0, 0.0, 0.0))
{
    float hz = leafFlutterFrequency(wind, frequency, frequencyCurve);
    // Resolve both noise bands even at the maximum exposed 48 Hz. This changes
    // only local leaf work, not the world's tick or other vegetation dynamics.
    uint steps = uint(clamp(ceil(max(dt, 0.0) * hz * 32.0), 1.0, 256.0));
    float step = max(dt, 0.0) / float(steps);
    float2 increment = leafFlutterRates(seed, hz) * step;
    for (uint i = 0u; i < steps; ++i)
    {
        float2 midpoint = phase;
        uint2 midpointCells = cells;
        advanceFlutterClock(midpoint, midpointCells, increment * 0.5);
        float target = leafTorqueTarget(wind, midpoint, midpointCells, seed, gain, curve);
        advanceMechanicalResponse(torsion, float2(target, 0.0), step,
            float2(hz, 0.32), float4(1.0, 1.0, 1.0, 0.0));
        // Mechanical travel stop; preserve interior angle/velocity on retuning.
        if (abs(torsion.x) > 1.0) {
            torsion.x = clamp(torsion.x, -1.0, 1.0);
            if (torsion.x * torsion.z > 0.0) torsion.z = 0.0;
        }
        advanceFlutterClock(phase, cells, increment);
    }
}
