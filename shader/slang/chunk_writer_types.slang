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
