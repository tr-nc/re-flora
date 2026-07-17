module terrain_smooth_mbo;

import chunk_writer_types;
import voxel_data;
import voxel_types;

public static const float TERRAIN_SMOOTH_MBO_DEAD_SCORE_SCALE = 1.0 / 256.0;

public uint terrainSmoothMboLocalIndex(U_TerrainSmoothMboInfo info, uint3 local)
{
    return (local.z * info.dim.y + local.y) * info.dim.x + local.x;
}

public bool terrainSmoothMboInLocalBounds(U_TerrainSmoothMboInfo info,
                                          int3 local)
{
    return all(local >= int3(0)) && all(local < int3(info.dim.xyz));
}

public int3 terrainSmoothMboAtlasPosition(U_TerrainSmoothMboInfo info,
                                          uint3 local)
{
    return int3(info.offset.xyz + local);
}

public uint terrainSmoothMboLoadVoxelType(
    U_TerrainSmoothMboInfo info, readonly RWTexture3D<uint> chunkAtlas,
    uint3 local)
{
    return voxelTypeFromAtlasData(
        chunkAtlas[terrainSmoothMboAtlasPosition(info, local)]);
}

public bool terrainSmoothMboIsTerrainVoxel(uint voxelType)
{
    return voxelType == VOXEL_TYPE_DIRT ||
           voxelType == VOXEL_TYPE_SAND ||
           voxelType == VOXEL_TYPE_ROCK;
}

public bool terrainSmoothMboIsMutableVoxel(uint voxelType)
{
    return voxelType == VOXEL_TYPE_EMPTY ||
           terrainSmoothMboIsTerrainVoxel(voxelType);
}

public float terrainSmoothMboOriginalDensity(uint voxelType)
{
    return terrainSmoothMboIsTerrainVoxel(voxelType) ? 1.0 : 0.0;
}

public float3 terrainSmoothMboVoxelCenter(U_TerrainSmoothMboInfo info,
                                          uint3 local)
{
    return float3(info.offset.xyz + local) + float3(0.5);
}

public float terrainSmoothMboBrushFalloff(U_TerrainSmoothMboInfo info,
                                           uint3 local)
{
    float distance = length(
        terrainSmoothMboVoxelCenter(info, local) - info.center_radius.xyz);
    if (distance > info.center_radius.w)
    {
        return 0.0;
    }
    float value = clamp(
        distance / max(info.center_radius.w, 0.0001), 0.0, 1.0);
    return 1.0 - smoothstep(0.0, 1.0, value);
}

public bool terrainSmoothMboIsCandidateVoxel(U_TerrainSmoothMboInfo info,
                                              uint3 local, uint voxelType)
{
    return terrainSmoothMboIsMutableVoxel(voxelType) &&
           terrainSmoothMboBrushFalloff(info, local) > 0.0;
}

public uint terrainSmoothMboScoreBin(U_TerrainSmoothMboInfo info, float score)
{
    float clampedScore = clamp(score, 0.0, 1.0);
    uint binCount = max(info.dim.w, 1u);
    return min(
        uint(floor(clampedScore * float(binCount - 1u) + 0.5)),
        binCount - 1u);
}

public uint terrainSmoothMboHash(uint3 position)
{
    uint value = position.x * 0x9E3779B9u ^
                 position.y * 0x85EBCA6Bu ^
                 position.z * 0xC2B2AE35u;
    value ^= value >> 16u;
    value *= 0x7FEB352Du;
    value ^= value >> 15u;
    value *= 0x846CA68Bu;
    return value ^ (value >> 16u);
}

public uint terrainSmoothMboChooseFillVoxelType(
    U_TerrainSmoothMboInfo info, readonly RWTexture3D<uint> chunkAtlas,
    uint3 local)
{
    uint dirtCount = 0u;
    uint sandCount = 0u;
    uint rockCount = 0u;
    static const int3 offsets[6] =
    {
        int3(1, 0, 0), int3(-1, 0, 0), int3(0, 1, 0),
        int3(0, -1, 0), int3(0, 0, 1), int3(0, 0, -1)
    };
    for (int i = 0; i < 6; ++i)
    {
        int3 next = int3(local) + offsets[i];
        if (!terrainSmoothMboInLocalBounds(info, next))
        {
            continue;
        }
        uint voxelType = terrainSmoothMboLoadVoxelType(
            info, chunkAtlas, uint3(next));
        if (voxelType == VOXEL_TYPE_DIRT)
        {
            ++dirtCount;
        }
        else if (voxelType == VOXEL_TYPE_SAND)
        {
            ++sandCount;
        }
        else if (voxelType == VOXEL_TYPE_ROCK)
        {
            ++rockCount;
        }
    }

    if (sandCount > dirtCount && sandCount >= rockCount)
    {
        return VOXEL_TYPE_SAND;
    }
    if (rockCount > dirtCount && rockCount > sandCount)
    {
        return VOXEL_TYPE_ROCK;
    }
    return VOXEL_TYPE_DIRT;
}
