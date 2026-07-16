module solid_workgroups;

public static const uint SOLID_WORKGROUP_SIZE = 8u;

public uint3 solidWorkgroupGridDimension(uint3 atlasDimension)
{
    return (atlasDimension + uint3(SOLID_WORKGROUP_SIZE - 1u)) / SOLID_WORKGROUP_SIZE;
}

public uint solidWorkgroupIndexFromCoordinate(uint3 workgroupCoordinate, uint3 gridDimension)
{
    return workgroupCoordinate.x + workgroupCoordinate.z * gridDimension.x +
           workgroupCoordinate.y * gridDimension.x * gridDimension.z;
}

public uint solidWorkgroupIndexFromVoxel(uint3 voxelPosition, uint3 atlasDimension)
{
    uint3 gridDimension = solidWorkgroupGridDimension(atlasDimension);
    uint3 workgroupCoordinate = voxelPosition / SOLID_WORKGROUP_SIZE;
    return solidWorkgroupIndexFromCoordinate(workgroupCoordinate, gridDimension);
}
