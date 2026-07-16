import definitions;
import packer;
import surface_build;
import voxel_data;
import voxel_types;

static const uint GROUP_SIZE = 8u;
static const uint GROUP_SIZE_3 = GROUP_SIZE * GROUP_SIZE * GROUP_SIZE;
static const uint SHARED_SIZE = GROUP_SIZE + 4u;
static const uint SHARED_SIZE_3 = SHARED_SIZE * SHARED_SIZE * SHARED_SIZE;

[[vk::binding(0, 0)]]
ConstantBuffer<U_MakeSurfaceInfo> make_surface_info;

[[vk::binding(1, 0)]]
GLSLShaderStorageBuffer<B_MakeSurfaceResult, Std430DataLayout> make_surface_result;

[[vk::binding(2, 0)]]
[[vk::image_format("r32ui")]]
WTexture3D<uint> surface;

[[vk::binding(3, 0)]]
[[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;

struct B_SurfaceActiveBrickFlags
{
    uint data[];
};

[[vk::binding(4, 0)]]
GLSLShaderStorageBuffer<B_SurfaceActiveBrickFlags, Std430DataLayout>
    surface_active_brick_flags;

struct B_SurfaceActiveBrickIndices
{
    uint data[];
};

[[vk::binding(5, 0)]]
writeonly GLSLShaderStorageBuffer<B_SurfaceActiveBrickIndices, Std430DataLayout>
    surface_active_brick_indices;

[[vk::binding(6, 0)]]
readonly GLSLShaderStorageBuffer<B_SurfaceSolidWorkgroupIndices, Std430DataLayout>
    surface_solid_workgroup_indices;

groupshared uint shared_data[SHARED_SIZE][SHARED_SIZE][SHARED_SIZE];

uint voxelHashBucketFromBlueNoise(int3 worldPos)
{
    uint hash = uint(worldPos.x) * 73856093u;
    hash ^= uint(worldPos.y) * 19349663u;
    hash ^= uint(worldPos.z) * 83492791u;
    hash ^= hash >> 16u;
    hash *= 0x45d9f3bu;
    hash ^= hash >> 16u;
    return hash & 0x3u;
}

uint3 decodeSurfaceWorkgroupCoord(uint localWorkgroupIndex)
{
    uint3 gridDim = (make_surface_info.atlas_read_dim + uint3(GROUP_SIZE - 1u)) / GROUP_SIZE;
    return uint3(localWorkgroupIndex % gridDim.x,
                 localWorkgroupIndex / (gridDim.x * gridDim.z),
                 (localWorkgroupIndex / gridDim.x) % gridDim.z);
}

int3 sharedIndexToAtlasIndex(int3 chunkBase, int3 workgroupBase, int3 sharedIndex)
{
    return sharedIndex + workgroupBase - int3(2) + chunkBase;
}

int3 localIndexToSharedIndex(int3 workgroupBase, int3 localIndex)
{
    return localIndex - (workgroupBase - int3(2));
}

uint loadAtlas(int3 atlasIndex, int3 atlasReadOffset, int3 atlasReadDim,
               bool isCrossingBoundary)
{
    if (isCrossingBoundary)
    {
        return voxelTypeFromAtlasData(chunk_atlas[uint3(atlasIndex)]);
    }

    if (any(atlasIndex < atlasReadOffset) ||
        any(atlasIndex >= atlasReadOffset + atlasReadDim))
    {
        return 0u;
    }
    return voxelTypeFromAtlasData(chunk_atlas[uint3(atlasIndex)]);
}

void preload(int3 workgroupBase, uint localInvocationIndex)
{
    for (uint linearIndex = localInvocationIndex; linearIndex < SHARED_SIZE_3;
         linearIndex += GROUP_SIZE_3)
    {
        uint3 sharedIndex;
        sharedIndex.x = linearIndex % SHARED_SIZE;
        sharedIndex.y = (linearIndex / SHARED_SIZE) % SHARED_SIZE;
        sharedIndex.z = linearIndex / (SHARED_SIZE * SHARED_SIZE);

        int3 atlasIndex = sharedIndexToAtlasIndex(int3(make_surface_info.atlas_read_offset),
                                                  workgroupBase, int3(sharedIndex));
        uint value = loadAtlas(atlasIndex, int3(make_surface_info.atlas_read_offset),
                               int3(make_surface_info.atlas_read_dim),
                               make_surface_info.is_crossing_boundary != 0u);
        shared_data[sharedIndex.x][sharedIndex.y][sharedIndex.z] = value;
    }
}

bool isSolid(uint voxelType)
{
    return voxelType != VOXEL_TYPE_EMPTY;
}

uint voxelAtLocalIndex(int3 workgroupBase, int3 localIndex)
{
    int3 sharedIndex = localIndexToSharedIndex(workgroupBase, localIndex);
    return shared_data[sharedIndex.x][sharedIndex.y][sharedIndex.z];
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
        int3 current = voxelIndex + NEIGHBORS[index];
        if (!isSolid(voxelAtLocalIndex(workgroupBase, current)))
        {
            return false;
        }
    }
    return true;
}

void calculateNormal(out float3 outputNormal, int3 workgroupBase, int3 voxelIndex)
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
    outputNormal = normalLengthSquared > EPSILON * EPSILON
        ? -normal * rsqrt(normalLengthSquared)
        : float3(0.0, 1.0, 0.0);
}

