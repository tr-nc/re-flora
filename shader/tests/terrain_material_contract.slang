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
    float4 soil = float4(16.0, 0.35, 0.25, 0.0);
    float4 rock = float4(12.0, 0.32, sin(0.25), cos(0.25));
    uint seed = 17u;
    uint failures = 0u;
    uint changed = 0u;
    float maxNeighborDelta = 0.0;
    float maxSeamDelta = 0.0;

    U_VoxelColors live = {};
    live.dirt_color = float3(0.4);
    live.sand_color = float3(0.6, 0.5, 0.4);
    live.rock_color = float3(0.5);
    live.terrain_soil = soil;
    live.terrain_rock = rock;
    live.terrain_seed = seed;
    live.terrain_enabled = 1u;
    U_DdgiRadianceVoxelPalette frozen = {};
    frozen.dirt_color = live.dirt_color;
    frozen.sand_color = live.sand_color;
    frozen.rock_color = live.rock_color;
    frozen.terrain_soil = soil;
    frozen.terrain_rock = rock;
    frozen.terrain_seed = seed;
    frozen.terrain_enabled = 1u;

    for (uint i = 0u; i < 128u; ++i)
    {
        float3 p = float3(float(i) * 0.03125, 0.501953125, 1.001953125);
        float3 a = applyTerrainMaterial(base, VOXEL_TYPE_DIRT, p, soil, rock, seed, 1u);
        float3 repeated = applyTerrainMaterial(base, VOXEL_TYPE_DIRT, p, soil, rock, seed, 1u);
        float3 reseeded = applyTerrainMaterial(base, VOXEL_TYPE_DIRT, p, soil, rock, seed + 1u, 1u);
        if (!all(a == repeated)) failures++;
        if (!near(a, reseeded)) changed++;
        float3 neighbor = applyTerrainMaterial(base, VOXEL_TYPE_DIRT,
            p + float3(1.0 / 256.0, 0.0, 0.0), soil, rock, seed, 1u);
        maxNeighborDelta = max(maxNeighborDelta, max(abs(a - neighbor).x,
            max(abs(a - neighbor).y, abs(a - neighbor).z)));
        for (uint voxelType = VOXEL_TYPE_DIRT; voxelType <= VOXEL_TYPE_PETAL; ++voxelType)
        {
            if (!near(terrainColorAtVoxel(live, voxelType, p),
                      ddgiTerrainColorAtVoxel(frozen, voxelType, p))) failures++;
            if (!all(applyTerrainMaterial(base, voxelType, p, soil, rock, seed, 0u) == base)) failures++;
            float3 value = applyTerrainMaterial(base, voxelType, p, soil, rock, seed, 1u);
            if (any(isnan(value)) || any(isinf(value)) || any(value < 0.0) || any(value > 1.0)) failures++;
            bool eligible = voxelType == VOXEL_TYPE_DIRT || voxelType == VOXEL_TYPE_SAND || voxelType == VOXEL_TYPE_ROCK;
            if (!eligible && !all(value == base)) failures++;
        }
        float3 chunkOrigin = float3(1.0, 0.0, 1.0);
        float3 fromLocal = terrainColorAtVoxel(live, VOXEL_TYPE_ROCK, (p - chunkOrigin) + chunkOrigin);
        if (!near(fromLocal, terrainColorAtVoxel(live, VOXEL_TYPE_ROCK, p))) failures++;
    }
    // Cross an actual world/chunk boundary, including a noise lattice boundary at x=256.
    float3 left = terrainColorAtVoxel(live, VOXEL_TYPE_DIRT, float3(1.0 - 1e-6, 0.5, 0.5));
    float3 right = terrainColorAtVoxel(live, VOXEL_TYPE_DIRT, float3(1.0 + 1e-6, 0.5, 0.5));
    maxSeamDelta = max(abs(left - right).x, max(abs(left - right).y, abs(left - right).z));
    if (maxSeamDelta > 1e-4 || maxNeighborDelta > 0.05 || changed < 120u) failures++;

    // Strength zero is the exact original palette even when the feature is enabled.
    soil.y = 0.0;
    rock.y = 0.0;
    for (uint voxelType = VOXEL_TYPE_DIRT; voxelType <= VOXEL_TYPE_PETAL; ++voxelType)
        if (!all(applyTerrainMaterial(base, voxelType, float3(0.3), soil, rock, seed, 1u) == base)) failures++;

    float3 patterned = terrainColorAtVoxel(live, VOXEL_TYPE_DIRT, float3(0.5));
    if (!near(applyTerrainMoisture(patterned, VOXEL_TYPE_DIRT, 2u),
              lerp(patterned * 0.52, float3(0.030, 0.038, 0.032), 0.28))) failures++;
    if (!all(applyTerrainMoisture(patterned, VOXEL_TYPE_ROCK, 3u) == patterned)) failures++;
    // A/B off must bypass the material in both production palette wrappers, including the
    // cottage material IDs introduced on main since this candidate was first authored.
    live.terrain_enabled = 0u;
    frozen.terrain_enabled = 0u;
    for (uint voxelType = VOXEL_TYPE_DIRT; voxelType <= VOXEL_TYPE_PETAL; ++voxelType)
    {
        if (!all(terrainColorAtVoxel(live, voxelType, float3(-0.5)) ==
                 voxelColorByType(live, voxelType))) failures++;
        if (!all(ddgiTerrainColorAtVoxel(frozen, voxelType, float3(-0.5)) ==
                 ddgiVoxelColorByType(frozen, voxelType))) failures++;
    }
    printf("terrain_material failures=%u reseeded=%u neighbor_delta=%.6f seam_delta=%.8f\n",
           failures, changed, maxNeighborDelta, maxSeamDelta);
    return failures == 0u ? 0 : 1;
}
