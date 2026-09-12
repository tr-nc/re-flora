module leaf_particle_pose;

// Particle texture-index flags; bit 31 remains the existing sprite flip.
public static const uint LEAF_FLIGHT_BIT = 0x40000000u;
public static const uint LEAF_ROTATING_PLATE_BIT = 0x20000000u;

// q is the normalized detached-leaf simulation pose, never an attached-leaf frame.
public float3 rotateLeafPlate(float4 q, float3 v)
{
    return v + 2.0 * cross(q.xyz, cross(q.xyz, v) + q.w * v);
}

public struct LeafParticlePose
{
    public float3 vertexOffset;
    public float3 opticalNormal;
    public bool hasFlightPose;
};

// The default billboard is an artistic shape proxy. Its optical orientation still
// comes from the held B simulation pose, never the camera-facing geometric normal.
public LeafParticlePose evaluateLeafParticlePose(float4 optics, uint flags,
    float2 offset, float3 cameraRight, float3 cameraUp)
{
    LeafParticlePose pose;
    pose.hasFlightPose = (flags & LEAF_FLIGHT_BIT) != 0u;
    pose.opticalNormal = pose.hasFlightPose
        ? rotateLeafPlate(optics, float3(0.0, 0.0, 1.0)) : optics.xyz;
    pose.vertexOffset = pose.hasFlightPose && (flags & LEAF_ROTATING_PLATE_BIT) != 0u
        ? rotateLeafPlate(optics, float3(offset, 0.0))
        : cameraRight * offset.x + cameraUp * offset.y;
    return pose;
}
