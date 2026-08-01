import aabb;
import contree_node;
import marching_result;
import scene_marching;

// Keep this exact voxel-shadow contract aligned with the DDGI probe transport path. The probe
// shader still owns an equivalent local implementation while feedback work is in flight; a
// source-contract test prevents either copy from silently drifting before they can be unified.
static const float DDGI_TERRAIN_VOXEL_SIZE = 1.0 / 256.0;
static const float DDGI_SHADOW_NORMAL_BIAS_VOXELS = 0.25;
static const float DDGI_SHADOW_LIGHT_BIAS_VOXELS = 0.75;

public float3 ddgiVoxelSurfacePositionAlongNormal(float3 voxelCenter,
                                                  float3 normal)
{
    float3 normalDirection = normalize(normal);
    float2 distance = intersectSlabs(float3(-1.0), float3(1.0),
                                     float3(0.0), 1.0 / normalDirection);
    const float halfVoxelSize = 1.0 / 512.0;
    return voxelCenter + normalDirection * (distance.y * halfVoxelSize);
}

public float3 ddgiTerrainShadowReceiverPosition(float3 voxelCenter,
                                                float3 normal,
                                                float3 lightDirection)
{
    float3 normalDirection = normalize(normal);
    float3 lightDirectionNormalized = normalize(lightDirection);
    return ddgiVoxelSurfacePositionAlongNormal(voxelCenter, normalDirection) +
        normalDirection *
            (DDGI_SHADOW_NORMAL_BIAS_VOXELS * DDGI_TERRAIN_VOXEL_SIZE) +
        lightDirectionNormalized *
            (DDGI_SHADOW_LIGHT_BIAS_VOXELS * DDGI_TERRAIN_VOXEL_SIZE);
}

public float ddgiExactTerrainSunVisibility(
    StructuredBuffer<ContreeNode> nodeData,
    StructuredBuffer<uint> leafData,
    readonly RWTexture3D<uint2> sceneTexture,
    MarchingResult receiver,
    float3 sunDirection,
    uint localInvocationIndex)
{
    float3 origin = ddgiTerrainShadowReceiverPosition(
        receiver.center_position, receiver.normal, sunDirection);
    MarchingResult shadowHit = marchScene(
        nodeData, leafData, sceneTexture,
        origin, sunDirection, 1.0 / sunDirection, localInvocationIndex);
    return shadowHit.is_hit ? 0.0 : 1.0;
}
