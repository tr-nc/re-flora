module terrain_ray_origin;

import aabb;

static const float TERRAIN_RAY_ORIGIN_VOXEL_SIZE = 1.0 / 256.0;

public float3 terrainVoxelSurfacePositionAlongNormal(float3 voxelCenter,
                                                     float3 normal,
                                                     float voxelSizeWorld)
{
    float3 normalDirection = normalize(normal);
    float2 distance = intersectSlabs(float3(-1.0), float3(1.0),
                                     float3(0.0), 1.0 / normalDirection);
    float halfVoxelSize = max(0.0, voxelSizeWorld) * 0.5;
    return voxelCenter + normalDirection * (distance.y * halfVoxelSize);
}

public float3 terrainVoxelSurfacePositionAlongNormal(float3 voxelCenter,
                                                     float3 normal)
{
    return terrainVoxelSurfacePositionAlongNormal(
        voxelCenter, normal, TERRAIN_RAY_ORIGIN_VOXEL_SIZE);
}

public float3 terrainRayOriginAlongNormal(float3 voxelCenter, float3 normal,
                                          float voxelSizeWorld,
                                          float offsetWorld)
{
    float3 normalDirection = normalize(normal);
    return terrainVoxelSurfacePositionAlongNormal(
        voxelCenter, normalDirection, voxelSizeWorld) +
        normalDirection * max(0.0, offsetWorld);
}

public float3 terrainRayOriginAlongNormal(float3 voxelCenter, float3 normal,
                                          float offsetWorld)
{
    return terrainRayOriginAlongNormal(
        voxelCenter, normal, TERRAIN_RAY_ORIGIN_VOXEL_SIZE, offsetWorld);
}

public float3 terrainRayOriginFromPosition(float3 position, float3 normal,
                                            float offsetWorld)
{
    float3 normalDirection = normalize(normal);
    return position +
        normalDirection * max(0.0, offsetWorld);
}
