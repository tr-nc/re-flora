module surface_build;

public struct U_MakeSurfaceInfo
{
    public uint3 atlas_read_offset;
    public uint3 atlas_read_dim;
    public uint is_crossing_boundary;
};

public struct B_SurfaceActiveBrickFlags
{
    public uint data[];
};

public struct B_SurfaceActiveBrickIndices
{
    public uint data[];
};
