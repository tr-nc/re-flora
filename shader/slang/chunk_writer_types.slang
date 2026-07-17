module chunk_writer_types;

public static const uint EDIT_STATS_VOXEL_TYPE_COUNT = 8u;
public static const uint EDIT_REMOVAL_SAMPLE_COUNT = 50u;

public struct B_EditStats
{
    public uint removed_counts[EDIT_STATS_VOXEL_TYPE_COUNT];
    public uint added_counts[EDIT_STATS_VOXEL_TYPE_COUNT];
};

public struct B_EditRemovalCandidates
{
    public float4 positions[];
};

public struct B_EditRemovalSample
{
    public uint sample_count;
    public float4 positions[EDIT_REMOVAL_SAMPLE_COUNT];
};

public struct U_VoxelPropertySampleInfo
{
    public uint4 atlas_offset_property_id;
    public uint4 atlas_dim_target_mask;
    public float4 center_radius;
    public uint4 options;
};

public struct B_VoxelPropertySampleResult
{
    public uint4 stats;
    public uint4 extra;
};

public struct U_ChunkSolidSampleInfo
{
    public uint3 atlas_offset;
    public uint3 atlas_dim;
    public uint3 sample_dim;
};

public struct B_ChunkSolidSamples
{
    public uint solid[];
};

public struct U_RegionInfo
{
    public uint3 offset;
    public uint3 dim;
};

public struct B_RegionIndirect
{
    public uint dispatch_x;
    public uint dispatch_y;
    public uint dispatch_z;
};

public struct U_TerrainSmoothMboInfo
{
    public uint4 offset;
    public uint4 dim;
    public float4 center_radius;
    public float4 params;
    public uint4 threshold;
};

public struct B_TerrainSmoothMboDensityA
{
    public float data[];
};

public struct B_TerrainSmoothMboDensityB
{
    public float data[];
};

public struct B_TerrainSmoothMboScores
{
    public float data[];
};

public struct B_TerrainSmoothMboHistogram
{
    public uint bins[];
};

public struct B_TerrainSmoothMboResult
{
    public uint4 counts;
    public uint4 counts_extra;
    public uint4 changed_min;
    public uint4 changed_max;
};
