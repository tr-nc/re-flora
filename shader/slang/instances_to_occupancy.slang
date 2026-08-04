import flora_occupancy;
import flora_types;

static const uint INSTANCE_WORKGROUP_SIZE = 128u;

struct U_InstancesToOccupancyInfo
{
    uint3 chunk_world_offset;
    uint3 chunk_dim;
    uint4 species_instance_len;
    uint tick_delta;
    uint spawn_time_ms;
};

struct B_ManualFloraInstances
{
    Instance data[];
};

[[vk::binding(0, 0)]]
ConstantBuffer<U_InstancesToOccupancyInfo> instances_to_occupancy_info;

[[vk::binding(1, 0)]]
[[vk::image_format("r32ui")]]
RWTexture3D<uint> occupancy_data;

[[vk::binding(0, 1)]]
readonly GLSLShaderStorageBuffer<B_ManualFloraInstances, Std430DataLayout>
    flora_instances;

bool isInChunkWorldBound(uint3 worldPosition)
{
    uint3 minimumBound = instances_to_occupancy_info.chunk_world_offset;
    uint3 maximumBound = minimumBound + instances_to_occupancy_info.chunk_dim;
    return all(worldPosition >= minimumBound) && all(worldPosition < maximumBound);
}

void markBaseOccupied(uint speciesIndex, uint index, uint instanceCount)
{
    if (index >= instanceCount)
    {
        return;
    }

    Instance instance = flora_instances.data[
        floraSpeciesInstanceOffset(speciesIndex) + index];
    uint3 baseWorld = instanceWorldPosition(
        instance, instances_to_occupancy_info.chunk_world_offset);
    if (!isInChunkWorldBound(baseWorld))
    {
        return;
    }

    uint3 baseLocal =
        baseWorld - instances_to_occupancy_info.chunk_world_offset;
    uint spawnAgeMs = instanceSpawnAgeMs(
        instance.spawn_start_ms, instances_to_occupancy_info.spawn_time_ms);
    uint occupancyValue = packFloraOccupancyWithSpawnAge(
        speciesIndex,
        unpackInstanceGrowthProgress(instance.packed_local_pos),
        spawnAgeMs);
    occupancy_data[baseLocal] = occupancyValue;
}

[shader("compute")]
[numthreads(INSTANCE_WORKGROUP_SIZE, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint index = dispatchThreadId.x;
    markBaseOccupied(0u, index,
                     instances_to_occupancy_info.species_instance_len.x);
    markBaseOccupied(1u, index,
                     instances_to_occupancy_info.species_instance_len.y);
    markBaseOccupied(2u, index,
                     instances_to_occupancy_info.species_instance_len.z);
    markBaseOccupied(3u, index,
                     instances_to_occupancy_info.species_instance_len.w);
}
