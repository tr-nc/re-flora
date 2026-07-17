module voxel_data;

import packer;

public static const uint VOXEL_TYPE_MASK = 0x0Fu;
public static const uint VOXEL_ATLAS_STATE_MASK = 0xF0u;
public static const uint VOXEL_MOISTURE_MASK = 0x30u;
public static const uint VOXEL_MOISTURE_SHIFT = 4u;
public static const uint VOXEL_MOISTURE_MAX = 3u;
public static const uint VOXEL_FERTILITY_MASK = 0xC0u;
public static const uint VOXEL_FERTILITY_SHIFT = 6u;
public static const uint VOXEL_FERTILITY_MAX = 3u;
public static const uint VOXEL_FERTILITY_DEFAULT = 1u;
public static const uint VOXEL_NORMAL_BITS_MASK = 0xFFFFu;
public static const uint VOXEL_HASH_MASK = 0x3u;
public static const uint VOXEL_HASH_SHIFT = 30u;

public uint voxelTypeFromAtlasData(uint voxelData)
{
    return voxelData & VOXEL_TYPE_MASK;
}

public uint voxelTypeFromData(uint voxelData)
{
    return voxelData & VOXEL_TYPE_MASK;
}

public uint voxelMoistureFromAtlasData(uint voxelData)
{
    return (voxelData & VOXEL_MOISTURE_MASK) >> VOXEL_MOISTURE_SHIFT;
}

public uint voxelFertilityFromAtlasData(uint voxelData)
{
    return (voxelData & VOXEL_FERTILITY_MASK) >> VOXEL_FERTILITY_SHIFT;
}

public uint defaultFertilityForVoxelType(uint voxelType)
{
    return voxelType == 0u ? 0u : VOXEL_FERTILITY_DEFAULT;
}

public uint packVoxelAtlasData(uint voxelType, uint moisture, uint fertility)
{
    return (voxelType & VOXEL_TYPE_MASK) |
           ((moisture & VOXEL_MOISTURE_MAX) << VOXEL_MOISTURE_SHIFT) |
           ((fertility & VOXEL_FERTILITY_MAX) << VOXEL_FERTILITY_SHIFT);
}

public uint packVoxelAtlasTypeClearState(uint voxelType)
{
    return packVoxelAtlasData(
        voxelType, 0u, defaultFertilityForVoxelType(voxelType));
}

public uint packVoxelAtlasMoisturePreserveState(uint oldVoxelData,
                                                 uint moisture)
{
    return (oldVoxelData & ~VOXEL_MOISTURE_MASK) |
           ((moisture & VOXEL_MOISTURE_MAX) << VOXEL_MOISTURE_SHIFT);
}

public uint packVoxelAtlasFertilityPreserveState(uint oldVoxelData,
                                                  uint fertility)
{
    return (oldVoxelData & ~VOXEL_FERTILITY_MASK) |
           ((fertility & VOXEL_FERTILITY_MAX) << VOXEL_FERTILITY_SHIFT);
}

public uint packVoxelAtlasTypePreserveState(uint oldVoxelData, uint voxelType)
{
    return (oldVoxelData & VOXEL_ATLAS_STATE_MASK) |
           (voxelType & VOXEL_TYPE_MASK);
}

public uint voxelNormalBitsFromData(uint voxelData)
{
    return (voxelData >> 8u) & VOXEL_NORMAL_BITS_MASK;
}

public float3 voxelSurfaceNormalFromData(uint voxelData)
{
    return unpackNormalOct16(voxelNormalBitsFromData(voxelData));
}

public uint voxelHashFromData(uint voxelData)
{
    return (voxelData >> VOXEL_HASH_SHIFT) & VOXEL_HASH_MASK;
}

public uint packVoxelSurfaceData(uint voxelType, uint normalBits, uint hashId)
{
    uint voxelData = 0u;
    voxelData |= voxelType & VOXEL_TYPE_MASK;
    voxelData |= (normalBits & VOXEL_NORMAL_BITS_MASK) << 8u;
    voxelData |= (hashId & VOXEL_HASH_MASK) << VOXEL_HASH_SHIFT;
    return voxelData;
}
