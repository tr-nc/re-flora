module chunk_writer_types;

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