uint compressVoxelTypeAndNormal(uint voxelType, float3 normal, int3 worldPos)
{
    uint normalBits = packNormalOct16(normal);
    uint hashId = voxelHashBucketFromBlueNoise(worldPos);
    return packVoxelSurfaceData(voxelType, normalBits, hashId);
}

[shader("compute")]
[numthreads(GROUP_SIZE, GROUP_SIZE, GROUP_SIZE)]
void main(uint3 groupId : SV_GroupID,
          uint3 localInvocationId : SV_GroupThreadID,
          uint localInvocationIndex : SV_GroupIndex)
{
    uint workgroupListIndex = groupId.x;
    if (workgroupListIndex >= make_surface_result.solid_workgroup_len)
    {
        return;
    }

    uint localWorkgroupIndex = surface_solid_workgroup_indices.data[workgroupListIndex];
    int3 workgroupBase = int3(decodeSurfaceWorkgroupCoord(localWorkgroupIndex) * GROUP_SIZE);

    preload(workgroupBase, localInvocationIndex);
    GroupMemoryBarrierWithGroupSync();

    int3 voxelIndex = workgroupBase + int3(localInvocationId);
    if (any(voxelIndex >= int3(make_surface_info.atlas_read_dim)))
    {
        return;
    }

    uint voxelType = voxelAtLocalIndex(workgroupBase, voxelIndex);
    if (!isSolid(voxelType))
    {
        return;
    }
    if (isOccluded(workgroupBase, voxelIndex))
    {
        return;
    }

    int3 worldPos = int3(make_surface_info.atlas_read_offset) + voxelIndex;
    float3 normal;
    calculateNormal(normal, workgroupBase, voxelIndex);
    uint properties = compressVoxelTypeAndNormal(voxelType, normal, worldPos);

    uint oldActiveVoxelLength;
    InterlockedAdd(make_surface_result.active_voxel_len, 1u, oldActiveVoxelLength);

    uint3 brickCoord = uint3(voxelIndex) >> 2u;
    uint brickDim = make_surface_info.atlas_read_dim.x >> 2u;
    uint brickIndex = brickCoord.x + brickCoord.z * brickDim +
                      brickCoord.y * brickDim * brickDim;
    uint flagWord = brickIndex >> 5u;
    uint flagMask = 1u << (brickIndex & 31u);
    uint oldFlags;
    InterlockedOr(surface_active_brick_flags.data[flagWord], flagMask, oldFlags);
    if ((oldFlags & flagMask) == 0u)
    {
        uint activeBrickIndex;
        InterlockedAdd(make_surface_result.active_brick_len, 1u, activeBrickIndex);
        surface_active_brick_indices.data[activeBrickIndex] = brickIndex;
    }

    surface.Store(uint3(voxelIndex), properties);
}
