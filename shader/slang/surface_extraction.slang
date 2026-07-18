module surface_extraction;

import definitions;
import packer;
import surface_normal_policy;
import voxel_data;
import voxel_types;

public static const uint SURFACE_GROUP_SIZE = 8u;
static const uint SURFACE_GROUP_SIZE_3 =
    SURFACE_GROUP_SIZE * SURFACE_GROUP_SIZE * SURFACE_GROUP_SIZE;
static const uint SURFACE_SHARED_SIZE =
    SURFACE_GROUP_SIZE + uint(SURFACE_NORMAL_RADIUS * 2);
static const uint SURFACE_SHARED_ROW_COUNT =
    SURFACE_SHARED_SIZE * SURFACE_SHARED_SIZE;

// Occupancy needs the complete normal halo, while only the central 8-wide rows
// need voxel types. Packing both keeps the smooth estimator's source data in
// less than 1 KiB of workgroup memory.
groupshared uint surface_shared_occupancy_rows
    [SURFACE_SHARED_SIZE][SURFACE_SHARED_SIZE];
groupshared uint surface_shared_type_rows
    [SURFACE_GROUP_SIZE][SURFACE_GROUP_SIZE];
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
    return sharedIndex + workgroupBase - int3(SURFACE_NORMAL_RADIUS) +
           atlasReadOffset;
}

int3 localIndexToSharedIndex(int3 workgroupBase, int3 localIndex)
{
    return localIndex - (workgroupBase - int3(SURFACE_NORMAL_RADIUS));
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
    for (uint linearRow = localInvocationIndex;
         linearRow < SURFACE_SHARED_ROW_COUNT;
         linearRow += SURFACE_GROUP_SIZE_3)
    {
        uint sharedY = linearRow % SURFACE_SHARED_SIZE;
        uint sharedZ = linearRow / SURFACE_SHARED_SIZE;
        uint occupancyRow = 0u;
        uint typeRow = 0u;
        bool isCentralRow =
            sharedY >= uint(SURFACE_NORMAL_RADIUS) &&
            sharedY < uint(SURFACE_NORMAL_RADIUS) + SURFACE_GROUP_SIZE &&
            sharedZ >= uint(SURFACE_NORMAL_RADIUS) &&
            sharedZ < uint(SURFACE_NORMAL_RADIUS) + SURFACE_GROUP_SIZE;

        for (uint sharedX = 0u; sharedX < SURFACE_SHARED_SIZE; ++sharedX)
        {
            uint3 sharedIndex = uint3(sharedX, sharedY, sharedZ);
            int3 atlasIndex = sharedIndexToAtlasIndex(
                atlasReadOffset, workgroupBase, int3(sharedIndex));
            uint voxelType = loadAtlas(
                chunkAtlas, atlasIndex, atlasReadOffset, atlasReadDimension,
                isCrossingBoundary);
            if (voxelType != VOXEL_TYPE_EMPTY)
            {
                occupancyRow |= 1u << sharedX;
            }

            if (isCentralRow &&
                sharedX >= uint(SURFACE_NORMAL_RADIUS) &&
                sharedX < uint(SURFACE_NORMAL_RADIUS) + SURFACE_GROUP_SIZE)
            {
                uint localX = sharedX - uint(SURFACE_NORMAL_RADIUS);
                typeRow |= (voxelType & VOXEL_TYPE_MASK) << (localX * 4u);
            }
        }

        surface_shared_occupancy_rows[sharedY][sharedZ] = occupancyRow;
        if (isCentralRow)
        {
            uint localY = sharedY - uint(SURFACE_NORMAL_RADIUS);
            uint localZ = sharedZ - uint(SURFACE_NORMAL_RADIUS);
            surface_shared_type_rows[localY][localZ] = typeRow;
        }
    }
}

bool isSolid(uint voxelType)
{
    return voxelType != VOXEL_TYPE_EMPTY;
}

bool isSolidAtLocalIndex(int3 workgroupBase, int3 localIndex)
{
    int3 sharedIndex = localIndexToSharedIndex(workgroupBase, localIndex);
    uint occupancyRow =
        surface_shared_occupancy_rows[sharedIndex.y][sharedIndex.z];
    return ((occupancyRow >> uint(sharedIndex.x)) & 1u) != 0u;
}

uint voxelTypeAtLocalIndex(int3 workgroupBase, int3 localIndex)
{
    int3 sharedIndex = localIndexToSharedIndex(workgroupBase, localIndex);
    uint localX = uint(sharedIndex.x - SURFACE_NORMAL_RADIUS);
    uint localY = uint(sharedIndex.y - SURFACE_NORMAL_RADIUS);
    uint localZ = uint(sharedIndex.z - SURFACE_NORMAL_RADIUS);
    return (surface_shared_type_rows[localY][localZ] >> (localX * 4u)) &
           VOXEL_TYPE_MASK;
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
        if (!isSolidAtLocalIndex(
                workgroupBase, voxelIndex + NEIGHBORS[index]))
        {
            return false;
        }
    }
    return true;
}

// A 5-bit X window represents offsets -2 through +2. Summing this
// per-row moment plus the Y/Z population moments is exactly equivalent to
// visiting all 125 voxels in the smooth estimator.
int radiusTwoXMoment(uint occupancyWindow)
{
    int negativeOuter = int(occupancyWindow & 1u);
    int negativeInner = int((occupancyWindow >> 1u) & 1u);
    int positiveInner = int((occupancyWindow >> 3u) & 1u);
    int positiveOuter = int((occupancyWindow >> 4u) & 1u);
    return 2 * (positiveOuter - negativeOuter) +
           positiveInner - negativeInner;
}

float3 calculateNormal(int3 workgroupBase, int3 voxelIndex)
{
    int3 sharedCenter = localIndexToSharedIndex(workgroupBase, voxelIndex);
    uint windowStartX = uint(sharedCenter.x - SURFACE_NORMAL_RADIUS);
    int3 normalSum = int3(0);
    for (int z = -SURFACE_NORMAL_RADIUS; z <= SURFACE_NORMAL_RADIUS; ++z)
    {
        for (int y = -SURFACE_NORMAL_RADIUS; y <= SURFACE_NORMAL_RADIUS; ++y)
        {
            uint occupancyRow = surface_shared_occupancy_rows
                [sharedCenter.y + y][sharedCenter.z + z];
            uint occupancyWindow =
                (occupancyRow >> windowStartX) & SURFACE_NORMAL_WINDOW_MASK;
            int solidCount = int(countbits(occupancyWindow));
            normalSum.x += radiusTwoXMoment(occupancyWindow);
            normalSum.y += y * solidCount;
            normalSum.z += z * solidCount;
        }
    }

    float3 normal = float3(normalSum);
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

    uint voxelType = voxelTypeAtLocalIndex(workgroupBase, voxelIndex);
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
