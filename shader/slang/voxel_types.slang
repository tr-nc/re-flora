module voxel_types;

public static const uint VOXEL_TYPE_EMPTY = 0u;
public static const uint VOXEL_TYPE_DIRT = 2u;
public static const uint VOXEL_TYPE_SAND = 3u;
public static const uint VOXEL_TYPE_STUCCO = 4u;
public static const uint VOXEL_TYPE_CHERRY_WOOD = 5u;
public static const uint VOXEL_TYPE_OAK_WOOD = 6u;
public static const uint VOXEL_TYPE_ROCK = 7u;
public static const uint VOXEL_TYPE_EMISSIVE = 8u;

// Warm, pale-yellow exterior plaster typical of early twentieth-century stucco.
public static const float3 VOXEL_STUCCO_COLOR_SRGB = float3(0.82, 0.74, 0.50);

// Distinct authored cottage materials; existing world palettes remain unchanged.
public static const uint VOXEL_TYPE_LIMESTONE = 9u;
public static const uint VOXEL_TYPE_IVY = 10u;
public static const uint VOXEL_TYPE_PETAL = 11u;
public static const float3 VOXEL_LIMESTONE_COLOR_SRGB = float3(0.88, 0.87, 0.80);
public static const float3 VOXEL_IVY_COLOR_SRGB = float3(0.22, 0.46, 0.17);
public static const float3 VOXEL_PETAL_COLOR_SRGB = float3(0.92, 0.54, 0.65);
