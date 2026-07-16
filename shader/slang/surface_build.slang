module surface_build;

public struct U_MakeSurfaceInfo
{
    public uint3 atlas_read_offset;
    public uint3 atlas_read_dim;
    public uint is_crossing_boundary;
};

public struct B_MakeSurfaceResult
{
    public uint active_voxel_len;
    public uint active_brick_len;
    public uint solid_workgroup_len;
};

public struct B_SolidWorkgroupFlags
{
    public uint data[];
};

public struct B_SurfaceSolidWorkgroupIndices
{
    public uint data[];
};

public struct B_SurfaceSolidWorkgroupDispatchIndirect
{
    public uint dispatch_x;
    public uint dispatch_y;
    public uint dispatch_z;
};
