module ray;

import projection;

public struct Ray
{
    public float3 origin;
    public float3 direction;
    public float3 inverse_direction;
};

public Ray generateCameraRay(float2 screenUv, float4x4 inverseViewProjection)
{
    Ray ray;
    ray.origin = projectScreenUvToWorld(screenUv, 0.0, inverseViewProjection);
    float3 farPosition = projectScreenUvToWorld(screenUv, 1.0, inverseViewProjection);
    ray.direction = normalize(farPosition - ray.origin);
    ray.inverse_direction = 1.0 / ray.direction;
    return ray;
}
