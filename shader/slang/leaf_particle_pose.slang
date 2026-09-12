module leaf_particle_pose;

// Optical encoding tag, not an A/B mode. Bit 31 remains the existing sprite flip.
public static const uint LEAF_FLIGHT_BIT = 0x40000000u;

// Rotate the optical normal only. The simulation pose is not a geometric frame.
public float3 leafParticleOpticalNormal(float4 optics, uint flags)
{
    if ((flags & LEAF_FLIGHT_BIT) == 0u) return optics.xyz;
    float3 restNormal = float3(0.0, 0.0, 1.0);
    return restNormal + 2.0 * cross(optics.xyz,
        cross(optics.xyz, restNormal) + optics.w * restNormal);
}

// No simulation orientation input: every particle quad uses the camera axes.
public float3 screenFacingParticleOffset(float2 offset, float3 cameraRight, float3 cameraUp)
{
    return cameraRight * offset.x + cameraUp * offset.y;
}
