module flora_light_stability;

// Persistent quality classification only: never stores/reuses old radiance,
// visibility, surface positions or normals. Exact receivers are shaded afresh.
public struct FloraLightStability
{
    public uint4 receiver; // root xyz, spawn generation; species/layer own the slot
    public uint4 revision_mode; // full live revision, traced geometry, mode (1 RIS, 2 exact)
    public uint4 signature; // selected connection and RGB attenuation fingerprints
};

public bool floraLightStabilityMatches(FloraLightStability previous,
    uint4 receiver,uint3 revision)
{
    return previous.revision_mode.w!=0u && all(previous.receiver==receiver) &&
        all(previous.revision_mode.xyz==revision);
}

public bool floraLightStabilityNeedsExact(FloraLightStability previous,uint4 signature)
{
    return previous.revision_mode.w==2u || any(previous.signature!=signature);
}

public bool floraLightStabilityCalibration(float3 reference,float3 estimate,float3 environment)
{
    float3 total=max(environment+reference,float3(1.0e-5));
    // Leave margin for smooth response drift. Do not let opposing color errors
    // cancel in luminance (saturated sources are still meaningful).
    return !all(isfinite(estimate)) || any(abs(reference-estimate)>0.01*total);
}
