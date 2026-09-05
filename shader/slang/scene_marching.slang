module scene_marching;

import aabb;
import contree_marching;
import contree_node;
import definitions;
import marching_result;
import voxel_data;

static const uint MAX_DDA_ITERATIONS = 256u;

bool traceChunk(
    inout MarchingResult result,
    StructuredBuffer<ContreeNode> nodeData,
    StructuredBuffer<uint> leafData,
    float3 origin, float3 direction, int3 chunkPosition, uint2 sceneData,
    uint localInvocationIndex)
{
    if (sceneData.x == 0u)
    {
        return false;
    }
    sceneData -= 1u;

    ContreeMarchingResult contreeResult = marchContree(
        nodeData, origin, direction, float3(chunkPosition), float3(1.0), false,
        sceneData.x, sceneData.y, localInvocationIndex);
    if (!contreeResult.is_hit)
    {
        return false;
    }

    uint voxelData = leafData[contreeResult.voxel_address];
    result.is_hit = true;
    result.position = contreeResult.position;
    result.center_position = contreeResult.center_position;
    result.normal = voxelSurfaceNormalFromData(voxelData);
    result.voxel_type = voxelTypeFromData(voxelData);
    result.voxel_address = contreeResult.voxel_address;
    return true;
}

public MarchingResult marchScene(
    StructuredBuffer<ContreeNode> nodeData,
    StructuredBuffer<uint> leafData,
    readonly RWTexture3D<uint2> sceneTexture, float3 origin, float3 direction,
    float3 inverseDirection, uint localInvocationIndex)
{
    MarchingResult result;
    result.iteration_count = 0u;
    result.is_hit = false;
    result.position = float3(0.0);
    result.center_position = float3(0.0);
    result.distance = 1.0e10;
    result.normal = float3(0.0);
    result.voxel_type = 0u;
    result.voxel_address = 0u;

    uint width;
    uint height;
    uint depth;
    sceneTexture.GetDimensions(width, height, depth);
    int3 visibleChunkDimensions = int3(width, height, depth);

    float3 minimumBound = float3(0.0);
    float3 maximumBound = float3(visibleChunkDimensions);
    direction = max(abs(direction), float3(EPSILON)) *
                (step(float3(0.0), direction) * 2.0 - 1.0);

    float minimumSceneDistance;
    float maximumSceneDistance;
    intersectSlabs(minimumBound, maximumBound, origin, inverseDirection,
                   minimumSceneDistance, maximumSceneDistance);
    if (minimumSceneDistance > maximumSceneDistance || maximumSceneDistance < 0.0)
    {
        return result;
    }

    float marchDistance = max(minimumSceneDistance, 0.0) + EPSILON;
    float3 marchedOrigin = origin + marchDistance * direction;

    float3 deltaDistance = 1.0 / abs(direction);
    int3 rayStep = int3(sign(direction));
    int3 mapPosition = int3(floor(marchedOrigin));
    float3 sideDistance =
        ((sign(direction) * 0.5 + 0.5) +
         sign(direction) * (float3(mapPosition) - marchedOrigin)) * deltaDistance;

    while (result.iteration_count++ < MAX_DDA_ITERATIONS)
    {
        bool3 minimumMask = sideDistance <= min(sideDistance.yzx, sideDistance.zxy);
        sideDistance += select(minimumMask, deltaDistance, float3(0.0));
        if (!isInsideAabb(mapPosition, int3(0), visibleChunkDimensions))
        {
            break;
        }

        uint2 sceneData = sceneTexture[uint3(mapPosition)];
        if (traceChunk(result, nodeData, leafData, marchedOrigin, direction, mapPosition,
                       sceneData, localInvocationIndex))
        {
            result.distance = length(origin - result.position);
            break;
        }
        mapPosition += select(minimumMask, int3(1), int3(0)) * rayStep;
    }
    return result;
}
