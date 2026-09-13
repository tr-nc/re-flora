module vegetation_response_solver;
import flora_types;
import leaf_flutter;

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

public void advanceLeafTorsion(inout float4 torsion, float2 wind, float dt,
                              uint seed, float gain)
{
    float target = leafTorqueTarget(wind, torsion.x, seed, gain);
    advanceMechanicalResponse(torsion, float2(target, 0.0), dt,
        float2(1.8, 0.32), float4(1.0, 1.0, 1.0, 0.0));
}
