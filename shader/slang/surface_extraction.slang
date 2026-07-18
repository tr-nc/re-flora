module surface_extraction;

import definitions;
import packer;
import voxel_data;
import voxel_types;

public static const uint SURFACE_GROUP_SIZE = 8u;
static const uint SURFACE_GROUP_SIZE_3 =
    SURFACE_GROUP_SIZE * SURFACE_GROUP_SIZE * SURFACE_GROUP_SIZE;
static const uint SURFACE_SHARED_SIZE = SURFACE_GROUP_SIZE + 4u;
static const uint SURFACE_SHARED_SIZE_3 =
    SURFACE_SHARED_SIZE * SURFACE_SHARED_SIZE * SURFACE_SHARED_SIZE;

groupshared uint surface_shared_data
    [SURFACE_SHARED_SIZE][SURFACE_SHARED_SIZE][SURFACE_SHARED_SIZE];
groupshared uint surface_workgroup_active_voxel_count;
groupshared uint surface_workgroup_active_brick_mask;

public void resetSurfaceWorkgroupAggregation(uint localInvocationIndex)
{
    if (localInvocationIndex == 0u)
    {
        surface_workgroup_active_voxel_count = 0u;
        surface_workgroup_active_brick_mask = 0u;
    }
}

public void recordActiveSurfaceVoxel(uint3 localInvocationId)
{
    uint ignored;
    InterlockedAdd(surface_workgroup_active_voxel_count, 1u, ignored);
    uint3 localBrick = localInvocationId / 4u;
    uint localBrickIndex = localBrick.x + localBrick.z * 2u + localBrick.y * 4u;
    InterlockedOr(surface_workgroup_active_brick_mask,
                  1u << localBrickIndex, ignored);
}

public uint surfaceWorkgroupActiveVoxelCount()
{
    return surface_workgroup_active_voxel_count;
}

public uint surfaceWorkgroupActiveBrickMask()
{
    return surface_workgroup_active_brick_mask;
}

uint voxelHashBucketFromBlueNoise(int3 worldPosition)
{
    uint hash = uint(worldPosition.x) * 73856093u;
    hash ^= uint(worldPosition.y) * 19349663u;
    hash ^= uint(worldPosition.z) * 83492791u;
    hash ^= hash >> 16u;
    hash *= 0x45d9f3bu;
    hash ^= hash >> 16u;
    return hash & 0x3u;
}

int3 sharedIndexToAtlasIndex(int3 atlasReadOffset, int3 workgroupBase, int3 sharedIndex)
{
    return sharedIndex + workgroupBase - int3(2) + atlasReadOffset;
}

int3 localIndexToSharedIndex(int3 workgroupBase, int3 localIndex)
{
    return localIndex - (workgroupBase - int3(2));
}

uint loadAtlas(readonly RWTexture3D<uint> chunkAtlas, int3 atlasIndex,
               int3 atlasReadOffset, int3 atlasReadDimension,
               bool isCrossingBoundary)
{
    if (isCrossingBoundary)
    {
        return voxelTypeFromAtlasData(chunkAtlas[uint3(atlasIndex)]);
    }

    if (any(atlasIndex < atlasReadOffset) ||
        any(atlasIndex >= atlasReadOffset + atlasReadDimension))
    {
        return 0u;
    }
    return voxelTypeFromAtlasData(chunkAtlas[uint3(atlasIndex)]);
}

public void preloadSurfaceData(readonly RWTexture3D<uint> chunkAtlas,
                               int3 atlasReadOffset, int3 atlasReadDimension,
                               bool isCrossingBoundary, int3 workgroupBase,
                               uint localInvocationIndex)
{
    for (uint linearIndex = localInvocationIndex;
         linearIndex < SURFACE_SHARED_SIZE_3;
         linearIndex += SURFACE_GROUP_SIZE_3)
    {
        uint3 sharedIndex;
        sharedIndex.x = linearIndex % SURFACE_SHARED_SIZE;
        sharedIndex.y = (linearIndex / SURFACE_SHARED_SIZE) % SURFACE_SHARED_SIZE;
        sharedIndex.z = linearIndex / (SURFACE_SHARED_SIZE * SURFACE_SHARED_SIZE);

        int3 atlasIndex = sharedIndexToAtlasIndex(
            atlasReadOffset, workgroupBase, int3(sharedIndex));
        surface_shared_data[sharedIndex.x][sharedIndex.y][sharedIndex.z] =
            loadAtlas(chunkAtlas, atlasIndex, atlasReadOffset, atlasReadDimension,
                      isCrossingBoundary);
    }
}

bool isSolid(uint voxelType)
{
    return voxelType != VOXEL_TYPE_EMPTY;
}

uint voxelAtLocalIndex(int3 workgroupBase, int3 localIndex)
{
    int3 sharedIndex = localIndexToSharedIndex(workgroupBase, localIndex);
    return surface_shared_data[sharedIndex.x][sharedIndex.y][sharedIndex.z];
}

bool isOccluded(int3 workgroupBase, int3 voxelIndex)
{
    static const int3 NEIGHBORS[6] =
    {
        int3(1, 0, 0), int3(-1, 0, 0), int3(0, 1, 0),
        int3(0, -1, 0), int3(0, 0, 1), int3(0, 0, -1),
    };

    for (int index = 0; index < 6; ++index)
    {
        if (!isSolid(voxelAtLocalIndex(workgroupBase, voxelIndex + NEIGHBORS[index])))
        {
            return false;
        }
    }
    return true;
}

float3 calculateNormal(int3 workgroupBase, int3 voxelIndex)
{
    float3 normal = float3(0.0);
    for (int z = -2; z <= 2; ++z)
    {
        for (int y = -2; y <= 2; ++y)
        {
            for (int x = -2; x <= 2; ++x)
            {
                int3 offset = int3(x, y, z);
                if (isSolid(voxelAtLocalIndex(workgroupBase, voxelIndex + offset)))
                {
                    normal += float3(offset);
                }
            }
        }
    }

    float normalLengthSquared = dot(normal, normal);
    return normalLengthSquared > EPSILON * EPSILON
        ? -normal * rsqrt(normalLengthSquared)
        : float3(0.0, 1.0, 0.0);
}

public bool extractSurfaceVoxel(int3 workgroupBase, int3 voxelIndex,
                                int3 atlasReadOffset, uint3 atlasReadDimension,
                                out uint properties)
{
    properties = 0u;
    if (any(voxelIndex >= int3(atlasReadDimension)))
    {
        return false;
    }

    uint voxelType = voxelAtLocalIndex(workgroupBase, voxelIndex);
    if (!isSolid(voxelType) || isOccluded(workgroupBase, voxelIndex))
    {
        return false;
    }

    int3 worldPosition = atlasReadOffset + voxelIndex;
    float3 normal = calculateNormal(workgroupBase, voxelIndex);
    uint normalBits = packNormalOct16(normal);
    uint hashId = voxelHashBucketFromBlueNoise(worldPosition);
    properties = packVoxelSurfaceData(voxelType, normalBits, hashId);
    return true;
}
