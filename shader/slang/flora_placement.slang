module flora_placement;

import flora_types;
import gradient_noise;

public static const float FLORA_GRASS_DECREASED_SPARSE_THRESHOLD = 0.45;

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
