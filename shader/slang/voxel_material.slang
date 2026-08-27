module voxel_material;

import voxel_types;

public static const uint VOXEL_SURFACE_CLASS_EMPTY = 0u;
public static const uint VOXEL_SURFACE_CLASS_OPAQUE = 1u;
public static const uint VOXEL_SURFACE_CLASS_DIELECTRIC = 2u;

public static const uint VOXEL_MATERIAL_FLAG_COLLISION_SOLID = 1u;
public static const uint VOXEL_MATERIAL_FLAG_WATER_SOLID = 2u;
public static const uint VOXEL_MATERIAL_FLAG_TERRAIN_SUPPORT = 4u;
public static const uint VOXEL_MATERIAL_FLAG_PROBE_RELOCATION_SOLID = 8u;
public static const uint VOXEL_MATERIAL_FLAG_BLOCKS_DDGI_VISIBILITY = 16u;
public static const uint VOXEL_MATERIAL_FLAG_SOIL_STATE_ALLOWED = 32u;

public static const uint VOXEL_DIRECT_SHADOW_OPAQUE = 0u;
public static const uint VOXEL_DIRECT_SHADOW_SKIP = 1u;
public static const uint VOXEL_LOCAL_SHADOW_OPAQUE = 0u;
public static const uint VOXEL_LOCAL_SHADOW_OPTICAL_TRANSMITTANCE = 1u;

public static const uint GLASS_EXPERIMENT_VOXEL_TYPE = 3u;
public static const uint STANDARD_SOIL_VOXEL_TYPE_MASK = 12u;
public static const uint STANDARD_SOLID_MATERIAL_FLAGS = 31u;
public static const uint GLASS_EXPERIMENT_MATERIAL_FLAGS = 15u;

public static const float GLASS_EXPERIMENT_IOR = 1.5;
public static const float3 GLASS_EXPERIMENT_ATTENUATION_COLOR = float3(0.82, 0.94, 0.98);
public static const float GLASS_EXPERIMENT_ATTENUATION_DISTANCE_WORLD = 0.25;

public struct VoxelMaterial
{
    public uint surface_class;
    public uint flags;
    public uint direct_shadow;
    public uint local_shadow;
    public float ior;
    public float3 attenuation_color;
    public float attenuation_distance_world;
};

public VoxelMaterial voxelMaterial(uint voxelType, bool glassExperimentEnabled)
{
    VoxelMaterial result;
    result.surface_class = VOXEL_SURFACE_CLASS_EMPTY;
    result.flags = 0u;
    result.direct_shadow = VOXEL_DIRECT_SHADOW_SKIP;
    result.local_shadow = VOXEL_LOCAL_SHADOW_OPTICAL_TRANSMITTANCE;
    result.ior = 1.0;
    result.attenuation_color = float3(1.0);
    result.attenuation_distance_world = 1.0;
    if (voxelType == VOXEL_TYPE_EMPTY) return result;

    if (glassExperimentEnabled && voxelType == GLASS_EXPERIMENT_VOXEL_TYPE)
    {
        result.surface_class = VOXEL_SURFACE_CLASS_DIELECTRIC;
        result.flags = GLASS_EXPERIMENT_MATERIAL_FLAGS;
        result.direct_shadow = VOXEL_DIRECT_SHADOW_SKIP;
        result.local_shadow = VOXEL_LOCAL_SHADOW_OPTICAL_TRANSMITTANCE;
        result.ior = GLASS_EXPERIMENT_IOR;
        result.attenuation_color = GLASS_EXPERIMENT_ATTENUATION_COLOR;
        result.attenuation_distance_world = GLASS_EXPERIMENT_ATTENUATION_DISTANCE_WORLD;
        return result;
    }

    result.surface_class = VOXEL_SURFACE_CLASS_OPAQUE;
    result.flags = STANDARD_SOLID_MATERIAL_FLAGS;
    if (((1u << voxelType) & STANDARD_SOIL_VOXEL_TYPE_MASK) != 0u)
    {
        result.flags |= VOXEL_MATERIAL_FLAG_SOIL_STATE_ALLOWED;
    }
    result.direct_shadow = VOXEL_DIRECT_SHADOW_OPAQUE;
    result.local_shadow = VOXEL_LOCAL_SHADOW_OPAQUE;
    return result;
}
