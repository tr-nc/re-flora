module surface_flora_vertex;

import flora_types;
import flora_vertex;
import flora_lighting_resources;
import grass_moisture_material;
import plant_water_status;

struct B_GrassGrowthPotentialLevels
{
    uint words[];
};

[[vk::binding(1, 1)]]
readonly GLSLShaderStorageBuffer<B_GrassGrowthPotentialLevels, Std430DataLayout>
    grass_growth_potential_levels;

static const uint GRASS_GROWTH_POTENTIAL_LEVEL_MAX = 15u;
static const uint3 GRASS_GROWTH_POTENTIAL_CHUNK_DIM = uint3(256u);

uint grassGrowthPotentialLevel(uint3 localBase)
{
    uint linearIndex = localBase.x + GRASS_GROWTH_POTENTIAL_CHUNK_DIM.x *
        (localBase.y + GRASS_GROWTH_POTENTIAL_CHUNK_DIM.y * localBase.z);
    uint word = grass_growth_potential_levels.words[linearIndex >> 3u];
    return (word >> ((linearIndex & 7u) * 4u)) & 0x0fu;
}

public float floraCompetitionGrowthFactor(uint3 localBase, uint targetSpecies)
{
    bool isGrass = targetSpecies == FLORA_SPECIES_TALL_GRASS ||
                   targetSpecies == FLORA_SPECIES_SHORT_GRASS;
    return isGrass
        ? float(grassGrowthPotentialLevel(localBase)) /
          float(GRASS_GROWTH_POTENTIAL_LEVEL_MAX)
        : 1.0;
}

public struct FloraSupportSample
{
    public float growth_factor;
    public uint moisture_level;
};

public FloraSupportSample floraSupportSample(uint3 localBase,
                                             uint3 chunkWorldOffset,
                                             uint targetSpecies)
{
    FloraSupportSample sample;
    sample.growth_factor = 1.0;
    sample.moisture_level = 0u;
    if (targetSpecies >= FLORA_SPECIES_COUNT) return sample;
    uint3 atlasPosition = chunkWorldOffset + localBase;
    uint width;
    uint height;
    uint depth;
    chunk_atlas.GetDimensions(width, height, depth);
    if (any(atlasPosition < chunkWorldOffset) ||
        any(atlasPosition >= uint3(width, height, depth))) return sample;

    uint atlasData = chunk_atlas[atlasPosition];
    uint moistureLevel = min((atlasData & 0x30u) >> 4u, 3u);
    // Retain growth semantics even for non-soil supports. Only material wetness
    // validates soil type; growth overrides never replace this raw moisture.
    sample.growth_factor = floraMoistureGrowthFactor(targetSpecies, moistureLevel);
    sample.moisture_level = floraSupportMoistureLevel(atlasData,
        all(localBase < GRASS_GROWTH_POTENTIAL_CHUNK_DIM));
    return sample;
}

public float floraEnvironmentGrowthFactor(uint3 localBase,
                                           uint3 chunkWorldOffset,
                                           uint targetSpecies)
{
    return floraSupportSample(localBase, chunkWorldOffset, targetSpecies).growth_factor;
}
