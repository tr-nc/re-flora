module grass_growth_potential;

import flora_types;

public static const uint GRASS_GROWTH_POTENTIAL_LEVEL_MAX = 15u;
public static const uint3 GRASS_GROWTH_POTENTIAL_CHUNK_DIM = uint3(256u);

struct B_GrassGrowthPotentialLevels
{
    uint words[];
};

[[vk::binding(1, 1)]]
readonly GLSLShaderStorageBuffer<B_GrassGrowthPotentialLevels, Std430DataLayout>
    grass_growth_potential_levels;

public uint grassGrowthPotentialLevel(uint3 localBase, uint3 chunkDimension)
{
    uint linearIndex = localBase.x +
        chunkDimension.x * (localBase.y + chunkDimension.y * localBase.z);
    uint packedWord =
        grass_growth_potential_levels.words[linearIndex >> 3u];
    return (packedWord >> ((linearIndex & 7u) * 4u)) & 0x0fu;
}

public float grassGrowthPotential(uint3 localBase, uint3 chunkDimension)
{
    return float(grassGrowthPotentialLevel(localBase, chunkDimension)) /
           float(GRASS_GROWTH_POTENTIAL_LEVEL_MAX);
}

public float grassGrowthPotential(uint3 localBase)
{
    return grassGrowthPotential(localBase, GRASS_GROWTH_POTENTIAL_CHUNK_DIM);
}

public float floraCompetitionGrowthFactor(uint3 localBase, uint targetSpecies)
{
    bool isGrass = targetSpecies == FLORA_SPECIES_TALL_GRASS ||
                   targetSpecies == FLORA_SPECIES_SHORT_GRASS;
    return isGrass ? grassGrowthPotential(localBase) : 1.0;
}

public uint grassGrowthProgressLimit(uint3 localBase, uint3 chunkDimension)
{
    uint level = grassGrowthPotentialLevel(localBase, chunkDimension);
    return max(1u,
               (level * INSTANCE_GROWTH_PROGRESS_MATURE + 7u) /
                   GRASS_GROWTH_POTENTIAL_LEVEL_MAX);
}
