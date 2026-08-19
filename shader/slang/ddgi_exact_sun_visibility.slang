module ddgi_exact_sun_visibility;

import contree_node;
import marching_result;
import scene_marching;
import terrain_ray_origin;

public float ddgiExactTerrainSunVisibility(
    StructuredBuffer<ContreeNode> nodeData,
    StructuredBuffer<uint> leafData,
    readonly RWTexture3D<uint2> sceneTexture,
    MarchingResult receiver,
    float3 sunDirection,
    float originOffsetWorld,
    uint localInvocationIndex)
{
    float3 origin = terrainRayOriginAlongNormal(
        receiver.center_position, receiver.normal, originOffsetWorld);
    MarchingResult shadowHit = marchScene(
        nodeData, leafData, sceneTexture,
        origin, sunDirection, 1.0 / sunDirection, localInvocationIndex);
    return shadowHit.is_hit ? 0.0 : 1.0;
}
