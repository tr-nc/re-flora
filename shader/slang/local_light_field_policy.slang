module local_light_field_policy;

// Per-connection RGB transmission, not irradiance. Two uint2 header records,
// followed by 24 fixed emitter-face connections per (probe, live light).
public static const uint LOCAL_LIGHT_FIELD_ABI = 1u;
public static const uint LOCAL_LIGHT_FIELD_CONNECTIONS = 24u;

public uint localLightConnectionIndex(uint probe, uint light, uint lightCount, uint tap)
{
    return 2u + (probe * lightCount + light) * LOCAL_LIGHT_FIELD_CONNECTIONS + tap;
}

public uint2 packLocalLightTransmission(float3 transmission)
{
    uint3 q = uint3(round(saturate(transmission) * 65535.0));
    return uint2(q.x | (q.y << 16u), q.z | (1u << 16u));
}

public float4 unpackLocalLightTransmission(uint2 packed)
{
    return float4(float3(packed.x & 65535u, packed.x >> 16u,
        packed.y & 65535u) / 65535.0, (packed.y >> 16u) != 0u ? 1.0 : 0.0);
}

public struct LocalLightTransmissionSupport
{
    public float3 weighted;
    public float3 minimum;
    public float3 maximum;
    public float weight;
};

public LocalLightTransmissionSupport emptyLocalLightTransmissionSupport()
{
    LocalLightTransmissionSupport s = {};
    s.minimum = float3(1.0);
    return s;
}

public void accumulateLocalLightTransmission(
    inout LocalLightTransmissionSupport s, float weight, float4 sample)
{
    if (weight <= 1.0e-6 || sample.w == 0.0) return;
    s.weight += weight;
    s.weighted += weight * sample.xyz;
    s.minimum = min(s.minimum, sample.xyz);
    s.maximum = max(s.maximum, sample.xyz);
}

public bool finishLocalLightTransmission(
    LocalLightTransmissionSupport s, float expectedWeight, out float3 transmission)
{
    transmission = float3(0.0);
    // A blocked probe connection is not proof that a nearby receiver is blocked
    // (e.g. a lantern window between probes). Ask the original evaluator instead
    // of publishing a false dark patch. Conflicting/glass-boundary connections
    // also use the exact path, not a blurred shadow or ambient compensation.
    if (expectedWeight <= 1.0e-6 || s.weight < expectedWeight * (1.0 - 1.0e-4) ||
        any(s.maximum - s.minimum > float3(1.0e-3)) ||
        !any(s.maximum > float3(0.0))) return false;
    transmission = s.weighted / s.weight;
    return all(isfinite(transmission));
}

public bool localLightOutsideReceiverSupport(
    float3 receiver, float3 source, float sourceExtent, float supportRadiusSquared)
{
    // A source intersecting the receiver's probe support is a near-field
    // visibility singularity, not a reliable interpolation domain.
    float clearance = max(0.0, length(source - receiver) - sourceExtent);
    return clearance * clearance > supportRadiusSquared;
}
