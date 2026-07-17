module terrain_smooth;

import voxel_types;

public struct U_TerrainSmoothInfo
{
    public uint3 offset;
    public uint _pad0;
    public uint3 dim;
    public uint _pad1;
    public float2 center_xz_vox;
    public float brush_radius_vox;
    public float kernel_radius_vox;
    public float strength;
    public float max_delta_vox;
    public float deadband_vox;
    public float _pad2;
};

public struct B_TerrainSmoothColumns
{
    public float4 data[];
};

public struct B_TerrainSmoothResult
{
    public uint changed_voxel_count;
};

public bool terrainSmoothIsTerrainVoxel(uint voxelType)
{
    return voxelType == VOXEL_TYPE_DIRT ||
           voxelType == VOXEL_TYPE_SAND ||
           voxelType == VOXEL_TYPE_ROCK;
}

public uint terrainSmoothColumnIndex(U_TerrainSmoothInfo info, uint2 coordinate)
{
    return coordinate.x * info.dim.z + coordinate.y;
}
