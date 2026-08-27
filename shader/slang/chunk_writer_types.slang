module chunk_writer_types;

public static const uint EDIT_STATS_VOXEL_TYPE_COUNT = 9u;
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

public struct BvhNode
{
    public float3 aabb_min;
    public float3 aabb_max;
    public uint offset;
};

public struct RoundCone
{
    public float3 center_a;
    public float radius_a;
    public float3 center_b;
    public float radius_b;
};

public struct Cuboid
{
    public float3 center;
    public float _pad0;
    public float3 half_size;
    public float _pad1;
    public float4 inverse_rotation;
};

public struct Sphere
{
    public float3 center;
    public float radius;
};

public struct Torus
{
    public float3 center;
    public float major_radius;
    public float4 inverse_rotation;
    public float tube_radius;
};

public struct U_ChunkModifyInfo
{
    public uint3 offset;
    public uint3 dim;
    public uint fill_voxel_type;
    public uint target_voxel_type;
    public uint primitive_kind;
    public uint surface_only;
    public uint max_write_count;
    public uint clear_fill_voxel_state;
    public uint4 max_removed_counts_0_3;
    public uint4 max_removed_counts_4_7;
    public uint4 max_removed_counts_8_11;
};

public struct B_BvhNodes
{
    public BvhNode data[];
};

public struct B_RoundCones
{
    public RoundCone data[];
};

public struct B_Cuboids
{
    public Cuboid data[];
};

public struct B_Spheres
{
    public Sphere data[];
};

public struct B_Toruses
{
    public Torus data[];
};

public struct U_ModelVoxelizeInfo
{
    public uint3 offset;
    public uint triangle_count;
    public uint3 dim;
    public uint fill_voxel_type;
    public float3 position_vox;
    public float surface_thickness_vox;
};

public struct B_ModelTriangles
{
    public float4 data[];
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

public struct HeightmapEntry
{
    public float surface_height;
    public float base_shape;
};

public struct B_Heightmap
{
    public HeightmapEntry data[];
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
