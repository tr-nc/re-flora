module local_emitter;

// Stable 2x2 Gauss quadrature on each receiver-facing box face. Positions and
// area weights belong to the light, never to a screen pixel or frame RNG.
// Integrated intensity retains the point approximation's total radiant power:
// for a Lambertian closed box, mean projected area is surface area / four.
public static const uint LOCAL_EMITTER_SAMPLE_COUNT = 12u;
public static const uint LOCAL_EMITTER_FIXED_SAMPLE_COUNT = 24u;

public struct LocalEmitterSample
{
    public float3 position;
    public float3 normal;
    public float irradiance_per_intensity;
};

// Fixed identities include both sides of all three axes, so a cached connection
// never silently changes emitting face when the receiver crosses the source.
public uint localEmitterFixedSampleIndex(float3 center, float3 receiver, uint index)
{
    uint axis = index / 4u;
    return axis * 8u + (receiver[axis] >= center[axis] ? 4u : 0u) + (index & 3u);
}

public LocalEmitterSample localEmitterFixedSurfaceSample(
    float3 center, float3 halfExtents, uint index)
{
    LocalEmitterSample result = {};
    result.position = center;
    if (index >= LOCAL_EMITTER_FIXED_SAMPLE_COUNT || any(halfExtents <= float3(0.0)))
        return result;
    uint axis = index / 8u;
    uint u = (axis + 1u) % 3u;
    uint v = (axis + 2u) % 3u;
    float side = (index & 4u) != 0u ? 1.0 : -1.0;
    result.normal[axis] = side;
    result.position[axis] += side * halfExtents[axis];
    float2 gauss = float2((index & 1u) != 0u ? 1.0 : -1.0,
                         (index & 2u) != 0u ? 1.0 : -1.0) * 0.57735026919;
    result.position[u] += gauss.x * halfExtents[u];
    result.position[v] += gauss.y * halfExtents[v];
    return result;
}

public LocalEmitterSample sampleLocalEmitterFixedIncident(
    float3 center, float3 halfExtents, float range, float3 receiver, uint index)
{
    LocalEmitterSample result = localEmitterFixedSurfaceSample(center, halfExtents, index);
    if (index >= LOCAL_EMITTER_FIXED_SAMPLE_COUNT || any(halfExtents <= float3(0.0)) || range <= 0.0)
        return result;
    float normalizedDistance = length(center - receiver) / range;
    if (normalizedDistance >= 1.0) return result;
    float3 segment = result.position - receiver;
    float distanceSquared = dot(segment, segment);
    if (distanceSquared <= 1.0e-12) return result;
    float3 direction = segment * rsqrt(distanceSquared);
    float emitterCosine = max(0.0, dot(result.normal, -direction));
    float meanProjectedArea = 2.0 * (halfExtents.x * halfExtents.y +
        halfExtents.x * halfExtents.z + halfExtents.y * halfExtents.z);
    uint axis = index / 8u;
    float areaWeight = halfExtents[(axis + 1u) % 3u] *
        halfExtents[(axis + 2u) % 3u] / meanProjectedArea;
    float window = max(0.0, 1.0 - pow(normalizedDistance, 4.0));
    result.irradiance_per_intensity = areaWeight * emitterCosine *
        window * window / distanceSquared;
    return result;
}

// Stop at the first entry into the authored emitter box, including when a
// probe sees the opposite face of a fixed sample. Only the emitter's own volume
// is exempted; lantern frames/caps and every blocker before it remain traced.
public float localEmitterConnectionDistance(
    float3 origin, float3 center, float3 halfExtents, float3 samplePosition)
{
    float3 segment = samplePosition - origin;
    float entry = 0.0;
    for (uint axis = 0u; axis < 3u; ++axis)
    {
        if (abs(segment[axis]) > 1.0e-12)
        {
            float a = (center[axis] - halfExtents[axis] - origin[axis]) / segment[axis];
            float b = (center[axis] + halfExtents[axis] - origin[axis]) / segment[axis];
            entry = max(entry, min(a, b));
        }
    }
    return length(segment) * saturate(entry);
}

// Incident response excludes receiver cosine; probes and direct receivers share
// the same authored shape, units, quadrature, and range window.
public LocalEmitterSample sampleLocalEmitterIncident(
    float3 center, float3 halfExtents, float range,
    float3 receiver, uint index)
{
    if (index >= LOCAL_EMITTER_SAMPLE_COUNT)
    {
        LocalEmitterSample empty = {};
        empty.position = center;
        return empty;
    }
    return sampleLocalEmitterFixedIncident(center, halfExtents, range, receiver,
        localEmitterFixedSampleIndex(center, receiver, index));
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
