module flora_placement;

import flora_types;
import gradient_noise;

public static const float FLORA_LAVENDER_SPARSE_THRESHOLD = 0.10;
public static const float FLORA_EMBER_BLOOM_SPARSE_THRESHOLD = 0.01;
public static const float FLORA_GRASS_DECREASED_SPARSE_THRESHOLD = 0.45;
public static const uint FLORA_PAINT_MAX_PLANTS_PER_CELL_PER_DAB = 16u;

public float floraLegacyHash(float2 point)
{
    float3 value = frac(float3(point.x, point.y, point.x) * 0.1031);
    value += dot(value, value.yzx + 33.33);
    return frac((value.x + value.y) * value.z);
}

public void calculateFloraDensityPlacement(int3 worldPosition,
                                           out bool allowAnyFlora,
                                           out bool allowTallGrass,
                                           out bool allowShortGrass)
{
    float anyFloraRandom = floraLegacyHash(
        float2(worldPosition.x, worldPosition.z));
    float tallGrassRandom = floraLegacyHash(
        float2(worldPosition.x + 17.0, worldPosition.z - 29.0));
    float shortGrassRandom = floraLegacyHash(
        float2(worldPosition.x - 53.0, worldPosition.z + 91.0));

    float densityNoise = seededClassicGradientNoise2D(
        float2(float(worldPosition.x), float(worldPosition.z)) * 5.0, 42u);
    float placementThreshold = (densityNoise + 1.0) * 0.5;
    placementThreshold = clamp(0.08 + placementThreshold * 0.22, 0.0, 1.0);

    float grassPlacementThreshold = clamp(placementThreshold * 2.25, 0.0, 1.0);
    float tallGrassThreshold = clamp(grassPlacementThreshold * 0.30, 0.0, 1.0);
    float shortGrassThreshold = clamp(grassPlacementThreshold * 0.8, 0.0, 1.0);

    allowAnyFlora = anyFloraRandom < placementThreshold;
    allowTallGrass = tallGrassRandom < tallGrassThreshold;
    allowShortGrass = shortGrassRandom < shortGrassThreshold;
}

public bool floraSpeciesUsesSparsePaintDensity(uint speciesIndex)
{
    return speciesIndex == FLORA_SPECIES_LAVENDER ||
           speciesIndex == FLORA_SPECIES_EMBER_BLOOM;
}

public float floraSparseSpeciesRandom(int3 worldPosition)
{
    return floraLegacyHash(
        float2(worldPosition.x + 42.0, worldPosition.z - 15.0));
}

public bool floraSparseSpeciesMaskAllows(uint speciesIndex, int3 worldPosition)
{
    float speciesRandom = floraSparseSpeciesRandom(worldPosition);
    if (speciesIndex == FLORA_SPECIES_LAVENDER)
    {
        return speciesRandom < FLORA_LAVENDER_SPARSE_THRESHOLD;
    }
    if (speciesIndex == FLORA_SPECIES_EMBER_BLOOM)
    {
        return speciesRandom < FLORA_EMBER_BLOOM_SPARSE_THRESHOLD;
    }
    return true;
}

uint floraUnsignedGcd(uint a, uint b)
{
    for (uint index = 0u; index < 32u && b != 0u; ++index)
    {
        uint remainder = a % b;
        a = b;
        b = remainder;
    }
    return a;
}

uint floraCoprimeStep(uint rawStep, uint modulus)
{
    if (modulus <= 1u)
    {
        return 0u;
    }

    uint step = rawStep % modulus;
    if (step == 0u)
    {
        step = 1u;
    }

    for (uint attempt = 0u; attempt < 64u; ++attempt)
    {
        if (floraUnsignedGcd(step, modulus) == 1u)
        {
            return step;
        }
        step += 1u;
        if (step >= modulus)
        {
            step = 1u;
        }
    }
    return 1u;
}

uint2 floraLayeredSparsePaintAnchor(uint cellSize, int2 cell, uint layer,
                                     float2 seedOffset)
{
    uint safeCellSize = max(cellSize, 1u);
    uint cellArea = safeCellSize * safeCellSize;
    if (cellArea <= 1u)
    {
        return uint2(0u);
    }

    float2 cellSeed = float2(float(cell.x), float(cell.y)) + seedOffset;
    uint baseX = min(
        uint(floor(floraLegacyHash(cellSeed) * float(safeCellSize))),
        safeCellSize - 1u);
    uint baseZ = min(
        uint(floor(floraLegacyHash(cellSeed + float2(37.0, 71.0)) *
                   float(safeCellSize))),
        safeCellSize - 1u);
    uint baseSlot = baseZ * safeCellSize + baseX;
    uint rawStep = 1u + min(
        uint(floor(floraLegacyHash(cellSeed + float2(113.0, 197.0)) *
                   float(cellArea - 1u))),
        cellArea - 2u);
    uint step = floraCoprimeStep(rawStep, cellArea);
    uint slot = (baseSlot + (layer % cellArea) * step) % cellArea;
    return uint2(slot % safeCellSize, slot / safeCellSize);
}

bool floraLayeredSparsePaintMaskAllows(uint4 paintConfiguration,
                                        float2 seedOffset,
                                        int3 worldPosition)
{
    uint cellSize = max(paintConfiguration.y, 1u);
    uint cellArea = cellSize * cellSize;
    uint maximumPlantsPerCell = min(paintConfiguration.z, cellArea);
    uint releasedPerDab = min(
        min(paintConfiguration.w, maximumPlantsPerCell),
        FLORA_PAINT_MAX_PLANTS_PER_CELL_PER_DAB);
    if (maximumPlantsPerCell == 0u || releasedPerDab == 0u)
    {
        return false;
    }

    int integerCellSize = int(cellSize);
    int2 worldXz = int2(worldPosition.x, worldPosition.z);
    int2 cell = int2(floor(float2(float(worldXz.x), float(worldXz.y)) /
                           float(integerCellSize)));
    int2 local = worldXz - cell * integerCellSize;
    uint2 unsignedLocal = uint2(local);

    uint firstLayer =
        ((paintConfiguration.x % maximumPlantsPerCell) * releasedPerDab) %
        maximumPlantsPerCell;
    for (uint index = 0u; index < FLORA_PAINT_MAX_PLANTS_PER_CELL_PER_DAB;
         ++index)
    {
        if (index >= releasedPerDab)
        {
            break;
        }
        uint layer = (firstLayer + index) % maximumPlantsPerCell;
        uint2 anchor = floraLayeredSparsePaintAnchor(
            cellSize, cell, layer, seedOffset);
        if (all(unsignedLocal == anchor))
        {
            return true;
        }
    }
    return false;
}

public bool floraSparsePaintSelectionAllows(uint paintSelection,
                                             int3 worldPosition,
                                             uint4 paintConfiguration)
{
    if (paintSelection == FLORA_SPECIES_LAVENDER)
    {
        return floraLayeredSparsePaintMaskAllows(
            paintConfiguration, float2(42.0, -15.0), worldPosition);
    }
    if (paintSelection == FLORA_SPECIES_EMBER_BLOOM)
    {
        return floraLayeredSparsePaintMaskAllows(
            paintConfiguration, float2(-53.0, 91.0), worldPosition);
    }
    return true;
}
