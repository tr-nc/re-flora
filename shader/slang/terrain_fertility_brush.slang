import terrain_soil;
import voxel_data;
import voxel_types;

static const int3 SOIL_NEIGHBOURS[6] =
{
    int3(1, 0, 0), int3(-1, 0, 0), int3(0, 1, 0),
    int3(0, -1, 0), int3(0, 0, 1), int3(0, 0, -1)
};

[[vk::push_constant]]
ConstantBuffer<TerrainSoilBrushPushConstants> pc;

[[vk::binding(1, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

bool isSurfaceFertilityVoxel(int3 position, uint voxelType)
{
    if (!terrainSoilIsVoxel(voxelType))
    {
        return false;
    }

    for (int i = 0; i < 6; ++i)
    {
        if (terrainSoilLoadVoxelType(
            chunk_atlas, position + SOIL_NEIGHBOURS[i]) == VOXEL_TYPE_EMPTY)
        {
            return true;
        }
    }
    return false;
}

bool isFertilizerGranulePeak(int3 position, uint seed)
{
    float centerHash = terrainSoilHash01(uint3(position), seed);
    for (int i = 0; i < 6; ++i)
    {
        int3 neighbour = position + SOIL_NEIGHBOURS[i];
        if (any(neighbour < int3(0)))
        {
            continue;
        }
        if (terrainSoilHash01(uint3(neighbour), seed) >= centerHash)
        {
            return false;
        }
    }
    return true;
}

[shader("compute")]
[numthreads(8, 8, 8)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint3 local = dispatchThreadId;
    uint3 dimension = pc.dim.xyz;
    if (any(local >= dimension))
    {
        return;
    }

    uint3 atlasPosition = pc.offset.xyz + local;
    uint ditherSeed = pc.offset.w;
    float3 startVoxel = pc.start_radius.xyz;
    float radiusVoxel = max(pc.start_radius.w, 0.0);
    float3 endVoxel = pc.end_amount.xyz;
    float amount = clamp(pc.end_amount.w, 0.0, 1.0);
    if (radiusVoxel <= 0.0 || amount <= 0.0)
    {
        return;
    }

    float3 voxelCenter = float3(atlasPosition) + float3(0.5);
    float distanceFromStroke = terrainSoilDistanceToSegment(
        voxelCenter, startVoxel, endVoxel);
    if (distanceFromStroke > radiusVoxel)
    {
        return;
    }

    uint oldData = chunk_atlas[atlasPosition];
    uint voxelType = voxelTypeFromAtlasData(oldData);
    if (!isSurfaceFertilityVoxel(int3(atlasPosition), voxelType))
    {
        return;
    }

    float falloff = clamp(
        1.0 - distanceFromStroke / max(radiusVoxel, 0.0001),
        0.0,
        1.0);
    if (!isFertilizerGranulePeak(
        int3(atlasPosition), ditherSeed ^ 0xB5297A4Du))
    {
        return;
    }

    static const float EDGE_GRANULE_COVERAGE = 0.06;
    static const float CENTER_GRANULE_COVERAGE = 0.92;
    float granuleProbability = clamp(
        amount * lerp(
            EDGE_GRANULE_COVERAGE,
            CENTER_GRANULE_COVERAGE,
            falloff * falloff),
        0.0,
        1.0);
    if (terrainSoilHash01(
        atlasPosition, ditherSeed ^ 0x68BC21EBu) >= granuleProbability)
    {
        return;
    }

    uint oldFertility = voxelFertilityFromAtlasData(oldData);
    uint newFertility = min(
        VOXEL_FERTILITY_MAX, oldFertility + 2u);
    if (newFertility == oldFertility)
    {
        return;
    }

    chunk_atlas[atlasPosition] = packVoxelAtlasFertilityPreserveState(
        oldData, newFertility);
}
