module local_emitter;

// Stable 2x2 Gauss quadrature on each receiver-facing box face. Positions and
// area weights belong to the light, never to a screen pixel or frame RNG.
// Integrated intensity retains the point approximation's total radiant power:
// for a Lambertian closed box, mean projected area is surface area / four.
public static const uint LOCAL_EMITTER_SAMPLE_COUNT = 12u;

// Ownership is an entire authored voxel, not a grown endpoint epsilon. A frame,
// foreign edit or neighbouring emitter outside this box remains a blocker.
public bool localEmitterContainsVoxel(float3 center, float3 halfExtents, float3 voxelCenter)
{
    float3 halfVoxel=float3(0.5/256.0);
    return all(voxelCenter-halfVoxel>=center-halfExtents) &&
           all(voxelCenter+halfVoxel<=center+halfExtents);
}

public struct LocalEmitterSample
{
    public float3 position;
    public float3 normal;
    public float irradiance_per_intensity;
};

// Incident response excludes receiver cosine, so a probe connection can be
// shared across all output normal directions without repeating transport.
public LocalEmitterSample sampleLocalEmitterIncident(
    float3 center, float3 halfExtents, float range,
    float3 receiver, uint index)
{
    LocalEmitterSample result;
    result.position = center;
    result.normal = float3(0.0);
    result.irradiance_per_intensity = 0.0;
    if (index >= LOCAL_EMITTER_SAMPLE_COUNT || any(halfExtents <= float3(0.0)) || range <= 0.0)
        return result;
    float normalizedDistance = length(center - receiver) / range;
    if (normalizedDistance >= 1.0) return result;
    uint axis = index / 4u;
    uint u = (axis + 1u) % 3u;
    uint v = (axis + 2u) % 3u;
    float side = receiver[axis] >= center[axis] ? 1.0 : -1.0;
    result.normal[axis] = side;
    result.position[axis] += side * halfExtents[axis];
    float2 gauss = float2((index & 1u) != 0u ? 1.0 : -1.0,
                         (index & 2u) != 0u ? 1.0 : -1.0) * 0.57735026919;
    result.position[u] += gauss.x * halfExtents[u];
    result.position[v] += gauss.y * halfExtents[v];
    float3 segment = result.position - receiver;
    float distanceSquared = dot(segment, segment);
    if (distanceSquared <= 1.0e-12) return result;
    float3 direction = segment * rsqrt(distanceSquared);
    float emitterCosine = max(0.0, dot(result.normal, -direction));
    float meanProjectedArea = 2.0 * (halfExtents.x * halfExtents.y +
        halfExtents.x * halfExtents.z + halfExtents.y * halfExtents.z);
    // Face area is 4*h[u]*h[v]; each quadrature point owns one quarter.
    float areaWeight = halfExtents[u] * halfExtents[v] / meanProjectedArea;
    float window = max(0.0, 1.0 - pow(normalizedDistance, 4.0));
    result.irradiance_per_intensity = areaWeight * emitterCosine *
        window * window / distanceSquared;
    return result;
}

public LocalEmitterSample sampleLocalEmitter(
    float3 center, float3 halfExtents, float range,
    float3 receiver, float3 receiverNormal, uint index)
{
    LocalEmitterSample result = sampleLocalEmitterIncident(
        center, halfExtents, range, receiver, index);
    float3 segment = result.position - receiver;
    float distanceSquared = dot(segment, segment);
    if (distanceSquared > 1.0e-12)
        result.irradiance_per_intensity *= max(0.0,
            dot(normalize(receiverNormal), segment * rsqrt(distanceSquared)));
    return result;
}
