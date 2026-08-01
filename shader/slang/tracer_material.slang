module tracer_material;

import color;
import ddgi_types;
import tracer_types;
import voxel_data;
import voxel_types;

float3 voxelColorByTypeSrgb(U_VoxelColors colors, uint voxelType)
{
    if (voxelType == VOXEL_TYPE_DIRT) return colors.dirt_color;
    if (voxelType == VOXEL_TYPE_SAND) return colors.sand_color;
    if (voxelType == VOXEL_TYPE_CHERRY_WOOD) return colors.cherry_wood_color;
    if (voxelType == VOXEL_TYPE_OAK_WOOD) return colors.oak_wood_color;
    if (voxelType == VOXEL_TYPE_ROCK) return colors.rock_color;
    return float3(0.0);
}

float voxelHashVariance(uint voxelType)
{
    if (voxelType == VOXEL_TYPE_DIRT || voxelType == VOXEL_TYPE_SAND) return 1.0;
    if (voxelType == VOXEL_TYPE_ROCK) return 0.6;
    return 0.0;
}

float3 voxelColorByComponentsAndHash(float3 baseColor,
                                     float hashColorVariance,
                                     uint voxelType, uint hashId)
{
    float3 hsv = rgbToHsv(baseColor);
    float variant = float(hashId & 0x3u) - 1.5;
    float amount = hashColorVariance * voxelHashVariance(voxelType);
    hsv.x = frac(hsv.x + variant * 0.01 * amount);
    hsv.y = clamp(hsv.y + variant * 0.03 * amount, 0.0, 1.0);
    hsv.z = clamp(hsv.z + variant * 0.025 * amount, 0.0, 1.0);
    return srgbToLinear(hsvToRgb(hsv));
}

public float3 voxelColorByTypeAndHash(U_VoxelColors colors, uint voxelType, uint hashId)
{
    return voxelColorByComponentsAndHash(
        voxelColorByTypeSrgb(colors, voxelType), colors.hash_color_variance,
        voxelType, hashId);
}

float3 ddgiVoxelColorByTypeSrgb(U_DdgiRadianceVoxelPalette colors,
                                uint voxelType)
{
    if (voxelType == VOXEL_TYPE_DIRT) return colors.dirt_color;
    if (voxelType == VOXEL_TYPE_SAND) return colors.sand_color;
    if (voxelType == VOXEL_TYPE_CHERRY_WOOD) return colors.cherry_wood_color;
    if (voxelType == VOXEL_TYPE_OAK_WOOD) return colors.oak_wood_color;
    if (voxelType == VOXEL_TYPE_ROCK) return colors.rock_color;
    return float3(0.0);
}

public float3 ddgiVoxelColorByTypeAndHash(
    U_DdgiRadianceVoxelPalette colors, uint voxelType, uint hashId)
{
    return voxelColorByComponentsAndHash(
        ddgiVoxelColorByTypeSrgb(colors, voxelType),
        colors.hash_color_variance, voxelType, hashId);
}

public float3 applyTerrainMoisture(float3 baseColor, uint voxelType, uint moistureLevel)
{
    bool canShowMoisture = voxelType == VOXEL_TYPE_DIRT || voxelType == VOXEL_TYPE_SAND;
    if (!canShowMoisture || moistureLevel == 0u) return baseColor;

    uint level = min(moistureLevel, VOXEL_MOISTURE_MAX);
    float3 wetColor;
    if (level == 1u)
    {
        wetColor = lerp(baseColor * 0.76, float3(0.060, 0.050, 0.038), 0.16);
    }
    else if (level == 2u)
    {
        wetColor = lerp(baseColor * 0.52, float3(0.030, 0.038, 0.032), 0.28);
    }
    else
    {
        wetColor = lerp(baseColor * 0.32, float3(0.010, 0.024, 0.022), 0.42);
    }
    return clamp(wetColor, float3(0.0), float3(1.0));
}

float fertilizerGranuleNoise(float3 centerPosition, uint seed)
{
    int3 position = int3(floor(centerPosition * 256.0));
    uint hash = seed ^ 0x9E3779B9u;
    hash ^= uint(position.x) * 0x85EBCA6Bu;
    hash ^= uint(position.y) * 0xC2B2AE35u;
    hash ^= uint(position.z) * 0x27D4EB2Fu;
    hash ^= hash >> 16u;
    hash *= 0x7FEB352Du;
    hash ^= hash >> 15u;
    hash *= 0x846CA68Bu;
    hash ^= hash >> 16u;
    return float(hash & 0x00FFFFFFu) * (1.0 / 16777216.0);
}

public float3 applyTerrainFertility(float3 baseColor, uint voxelType, uint fertilityLevel,
                                    float3 centerPosition)
{
    bool canShowFertility = voxelType == VOXEL_TYPE_DIRT || voxelType == VOXEL_TYPE_SAND;
    if (!canShowFertility) return baseColor;

    uint level = min(fertilityLevel, VOXEL_FERTILITY_MAX);
    if (level == 0u)
    {
        float3 gray = float3(dot(baseColor, float3(0.299, 0.587, 0.114)));
        return clamp(lerp(baseColor, gray, 0.28) * float3(1.05, 0.98, 0.86),
                     float3(0.0), float3(1.0));
    }
    if (level == 1u) return baseColor;

    const float3 lightCompostBrown = float3(0.275, 0.171, 0.091);
    const float3 richCompostBrown = float3(0.332, 0.195, 0.098);
    const float3 palePellet = float3(0.456, 0.332, 0.205);
    const float3 darkPellet = float3(0.172, 0.100, 0.050);

    float3 compostColor = level == 2u ? lightCompostBrown : richCompostBrown;
    float tint = level == 2u ? 0.10 : 0.18;
    float3 materialWarmth = level == 2u ? float3(1.02, 1.00, 0.96)
                                         : float3(1.04, 1.01, 0.94);
    float3 fertileColor = lerp(baseColor * materialWarmth, compostColor, tint);

    float fineNoise = fertilizerGranuleNoise(centerPosition, 0x6D2B79F5u);
    float coarseNoise = fertilizerGranuleNoise(centerPosition * 0.31 + 11.0, 0xA511E9B3u);
    float threshold = level == 2u ? 0.80 : 0.62;
    float granule = smoothstep(threshold, 0.98, lerp(fineNoise, coarseNoise, 0.28));
    float3 granuleColor = lerp(darkPellet, palePellet,
                               fertilizerGranuleNoise(centerPosition, 0x68BC21EBu));
    float opacity = level == 2u ? 0.34 : 0.50;
    return clamp(lerp(fertileColor, granuleColor, granule * opacity),
                 float3(0.0), float3(1.0));
}
