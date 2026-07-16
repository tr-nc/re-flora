module flora_types;

public static const int FLORA_HEIGHT_COLOR_TABLE_LEN = 12;
public static const uint MAX_FLORA_SPECIES = 5u;
public static const uint FLORA_SPECIES_COUNT = 5u;
public static const uint FLORA_SPECIES_TALL_GRASS = 0u;
public static const uint FLORA_SPECIES_SHORT_GRASS = 1u;
public static const uint FLORA_SPECIES_LAVENDER = 2u;
public static const uint FLORA_SPECIES_EMBER_BLOOM = 3u;
public static const uint FLORA_SPECIES_KOCHIA = 4u;
public static const uint FLORA_SPECIES_TREE_LEAF = 5u;
public static const uint FLORA_SPECIES_APPLE = 6u;

public static const uint INSTANCE_SPAWN_INACTIVE = 0xffffffffu;
public static const uint INSTANCE_GROWTH_PROGRESS_MATURE = 0xffu;

public static const uint FLORA_VOXEL_MATERIAL_ALLIUM_CORE = 1u;
public static const uint FLORA_VOXEL_MATERIAL_BITS = 3u;
public static const uint FLORA_VOXEL_MATERIAL_MASK =
    (1u << FLORA_VOXEL_MATERIAL_BITS) - 1u;

public struct PushConstantFlora
{
    public float time;
    public uint instance_ty;
    public uint3 chunk_world_offset;
    public uint _padding_after_chunk_world_offset;
    public uint height_dark_color_rgb10[FLORA_HEIGHT_COLOR_TABLE_LEN];
    public uint height_light_color_rgb10[FLORA_HEIGHT_COLOR_TABLE_LEN];
};

public struct U_FloraGrowthInfo
{
    public uint flora_tick;
    public uint sprout_delay_ticks;
    public uint full_growth_ticks;
    public uint spawn_time_ms;
    public float spawn_duration_seconds;
    public float spawn_rise_fraction;
    public float spawn_overshoot_min_voxels;
    public float spawn_overshoot_max_voxels;
    public float spawn_stagger_seconds;
    public float4 moisture_growth_factors[MAX_FLORA_SPECIES];
};

public struct U_WindVolumeInfo
{
    public float3 world_chunk_extent;
};

public struct Instance
{
    public uint packed_local_pos;
    public uint spawn_start_ms;
};

public struct FloraSpawnAnimationPose
{
    public float3 translation_voxels;
    public float3 scale;
    public float visibility;
};

public uint3 unpackInstanceLocalPosition(uint packedLocalPosition)
{
    return uint3(packedLocalPosition & 0xffu,
                 (packedLocalPosition >> 8u) & 0xffu,
                 (packedLocalPosition >> 16u) & 0xffu);
}

public uint unpackInstanceGrowthProgress(uint packedLocalPosition)
{
    return (packedLocalPosition >> 24u) & 0xffu;
}

public uint3 surfaceFloraStemWorldPosition(uint packedLocalPosition,
                                           uint3 chunkWorldOffset)
{
    return chunkWorldOffset + unpackInstanceLocalPosition(packedLocalPosition) +
           uint3(0u, 1u, 0u);
}

public uint instanceSeed(uint3 instancePositionVoxels)
{
    uint seed = instancePositionVoxels.x ^ (instancePositionVoxels.y << 10u) ^
                (instancePositionVoxels.z << 20u);
    seed ^= seed >> 16u;
    seed ^= seed << 5u;
    seed ^= seed >> 11u;
    return seed;
}

public uint instanceSpawnAgeMs(uint spawnStartMs, uint nowMs)
{
    return spawnStartMs == INSTANCE_SPAWN_INACTIVE
        ? INSTANCE_SPAWN_INACTIVE
        : nowMs - spawnStartMs;
}

public float floraVoxelInfoUnorm8(uint info, uint shift)
{
    return float((info >> shift) & 0xffu) * (1.0 / 255.0);
}

public float floraVoxelColorGradient(uint info)
{
    return floraVoxelInfoUnorm8(info, 0u);
}

public float floraVoxelWindGradient(uint info)
{
    return floraVoxelInfoUnorm8(info, 8u);
}

public float floraVoxelGrowthGradient(uint info)
{
    return floraVoxelInfoUnorm8(info, 16u);
}

public uint floraVoxelMaterialId(uint info)
{
    return (info >> 24u) & FLORA_VOXEL_MATERIAL_MASK;
}

public uint floraVoxelAnimationGroup(uint info)
{
    return info >> (24u + FLORA_VOXEL_MATERIAL_BITS);
}
