module local_lighting;

public static const uint LOCAL_LIGHT_GPU_ABI_VERSION = 1u;
public static const uint LOCAL_LIGHT_KIND_POINT = 1u;

// Versioned record shared by live direct consumers and immutable DDGI transport snapshots.
// Point intensity is scene-linear radiant intensity; the evaluator produces irradiance.
public struct U_LightGpu
{
    public uint abi_version;
    public uint kind;
    public uint stable_id_slot;
    public uint stable_id_generation;
    public float3 position;
    public float range;
    public float3 color;
    public float intensity;
    public float3 direction;
    public float source_radius;
    public float4 shape_params;
};

public struct U_LocalLightInfo
{
    public uint abi_version;
    public uint count;
    public uint capacity;
    public uint overflow_count;
    public uint source_revision_low;
    public uint source_revision_high;
    public uint transport_revision;
    public uint flags;
};

public struct B_LocalLights
{
    public U_LightGpu data[];
};

public struct LocalPointLightSample
{
    public float3 irradiance;
    public float3 direction_to_light;
    public float distance_to_light;
};

public uint localLightCount(U_LocalLightInfo info)
{
    if (info.abi_version != LOCAL_LIGHT_GPU_ABI_VERSION) return 0u;
    return min(info.count, info.capacity);
}

public LocalPointLightSample evaluateUnshadowedPointLight(
    U_LightGpu light, float3 receiverPosition, float3 receiverNormal)
{
    LocalPointLightSample result;
    result.irradiance = float3(0.0);
    result.direction_to_light = float3(0.0);
    result.distance_to_light = 0.0;
    if (light.abi_version != LOCAL_LIGHT_GPU_ABI_VERSION ||
        light.kind != LOCAL_LIGHT_KIND_POINT || light.intensity <= 0.0 ||
        light.range <= 0.0 || light.source_radius <= 0.0)
    {
        return result;
    }

    float3 toLight = light.position - receiverPosition;
    float distanceSquared = dot(toLight, toLight);
    if (distanceSquared <= 1.0e-12) return result;
    float distance = sqrt(distanceSquared);
    if (distance >= light.range) return result;
    float3 direction = toLight / distance;
    float normalLengthSquared = dot(receiverNormal, receiverNormal);
    if (normalLengthSquared <= 1.0e-12) return result;
    float cosine = max(0.0, dot(receiverNormal * rsqrt(normalLengthSquared), direction));
    if (cosine <= 0.0) return result;

    float normalizedDistance = distance / light.range;
    float rangeBase = max(0.0, 1.0 - pow(normalizedDistance, 4.0));
    float rangeWindow = rangeBase * rangeBase;
    float attenuation = rangeWindow /
        max(distanceSquared, light.source_radius * light.source_radius);
    result.irradiance = light.color * light.intensity * cosine * attenuation;
    result.direction_to_light = direction;
    result.distance_to_light = distance;
    return result;
}
