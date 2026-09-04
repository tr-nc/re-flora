import terrain_ray_origin;

bool nearlyEqual(float3 a, float3 b)
{
    return all(abs(a - b) < float3(1.0e-6));
}

export __extern_cpp int main()
{
    float3 center = float3(1.0, 2.0, 3.0);
    float3 normal = float3(1.0);
    float3 surface = terrainVoxelSurfacePositionAlongNormal(
        center, normal, 0.25);
    float3 clamped = terrainRayOriginAlongNormal(
        center, normal, 0.25, -1.0);
    float3 biased = terrainRayOriginAlongNormal(
        center, normal, 0.25, 0.5);
    float3 fromPositionClamped = terrainRayOriginFromPosition(
        center, float3(0.0, 2.0, 0.0), -1.0);
    float3 fromPositionBiased = terrainRayOriginFromPosition(
        center, float3(0.0, 2.0, 0.0), 0.25);

    float3 normalized = normalize(normal);
    bool surfacePolicy = nearlyEqual(surface, center + float3(0.125)) &&
        nearlyEqual(clamped, surface) &&
        nearlyEqual(biased, surface + normalized * 0.5);
    bool positionOriginPolicy = nearlyEqual(fromPositionClamped, center) &&
        nearlyEqual(fromPositionBiased, center + float3(0.0, 0.25, 0.0));
    return surfacePolicy && positionOriginPolicy ? 0 : 1;
}
