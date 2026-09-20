import terrain_material;
import tracer_material;
import tracer_types;
import ddgi_types;
import voxel_types;

bool near(float3 a, float3 b)
{
    return all(abs(a - b) < float3(1e-5));
}

export __extern_cpp int main()
{
    float3 base = float3(0.2, 0.3, 0.4);
    float2 variation = float2(0.135, 0.32);
    uint seed = 17u;
    uint failures = 0u;
    uint changed = 0u;
    uint3 neighborsChanged = uint3(0u);

    U_VoxelColors live = {};
    live.dirt_color = float3(0.4);
    live.sand_color = float3(0.6, 0.5, 0.4);
    live.rock_color = float3(0.5);
    live.terrain_variation = variation;
    live.terrain_seed = seed;
    U_DdgiRadianceVoxelPalette frozen = {};
    frozen.dirt_color = live.dirt_color;
    frozen.sand_color = live.sand_color;
    frozen.rock_color = live.rock_color;
    frozen.terrain_variation = variation;
    frozen.terrain_seed = seed;

    for (int i = -64; i < 64; ++i)
    {
        // Includes negative coordinates and crosses the world/chunk boundary at zero.
        float3 cell = float3(i, -i, 255 + i);
        float3 p = (cell + 0.5) / 256.0;
        float3 a = applyTerrainMaterial(base, VOXEL_TYPE_DIRT, p, variation, seed);
        if (!all(a == applyTerrainMaterial(base, VOXEL_TYPE_DIRT, p, variation, seed))) failures++;
        if (!near(a, applyTerrainMaterial(base, VOXEL_TYPE_DIRT, p, variation, seed + 1u))) changed++;
        for (uint axis = 0u; axis < 3u; ++axis)
        {
            float3 offset = float3(0.0);
            offset[axis] = 1.0 / 256.0;
            if (!near(a, applyTerrainMaterial(base, VOXEL_TYPE_DIRT, p + offset, variation, seed)))
                neighborsChanged[axis]++;
        }
        for (uint voxelType = VOXEL_TYPE_DIRT; voxelType <= VOXEL_TYPE_PETAL; ++voxelType)
        {
            if (!all(terrainColorAtVoxel(live, voxelType, p) ==
                     ddgiTerrainColorAtVoxel(frozen, voxelType, p))) failures++;
            float3 value = applyTerrainMaterial(base, voxelType, p, variation, seed);
            if (any(isnan(value)) || any(isinf(value)) || any(value < 0.0) || any(value > 1.0)) failures++;
            bool eligible = voxelType == VOXEL_TYPE_DIRT || voxelType == VOXEL_TYPE_SAND || voxelType == VOXEL_TYPE_ROCK;
            if (!eligible && !all(value == base)) failures++;
            // The entire cell has one color: no gradual interpolation inside or between cells.
            if (!all(value == applyTerrainMaterial(base, voxelType, (cell + 0.01) / 256.0, variation, seed))) failures++;
            if (!all(value == applyTerrainMaterial(base, voxelType, (cell + 0.99) / 256.0, variation, seed))) failures++;
            float3 chunkOrigin = float3(-1.0, 0.0, 1.0);
            if (!all(value == applyTerrainMaterial(base, voxelType, (p - chunkOrigin) + chunkOrigin, variation, seed))) failures++;
        }
    }
    if (changed < 120u || any(neighborsChanged < uint3(120u))) failures++;

    // A zero variation strength naturally returns the authored base, without a legacy mode.
    for (uint voxelType = VOXEL_TYPE_DIRT; voxelType <= VOXEL_TYPE_PETAL; ++voxelType)
        if (!all(applyTerrainMaterial(base, voxelType, float3(0.3), float2(0.0), seed) == base)) failures++;

    float3 patterned = terrainColorAtVoxel(live, VOXEL_TYPE_DIRT, float3(0.5));
    if (!near(applyTerrainMoisture(patterned, VOXEL_TYPE_DIRT, 2u),
              lerp(patterned * 0.52, float3(0.030, 0.038, 0.032), 0.28))) failures++;
    if (!all(applyTerrainMoisture(patterned, VOXEL_TYPE_ROCK, 3u) == patterned)) failures++;
    live.terrain_variation = float2(0.0);
    frozen.terrain_variation = float2(0.0);
    for (uint voxelType = VOXEL_TYPE_DIRT; voxelType <= VOXEL_TYPE_PETAL; ++voxelType)
    {
        if (!all(terrainColorAtVoxel(live, voxelType, float3(-0.5)) ==
                 voxelColorByType(live, voxelType))) failures++;
        if (!all(ddgiTerrainColorAtVoxel(frozen, voxelType, float3(-0.5)) ==
                 ddgiVoxelColorByType(frozen, voxelType))) failures++;
    }
    printf("terrain_material failures=%u reseeded=%u neighbors_changed=%u,%u,%u\n",
           failures, changed, neighborsChanged.x, neighborsChanged.y, neighborsChanged.z);
    return failures == 0u ? 0 : 1;
}
