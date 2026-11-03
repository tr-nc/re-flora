module light_reservoir;

// Independently expressed weighted reservoir / RIS mathematics. No SDK code.
// A proposal represents one discrete (light, quadrature tap) pair. The target
// excludes visibility; only the selected pair needs an expensive connection.
public uint lightSamplingHash(uint x)
{
    x ^= x >> 16u;
    x *= 0x7feb352du;
    x ^= x >> 15u;
    x *= 0x846ca68bu;
    return x ^ (x >> 16u);
}

public float lightSamplingRandom(inout uint state)
{
    state += 0x9e3779b9u;
    return float(lightSamplingHash(state) >> 8u) * (1.0 / 16777216.0);
}

public struct LightReservoir
{
    public uint selected;
    public float weight_sum;
    public float selected_target;
};

public float lightSamplingTarget(float3 response)
{
    // All channels matter, including a saturated blue light. A zero target is
    // permitted only for an exactly zero, nonnegative unshadowed response.
    return dot(response, float3(0.2126, 0.7152, 0.0722));
}

public void streamLightCandidate(inout LightReservoir reservoir,
    uint candidate, float target, float inverseProposal, float random)
{
    float weight = target * inverseProposal;
    if (weight <= 0.0) return;
    reservoir.weight_sum += weight;
    if (random * reservoir.weight_sum < weight)
    {
        reservoir.selected = candidate;
        reservoir.selected_target = target;
    }
}

public float lightReservoirNormalization(LightReservoir reservoir, uint candidates)
{
    // M counts every proposal, including zero-response and out-of-range ones.
    return reservoir.selected_target > 0.0 && candidates > 0u
        ? reservoir.weight_sum / (float(candidates) * reservoir.selected_target) : 0.0;
}
