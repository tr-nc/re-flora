module contree_build;

public struct U_ContreeBuildInfo
{
    public uint dim;
    public uint max_level;
    public uint node_write_offset;
    public uint leaf_write_offset;
};

public struct B_ContreeBuildState
{
    public uint prev_dim;
    public uint curr_dim;
    public uint level;
};

public struct B_ContreeBuildResult
{
    public uint node_len;
    public uint leaf_len;
};

public struct B_MakeSurfaceResult
{
    public uint active_voxel_len;
    public uint active_brick_len;
};

public struct B_NodeOffsetForLevels
{
    public uint data[];
};

public struct B_CounterForLevels
{
    public uint data[];
};
