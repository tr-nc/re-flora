module terrain_soil;

import voxel_data;
import voxel_types;

public struct TerrainMoistureSpreadPushConstants
{
    public uint4 offset;
    public uint4 dim;
    public float4 spread_params;
    public uint4 phase_params;
};

public struct TerrainSoilBrushPushConstants
{
    public uint4 offset;
    public uint4 dim;
    public float4 start_radius;
    public float4 end_amount;
};

public bool terrainSoilIsVoxel(uint voxelType)
{
    return voxelType == VOXEL_TYPE_DIRT || voxelType == VOXEL_TYPE_SAND;
}

public uint terrainSoilLoadVoxelType(RWTexture3D<uint> chunkAtlas, int3 position)
{
    uint width;
    uint height;
    uint depth;
    chunkAtlas.GetDimensions(width, height, depth);
    if (any(position < int3(0)) ||
        any(position >= int3(width, height, depth)))
    {
        return VOXEL_TYPE_EMPTY;
    }
    return voxelTypeFromAtlasData(chunkAtlas[position]);
}

public float terrainSoilDistanceToSegment(float3 point,
                                          float3 start,
                                          float3 end)
{
    float3 direction = end - start;
    float lengthSquared = dot(direction, direction);
    if (lengthSquared <= 1.0e-5)
    {
        return length(point - start);
    }
    float t = clamp(
        dot(point - start, direction) / lengthSquared, 0.0, 1.0);
    return length(point - (start + direction * t));
}

public float terrainSoilHash01(uint3 position, uint seed)
{
    uint value = seed ^ 0x9E3779B9u;
    value ^= position.x * 0x85EBCA6Bu;
    value ^= position.y * 0xC2B2AE35u;
    value ^= position.z * 0x27D4EB2Fu;
    value ^= value >> 16u;
    value *= 0x7FEB352Du;
    value ^= value >> 15u;
    value *= 0x846CA68Bu;
    value ^= value >> 16u;
    return float(value & 0x00FFFFFFu) * (1.0 / 16777216.0);
}

public uint3 terrainSoilAxisOffset(uint axis)
{
    if (axis == 0u)
    {
        return uint3(1u, 0u, 0u);
    }
    if (axis == 1u)
    {
        return uint3(0u, 1u, 0u);
    }
    return uint3(0u, 0u, 1u);
}

public uint terrainSoilAxisCoordinate(uint3 position, uint axis)
{
    if (axis == 0u)
    {
        return position.x;
    }
    if (axis == 1u)
    {
        return position.y;
    }
    return position.z;
}
