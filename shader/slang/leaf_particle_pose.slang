module leaf_particle_pose;

// q is the normalized detached-leaf simulation pose, never an attached-leaf frame.
public float3 rotateLeafPlate(float4 q, float3 v)
{
    return v + 2.0 * cross(q.xyz, cross(q.xyz, v) + q.w * v);
}
