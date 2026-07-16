module flora_surface_build;

import flora_types;

public struct U_OccupancyToInstancesInfo
{
    public uint3 chunk_world_offset;
    public uint3 chunk_dim;
    public uint spawn_time_ms;
    public uint preserve_existing_placements;
};

public struct B_OccupancyToInstancesResult
{
    public uint flora_instance_len[MAX_FLORA_SPECIES];
    public uint has_growing_flora;
};

public struct B_ManualFloraInstances
{
    public Instance data[];
};
