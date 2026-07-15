module voxel_data;

public static const uint VOXEL_TYPE_MASK = 0x0Fu;
public static const uint VOXEL_NORMAL_BITS_MASK = 0xFFFFu;
public static const uint VOXEL_HASH_MASK = 0x3u;
public static const uint VOXEL_HASH_SHIFT = 30u;

public uint voxelTypeFromAtlasData(uint voxelData)
{
    return voxelData & VOXEL_TYPE_MASK;
}

public uint packVoxelSurfaceData(uint voxelType, uint normalBits, uint hashId)
{
    uint voxelData = 0u;
    voxelData |= voxelType & VOXEL_TYPE_MASK;
    voxelData |= (normalBits & VOXEL_NORMAL_BITS_MASK) << 8u;
    voxelData |= (hashId & VOXEL_HASH_MASK) << VOXEL_HASH_SHIFT;
    return voxelData;
}
