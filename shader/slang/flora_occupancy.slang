module flora_occupancy;

public static const uint FLORA_OCCUPANCY_SELECTION_GRASS_MIX = 254u;
public static const uint FLORA_OCCUPANCY_SELECTION_NATURAL = 255u;

public static const uint FLORA_OCCUPANCY_OCCUPIED_BIT = 1u;
public static const uint FLORA_OCCUPANCY_GROWTH_SHIFT = 1u;
public static const uint FLORA_OCCUPANCY_GROWTH_MASK = 0xffu;
public static const uint FLORA_OCCUPANCY_SELECTION_SHIFT = 9u;
public static const uint FLORA_OCCUPANCY_SELECTION_MASK = 0xffu;
public static const uint FLORA_OCCUPANCY_SPAWN_AGE_SHIFT = 17u;
public static const uint FLORA_OCCUPANCY_SPAWN_AGE_MASK = 0x3fffu;
public static const uint FLORA_OCCUPANCY_SPAWN_INACTIVE =
    FLORA_OCCUPANCY_SPAWN_AGE_MASK;

public uint packFloraOccupancyWithSpawnAge(uint selection, uint growthProgress,
                                           uint spawnAgeMs)
{
    uint packedSpawnAge = min(spawnAgeMs, FLORA_OCCUPANCY_SPAWN_INACTIVE);
    return FLORA_OCCUPANCY_OCCUPIED_BIT |
           ((growthProgress & FLORA_OCCUPANCY_GROWTH_MASK)
            << FLORA_OCCUPANCY_GROWTH_SHIFT) |
           ((selection & FLORA_OCCUPANCY_SELECTION_MASK)
            << FLORA_OCCUPANCY_SELECTION_SHIFT) |
           ((packedSpawnAge & FLORA_OCCUPANCY_SPAWN_AGE_MASK)
            << FLORA_OCCUPANCY_SPAWN_AGE_SHIFT);
}

public uint packFloraOccupancy(uint selection, uint growthProgress)
{
    return packFloraOccupancyWithSpawnAge(
        selection, growthProgress, FLORA_OCCUPANCY_SPAWN_INACTIVE);
}

public uint unpackFloraOccupancyGrowth(uint occupancyValue)
{
    return (occupancyValue >> FLORA_OCCUPANCY_GROWTH_SHIFT) &
           FLORA_OCCUPANCY_GROWTH_MASK;
}

public uint unpackFloraOccupancySelection(uint occupancyValue)
{
    return (occupancyValue >> FLORA_OCCUPANCY_SELECTION_SHIFT) &
           FLORA_OCCUPANCY_SELECTION_MASK;
}

public uint unpackFloraOccupancySpawnAgeMs(uint occupancyValue)
{
    return (occupancyValue >> FLORA_OCCUPANCY_SPAWN_AGE_SHIFT) &
           FLORA_OCCUPANCY_SPAWN_AGE_MASK;
}
