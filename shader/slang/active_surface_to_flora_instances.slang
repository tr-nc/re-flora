import flora_instance_placement;
import flora_placement;
import flora_surface_build;
import flora_types;
import grass_growth_potential;
import surface_build;
import surface_result;
import voxel_data;

static const uint FLORA_WORKGROUP_SIZE = 128u;

[[vk::binding(0, 0)]]
ConstantBuffer<U_OccupancyToInstancesInfo> occupancy_to_instances_info;

[[vk::binding(1, 0)]]
GLSLShaderStorageBuffer<B_OccupancyToInstancesResult, Std430DataLayout>
    occupancy_to_instances_result;

[[vk::binding(2, 0)]]
[[vk::image_format("r32ui")]]
readonly RWTexture3D<uint> surface;

[[vk::binding(3, 0)]]
readonly GLSLShaderStorageBuffer<B_SurfaceActiveBrickIndices, Std430DataLayout>
    surface_active_brick_indices;

[[vk::binding(4, 0)]]
readonly GLSLShaderStorageBuffer<B_MakeSurfaceResult, Std430DataLayout>
    make_surface_result;

[[vk::binding(0, 1)]]
writeonly GLSLShaderStorageBuffer<B_ManualFloraInstances, Std430DataLayout>
    flora_instances;

void addFloraInstance(int3 baseWorldPosition, uint speciesIndex,
                      uint growthProgress, uint growthLimit)
{
    uint writeIndex;
    InterlockedAdd(
        occupancy_to_instances_result.flora_instance_len[speciesIndex],
        1u, writeIndex);

    Instance instance;
    initializeFloraInstance(
        instance,
        uint3(baseWorldPosition) -
            occupancy_to_instances_info.chunk_world_offset,
        growthProgress,
        INSTANCE_SPAWN_INACTIVE);
    flora_instances.data[
        floraSpeciesInstanceOffset(speciesIndex) + writeIndex] = instance;

    if (growthProgress < growthLimit)
    {
        uint ignored;
        InterlockedOr(occupancy_to_instances_result.has_growing_flora,
                      1u, ignored);
    }
}

uint3 decodeActiveSurfaceLocalPosition(uint linearIndex)
{
    uint brickListIndex = linearIndex >> 6u;
    uint voxelInBrick = linearIndex & 63u;

    uint brickIndex = surface_active_brick_indices.data[brickListIndex];
    uint brickDimension = occupancy_to_instances_info.chunk_dim.x >> 2u;
    uint brickX = brickIndex % brickDimension;
    uint brickZ = (brickIndex / brickDimension) % brickDimension;
    uint brickY = brickIndex / (brickDimension * brickDimension);

    uint3 voxelOffset = uint3(
        voxelInBrick & 3u,
        (voxelInBrick >> 4u) & 3u,
        (voxelInBrick >> 2u) & 3u);
    return uint3(brickX, brickY, brickZ) * 4u + voxelOffset;
}

[shader("compute")]
[numthreads(FLORA_WORKGROUP_SIZE, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint linearIndex = dispatchThreadId.x;
    uint maximumIndex = make_surface_result.active_brick_len * 64u;
    if (linearIndex >= maximumIndex)
    {
        return;
    }

    uint3 baseLocal = decodeActiveSurfaceLocalPosition(linearIndex);
    if (any(baseLocal >= occupancy_to_instances_info.chunk_dim))
    {
        return;
    }

    uint surfaceData = surface[baseLocal];
    if (surfaceData == 0u)
    {
        return;
    }
    uint voxelType = voxelTypeFromData(surfaceData);
    float3 normal = voxelSurfaceNormalFromData(surfaceData);
    int3 worldBasePosition = int3(
        occupancy_to_instances_info.chunk_world_offset + baseLocal);

    bool allowAnyFlora;
    bool allowTallGrass;
    bool allowShortGrass;
    calculateFloraDensityPlacement(worldBasePosition, allowAnyFlora,
                                   allowTallGrass, allowShortGrass);
    if (!(allowAnyFlora || allowTallGrass || allowShortGrass))
    {
        return;
    }
    if (!checkFloraSurfacePlantability(
            voxelType, normal, worldBasePosition))
    {
        return;
    }

    uint floraType;
    if (!selectFloraType(worldBasePosition, allowAnyFlora,
                         allowTallGrass, allowShortGrass, floraType))
    {
        return;
    }

    uint growthLimit = grassGrowthProgressLimit(
        baseLocal, occupancy_to_instances_info.chunk_dim);
    addFloraInstance(worldBasePosition, floraType,
                     growthLimit, growthLimit);
}
