module grass_growth_shape;

import flora_types;
import flora_noise;
import hash;

public static const uint TALL_GRASS_HEIGHT_VOXELS = 8u;
public static const uint SHORT_GRASS_HEIGHT_VOXELS = 4u;
public static const uint TALL_GRASS_MIN_HEIGHT_VOXELS = 3u;
public static const uint SHORT_GRASS_MIN_HEIGHT_VOXELS = 2u;

float sampleStandardNormal(uint seed)
{
    uint stateA = wellonsHash(seed ^ 0xA511E9B3u);
    uint stateB = wellonsHash(seed ^ 0x63D83595u);
    return (constructFloat01(stateA) + constructFloat01(stateB) - 1.0) * 2.4494898;
}

public uint sampleGrassHeight(uint instanceType, uint seed)
{
    bool isShortGrass = instanceType == FLORA_SPECIES_SHORT_GRASS;
    float meanHeight = isShortGrass ? 3.0 : 5.0;
    float standardDeviation = isShortGrass ? 0.6 : 1.0;
    float minimumHeight = float(isShortGrass
        ? SHORT_GRASS_MIN_HEIGHT_VOXELS
        : TALL_GRASS_MIN_HEIGHT_VOXELS);
    float maximumHeight = float(isShortGrass
        ? SHORT_GRASS_HEIGHT_VOXELS
        : TALL_GRASS_HEIGHT_VOXELS);
    return uint(round(clamp(meanHeight + sampleStandardNormal(seed) * standardDeviation,
                            minimumHeight, maximumHeight)));
}


// Geometry and palette share the current discrete visible height.
public float grassGrownHeight(uint heightVoxels, float growthFactor)
{
    return floor(float(heightVoxels) * growthFactor + 0.001);
}

public float grassColorGradient(int layer, float visibleHeight)
{
    // A single visible layer is also the tip. Empty stems are trimmed by geometry.
    if (visibleHeight <= 1.0) return 1.0;
    return clamp(float(layer) / (visibleHeight - 1.0), 0.0, 1.0);
}
