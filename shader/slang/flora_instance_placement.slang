module flora_instance_placement;

import flora_occupancy;
import flora_placement;
import flora_surface_planting;
import flora_types;
import gradient_noise;

public bool checkFloraSurfacePlantability(uint voxelType, float3 normal,
                                           int3 worldPosition)
{
    if (!isFloraSurfacePlantable(voxelType))
    {
        return false;
    }

    float surfaceFlatness = dot(normal, float3(0.0, 1.0, 0.0));
    float spawnChance = smoothstep(0.5, 1.0, surfaceFlatness);
    float stiffnessRandom = floraLegacyHash(
        float2(worldPosition.x + 123.0, worldPosition.z + 456.0));
    return stiffnessRandom <= spawnChance;
}

bool selectFloraBiome(int3 worldPosition, out uint floraType)
{
    float biomeValue = seededClassicGradientNoise2D(
        float2(float(worldPosition.x), float(worldPosition.z)) * 0.02,
        1337u);
    float speciesRandom = floraSparseSpeciesRandom(worldPosition);
    floraType = FLORA_SPECIES_TALL_GRASS;

    if (biomeValue > 0.3)
    {
        if (speciesRandom < FLORA_LAVENDER_SPARSE_THRESHOLD)
        {
            floraType = FLORA_SPECIES_LAVENDER;
        }
        else if (speciesRandom < FLORA_GRASS_DECREASED_SPARSE_THRESHOLD)
        {
            floraType = FLORA_SPECIES_TALL_GRASS;
        }
        else
        {
            return false;
        }
    }
    else if (biomeValue < -0.3)
    {
        if (speciesRandom < FLORA_EMBER_BLOOM_SPARSE_THRESHOLD)
        {
            floraType = FLORA_SPECIES_EMBER_BLOOM;
        }
        else if (speciesRandom < FLORA_GRASS_DECREASED_SPARSE_THRESHOLD)
        {
            floraType = FLORA_SPECIES_TALL_GRASS;
        }
        else
        {
            return false;
        }
    }
    return true;
}

public bool selectFloraType(int3 worldBasePosition,
                            bool allowAnyFlora,
                            bool allowTallGrass,
                            bool allowShortGrass,
                            out uint floraType)
{
    if (!allowAnyFlora)
    {
        if (allowTallGrass)
        {
            floraType = FLORA_SPECIES_TALL_GRASS;
            return true;
        }
        if (allowShortGrass)
        {
            floraType = FLORA_SPECIES_SHORT_GRASS;
            return true;
        }
        return false;
    }

    uint biomeFloraType;
    if (!selectFloraBiome(worldBasePosition, biomeFloraType))
    {
        if (allowTallGrass)
        {
            floraType = FLORA_SPECIES_TALL_GRASS;
            return true;
        }
        if (allowShortGrass)
        {
            floraType = FLORA_SPECIES_SHORT_GRASS;
            return true;
        }
        return false;
    }

    if (biomeFloraType == FLORA_SPECIES_LAVENDER ||
        biomeFloraType == FLORA_SPECIES_EMBER_BLOOM)
    {
        floraType = biomeFloraType;
        return true;
    }
    if (allowTallGrass)
    {
        floraType = FLORA_SPECIES_TALL_GRASS;
        return true;
    }
    if (allowShortGrass)
    {
        floraType = FLORA_SPECIES_SHORT_GRASS;
        return true;
    }
    return false;
}

bool selectGrassMixType(bool allowAnyFlora,
                        bool allowTallGrass,
                        bool allowShortGrass,
                        out uint floraType)
{
    if (allowAnyFlora || allowTallGrass)
    {
        floraType = FLORA_SPECIES_TALL_GRASS;
        return true;
    }
    if (allowShortGrass)
    {
        floraType = FLORA_SPECIES_SHORT_GRASS;
        return true;
    }
    return false;
}

public bool selectPaintedFloraType(uint paintSelection,
                                   int3 worldBasePosition,
                                   bool allowAnyFlora,
                                   bool allowTallGrass,
                                   bool allowShortGrass,
                                   out uint floraType)
{
    if (paintSelection == FLORA_OCCUPANCY_SELECTION_NATURAL)
    {
        return selectFloraType(worldBasePosition, allowAnyFlora,
                               allowTallGrass, allowShortGrass, floraType);
    }
    if (paintSelection == FLORA_OCCUPANCY_SELECTION_GRASS_MIX)
    {
        return selectGrassMixType(allowAnyFlora, allowTallGrass,
                                  allowShortGrass, floraType);
    }
    if (paintSelection < FLORA_SPECIES_COUNT)
    {
        floraType = paintSelection;
        return true;
    }
    return false;
}
