import flora_instance_placement;
import flora_occupancy;
import flora_placement;
import flora_surface_build;
import flora_types;
import grass_growth_potential;
import voxel_data;

static const uint OCCUPANCY_WORKGROUP_SIZE = 8u;

[[vk::binding(0, 0)]]
ConstantBuffer<U_OccupancyToInstancesInfo> occupancy_to_instances_info;

[[vk::binding(1, 0)]]
GLSLShaderStorageBuffer<B_OccupancyToInstancesResult, Std430DataLayout>
    occupancy_to_instances_result;

[[vk::binding(2, 0)]]
[[vk::image_format("r32ui")]]
readonly RWTexture3D<uint> occupancy_data;

[[vk::binding(3, 0)]]
[[vk::image_format("r32ui")]]
readonly RWTexture3D<uint> surface;

[[vk::binding(0, 1)]]
writeonly GLSLShaderStorageBuffer<B_ManualFloraInstances, Std430DataLayout>
    flora_instances;

void addFloraInstance(int3 baseWorldPosition, uint speciesIndex,
                      uint growthProgress, uint growthLimit,
                      uint spawnAgeMs)
{
    uint writeIndex;
    InterlockedAdd(
        occupancy_to_instances_result.flora_instance_len[speciesIndex],
        1u, writeIndex);

    uint spawnStartMs = spawnAgeMs == FLORA_OCCUPANCY_SPAWN_INACTIVE
        ? INSTANCE_SPAWN_INACTIVE
        : occupancy_to_instances_info.spawn_time_ms - spawnAgeMs;
    Instance instance;
    initializeFloraInstance(
        instance,
        uint3(baseWorldPosition) -
            occupancy_to_instances_info.chunk_world_offset,
        growthProgress,
        spawnStartMs);
    flora_instances.data[
        floraSpeciesInstanceOffset(speciesIndex) + writeIndex] = instance;

    if (growthProgress < growthLimit)
    {
        uint ignored;
        InterlockedOr(occupancy_to_instances_result.has_growing_flora,
                      1u, ignored);
    }
}

[shader("compute")]
[numthreads(OCCUPANCY_WORKGROUP_SIZE,
            OCCUPANCY_WORKGROUP_SIZE,
            OCCUPANCY_WORKGROUP_SIZE)]
void main(uint3 baseLocal : SV_DispatchThreadID)
{
    if (any(baseLocal >= occupancy_to_instances_info.chunk_dim))
    {
        return;
    }

    uint occupancyValue = occupancy_data[baseLocal];
    if (occupancyValue == 0u)
    {
        return;
    }

    uint growthProgress = unpackFloraOccupancyGrowth(occupancyValue);
    uint spawnAgeMs = unpackFloraOccupancySpawnAgeMs(occupancyValue);
    uint paintSelection = unpackFloraOccupancySelection(occupancyValue);
    bool isGrassSelection = paintSelection == FLORA_SPECIES_TALL_GRASS ||
                            paintSelection == FLORA_SPECIES_SHORT_GRASS ||
                            paintSelection ==
                                FLORA_OCCUPANCY_SELECTION_GRASS_MIX;
    uint growthLimit = isGrassSelection
        ? grassGrowthProgressLimit(
              baseLocal, occupancy_to_instances_info.chunk_dim)
        : INSTANCE_GROWTH_PROGRESS_MATURE;

    int3 worldBasePosition = int3(
        occupancy_to_instances_info.chunk_world_offset + baseLocal);
    if (occupancy_to_instances_info.preserve_existing_placements != 0u)
    {
        addFloraInstance(worldBasePosition, paintSelection,
                         min(growthProgress, growthLimit), growthLimit,
                         spawnAgeMs);
        return;
    }

    uint surfaceData = surface[baseLocal];
    if (surfaceData == 0u)
    {
        return;
    }
    uint voxelType = voxelTypeFromData(surfaceData);
    float3 normal = voxelSurfaceNormalFromData(surfaceData);

    bool allowAnyFlora;
    bool allowTallGrass;
    bool allowShortGrass;
    calculateFloraDensityPlacement(worldBasePosition, allowAnyFlora,
                                   allowTallGrass, allowShortGrass);

    if (paintSelection != FLORA_SPECIES_LAVENDER &&
        paintSelection != FLORA_SPECIES_EMBER_BLOOM &&
        !(allowAnyFlora || allowTallGrass || allowShortGrass))
    {
        return;
    }
    if (!checkFloraSurfacePlantability(
            voxelType, normal, worldBasePosition))
    {
        return;
    }

    uint floraType;
    if (!selectPaintedFloraType(
            paintSelection, worldBasePosition, allowAnyFlora,
            allowTallGrass, allowShortGrass, floraType))
    {
        return;
    }

    addFloraInstance(worldBasePosition, floraType,
                     min(growthProgress, growthLimit), growthLimit,
                     spawnAgeMs);
}
