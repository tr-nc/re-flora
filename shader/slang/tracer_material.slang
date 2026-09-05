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
    if (voxelType == VOXEL_TYPE_STUCCO) return VOXEL_STUCCO_COLOR_SRGB;
    if (voxelType == VOXEL_TYPE_CHERRY_WOOD) return colors.cherry_wood_color;
    if (voxelType == VOXEL_TYPE_OAK_WOOD) return colors.oak_wood_color;
    if (voxelType == VOXEL_TYPE_ROCK) return colors.rock_color;
    if (voxelType == VOXEL_TYPE_EMISSIVE) return colors.emissive_color;
    return float3(0.0);
}

public float3 voxelColorByType(U_VoxelColors colors, uint voxelType)
{
    return srgbToLinear(voxelColorByTypeSrgb(colors, voxelType));
}

public float3 voxelSurfaceEmission(U_VoxelColors colors, uint voxelType)
{
    return voxelType == VOXEL_TYPE_EMISSIVE
        ? srgbToLinear(colors.emissive_color) * colors.emissive_radiance
        : float3(0.0);
}

float3 ddgiVoxelColorByTypeSrgb(U_DdgiRadianceVoxelPalette colors,
                                uint voxelType)
{
    if (voxelType == VOXEL_TYPE_DIRT) return colors.dirt_color;
    if (voxelType == VOXEL_TYPE_SAND) return colors.sand_color;
    if (voxelType == VOXEL_TYPE_STUCCO) return VOXEL_STUCCO_COLOR_SRGB;
    if (voxelType == VOXEL_TYPE_CHERRY_WOOD) return colors.cherry_wood_color;
    if (voxelType == VOXEL_TYPE_OAK_WOOD) return colors.oak_wood_color;
    if (voxelType == VOXEL_TYPE_ROCK) return colors.rock_color;
    if (voxelType == VOXEL_TYPE_EMISSIVE) return colors.emissive_color;
    return float3(0.0);
}

public float3 ddgiVoxelSurfaceEmission(U_DdgiRadianceVoxelPalette colors,
                                       uint voxelType)
{
    return voxelType == VOXEL_TYPE_EMISSIVE
        ? srgbToLinear(colors.emissive_color) * colors.emissive_radiance
        : float3(0.0);
}

public float3 ddgiVoxelColorByType(
    U_DdgiRadianceVoxelPalette colors, uint voxelType)
{
    return srgbToLinear(ddgiVoxelColorByTypeSrgb(colors, voxelType));
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
