import flora_surface_build;
import flora_types;
import grass_growth_potential;

static const uint INSTANCE_WORKGROUP_SIZE = 128u;

struct U_InstancesToOccupancyInfo
{
    uint3 chunk_world_offset;
    uint3 chunk_dim;
    uint4 species_instance_len;
    uint tick_delta;
};

[[vk::binding(0, 0)]]
ConstantBuffer<U_InstancesToOccupancyInfo> instances_to_occupancy_info;

[[vk::binding(1, 0)]]
GLSLShaderStorageBuffer<B_OccupancyToInstancesResult, Std430DataLayout>
    occupancy_to_instances_result;

[[vk::binding(0, 1)]]
GLSLShaderStorageBuffer<B_ManualFloraInstances, Std430DataLayout>
    manual_flora_instances;

void updateSpeciesGrowth(uint speciesIndex, uint index, uint instanceCount)
{
    if (index >= instanceCount)
    {
        return;
    }

    uint instanceIndex = floraSpeciesInstanceOffset(speciesIndex) + index;
    Instance instance = manual_flora_instances.data[instanceIndex];
    uint progress = unpackInstanceGrowthProgress(instance.packed_local_pos);
    uint3 localPosition = unpackInstanceLocalPosition(instance.packed_local_pos);
    bool isGrass = speciesIndex == FLORA_SPECIES_TALL_GRASS ||
                   speciesIndex == FLORA_SPECIES_SHORT_GRASS;
    uint growthLimit = isGrass
        ? grassGrowthProgressLimit(
              localPosition, instances_to_occupancy_info.chunk_dim)
        : INSTANCE_GROWTH_PROGRESS_MATURE;
    if (progress >= growthLimit)
    {
        return;
    }

    uint remaining = growthLimit - progress;
    progress += min(instances_to_occupancy_info.tick_delta, remaining);
    instance.packed_local_pos =
        setInstanceGrowthProgress(instance.packed_local_pos, progress);
    manual_flora_instances.data[instanceIndex] = instance;

    if (progress < growthLimit)
    {
        uint ignored;
        InterlockedOr(occupancy_to_instances_result.has_growing_flora,
                      1u, ignored);
    }
}

[shader("compute")]
[numthreads(INSTANCE_WORKGROUP_SIZE, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint index = dispatchThreadId.x;
    updateSpeciesGrowth(0u, index,
                        instances_to_occupancy_info.species_instance_len.x);
    updateSpeciesGrowth(1u, index,
                        instances_to_occupancy_info.species_instance_len.y);
    updateSpeciesGrowth(2u, index,
                        instances_to_occupancy_info.species_instance_len.z);
    updateSpeciesGrowth(3u, index,
                        instances_to_occupancy_info.species_instance_len.w);
}
