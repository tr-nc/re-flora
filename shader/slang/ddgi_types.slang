module ddgi_types;

public static const uint DDGI_PROBE_STATE_INACTIVE = 0u;
public static const uint DDGI_PROBE_STATE_VALID = 1u;
public static const uint DDGI_PROBE_STATE_RELOCATION_FAILED = 2u;

public struct DdgiProbeMetadata
{
    public float4 nominal_position_and_min_clearance;
    public float4 actual_position_and_clearance;
    public uint4 state_and_reserved;
};

public struct B_DdgiProbeMetadata
{
    public DdgiProbeMetadata data[];
};

public struct B_DdgiRayData
{
    public float4 data[];
};

public struct B_DdgiTraceStats
{
    public uint data[];
};

public struct B_DdgiRelocationStats
{
    public uint data[];
};

public struct B_DdgiAtlasReduction
{
    public uint data[];
};

// Latched once at the start of a full DDGI build. Builder shaders must not read the live
// per-frame sun uniform, because one atlas may span many frames.
public struct U_DdgiRadianceSun
{
    public float3 direction;
    public float terrain_ray_origin_offset_world;
    public float3 color;
    public float luminance;
};

// Stable material inputs transported by DDGI. Dynamic moisture and edit-preview presentation are
// deliberately absent from this snapshot.
public struct U_DdgiRadianceVoxelPalette
{
    public float3 dirt_color;
    public float3 sand_color;
    public float3 cherry_wood_color;
    public float3 oak_wood_color;
    public float3 rock_color;
    public float3 emissive_color;
    public float emissive_radiance;
};

// Builder-owned query geometry. This is intentionally separate from U_ShadingInfo, whose
// contents track active consumers and can change while a full-volume build is in flight.
public struct U_DdgiTransportQueryInfo
{
    public uint3 grid_dimensions;
    public float visibility_bias_world;
    public float3 world_to_grid_scale;
    public uint source_ready;
    public uint irradiance_tile_columns;
    public uint visibility_tile_columns;
    public uint geometry_revision;
    public uint glass_experiment_enabled;
    public uint glass_material_revision;
    public uint glass_padding_0;
    public uint glass_padding_1;
    public uint glass_padding_2;
};

// Exact authoritative voxel occupancy published for DDGI query visibility. X is packed into
// 32-bit words while Y/Z retain voxel resolution.
public static const uint DDGI_VOXEL_VISIBILITY_BLOCK_SIZE = 8u;

public struct U_DdgiVoxelVisibilityInfo
{
    public uint3 voxel_dimensions;
    public uint geometry_revision;
    public uint3 packed_word_dimensions;
    public uint ready;
    public float3 world_to_voxel_scale;
    public uint max_steps;
    public uint4 update_word_offset;
    public uint4 update_word_dimensions;
    public uint4 update_block_offset;
    public uint4 update_block_dimensions;
    public uint glass_experiment_enabled;
    public uint glass_material_revision;
    public uint2 padding;
};
