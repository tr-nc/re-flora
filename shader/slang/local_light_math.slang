module local_light_math;

// Authoritative authored-light units, shared by direct receivers and probes.
public float pointLightIncidentWeight(float distanceSquared, float radius, float range)
{
    if (distanceSquared <= 1.0e-12 || radius <= 0.0 || range <= 0.0 ||
        distanceSquared >= range * range) return 0.0;
    float normalizedSquared = distanceSquared / (range * range);
    float window = max(0.0, 1.0 - normalizedSquared * normalizedSquared);
    return window * window / max(distanceSquared, radius * radius);
}

public float3 projectLocalLightResponse(float3 incident, float3 direction, float3 normal)
{
    return incident * max(0.0, dot(normal, direction));
}
