module voxel_data;

import packer;

public static const uint VOXEL_TYPE_MASK = 0x0Fu;
public static const uint VOXEL_ATLAS_STATE_MASK = 0xF0u;
public static const uint VOXEL_MOISTURE_MASK = 0x30u;
public static const uint VOXEL_MOISTURE_SHIFT = 4u;
public static const uint VOXEL_MOISTURE_MAX = 3u;
public static const uint VOXEL_NORMAL_BITS_MASK = 0xFFFFu;

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

public uint packVoxelAtlasData(uint voxelType, uint moisture)
{
    return (voxelType & VOXEL_TYPE_MASK) |
           ((moisture & VOXEL_MOISTURE_MAX) << VOXEL_MOISTURE_SHIFT);
}

public uint packVoxelAtlasTypeClearState(uint voxelType)
{
    return packVoxelAtlasData(voxelType, 0u);
}

public uint packVoxelAtlasMoisturePreserveState(uint oldVoxelData,
                                                 uint moisture)
{
    return (oldVoxelData & ~VOXEL_MOISTURE_MASK) |
           ((moisture & VOXEL_MOISTURE_MAX) << VOXEL_MOISTURE_SHIFT);
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

// Surface records have twelve spare bits (4..7, 24..31); atlas state is a
// different format. Store six exposed faces and six-bit normal uncertainty.
// Zero metadata preserves the legacy reliable-normal response.
public float voxelNormalConfidenceFromData(uint data)
{
    uint uncertainty = ((data >> 4u) & 15u) | ((data >> 30u) << 4u);
    return 1.0 - float(uncertainty) / 63.0;
}

public uint voxelExposedFacesFromData(uint data)
{
    return (data >> 24u) & 63u;
}

public uint packVoxelSurfaceData(uint voxelType, uint normalBits)
{
    uint voxelData = 0u;
    voxelData |= voxelType & VOXEL_TYPE_MASK;
    voxelData |= (normalBits & VOXEL_NORMAL_BITS_MASK) << 8u;
    return voxelData;
}

public uint packVoxelSurfaceLightingData(uint voxelType, uint normalBits,
    float confidence, uint exposedFaces)
{
    uint uncertainty = uint(round((1.0 - saturate(confidence)) * 63.0));
    return packVoxelSurfaceData(voxelType, normalBits)
        | ((uncertainty & 15u) << 4u) | ((uncertainty >> 4u) << 30u)
        | ((exposedFaces & 63u) << 24u);
}
