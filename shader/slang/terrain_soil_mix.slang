import terrain_soil;
import voxel_data;

[[vk::push_constant]]
ConstantBuffer<TerrainSoilBrushPushConstants> pc;

[[vk::binding(1, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

bool isInsideTillBrush(uint3 atlasPosition,
                       float3 startVoxel,
                       float3 endVoxel,
                       float radiusVoxel,
                       out float falloff)
{
    float3 voxelCenter = float3(atlasPosition) + float3(0.5);
    float distanceFromStroke = terrainSoilDistanceToSegment(
        voxelCenter, startVoxel, endVoxel);
    if (distanceFromStroke > radiusVoxel)
    {
        falloff = 0.0;
        return false;
    }
    falloff = clamp(
        1.0 - distanceFromStroke / max(radiusVoxel, 0.0001),
        0.0,
        1.0);
    return true;
}

bool transferSoilLevel(inout uint aLevel,
                       inout uint bLevel,
                       uint maxLevel,
                       float mixStrength,
                       float pairFalloff,
                       uint3 pairKey,
                       uint seed)
{
    if (aLevel == bLevel)
    {
        return false;
    }

    uint difference = aLevel > bLevel
        ? aLevel - bLevel
        : bLevel - aLevel;
    float transferProbability = clamp(
        mixStrength * pairFalloff *
            (float(difference) / float(maxLevel)),
        0.0,
        1.0);
    if (terrainSoilHash01(pairKey, seed) >= transferProbability)
    {
        return false;
    }

    if (aLevel > bLevel)
    {
        aLevel -= 1u;
        bLevel += 1u;
    }
    else
    {
        aLevel += 1u;
        bLevel -= 1u;
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
    float mixStrength = clamp(pc.end_amount.w, 0.0, 1.0);
    if (radiusVoxel <= 0.0 || mixStrength <= 0.0)
    {
        return;
    }

    uint phase = pc.dim.w;
    uint axis = min(phase & 0x3u, 2u);
    uint pairParity = (phase >> 2u) & 1u;
    if ((terrainSoilAxisCoordinate(atlasPosition, axis) & 1u) != pairParity)
    {
        return;
    }

    uint3 neighbourPosition =
        atlasPosition + terrainSoilAxisOffset(axis);
    uint atlasWidth;
    uint atlasHeight;
    uint atlasDepth;
    chunk_atlas.GetDimensions(atlasWidth, atlasHeight, atlasDepth);
    if (any(neighbourPosition >= uint3(atlasWidth, atlasHeight, atlasDepth)))
    {
        return;
    }

    float aFalloff;
    float bFalloff;
    if (!isInsideTillBrush(
            atlasPosition,
            startVoxel,
            endVoxel,
            radiusVoxel,
            aFalloff) ||
        !isInsideTillBrush(
            neighbourPosition,
            startVoxel,
            endVoxel,
            radiusVoxel,
            bFalloff))
    {
        return;
    }

    uint aData = chunk_atlas[atlasPosition];
    uint bData = chunk_atlas[neighbourPosition];
    uint aType = voxelTypeFromAtlasData(aData);
    uint bType = voxelTypeFromAtlasData(bData);
    if (!terrainSoilIsVoxel(aType) || !terrainSoilIsVoxel(bType))
    {
        return;
    }

    uint aMoisture = voxelMoistureFromAtlasData(aData);
    uint bMoisture = voxelMoistureFromAtlasData(bData);

    float pairFalloff = min(aFalloff, bFalloff);
    uint3 pairKey = atlasPosition ^ (neighbourPosition * 1664525u);
    bool moistureChanged = transferSoilLevel(
        aMoisture,
        bMoisture,
        VOXEL_MOISTURE_MAX,
        mixStrength,
        pairFalloff,
        pairKey,
        ditherSeed ^ phase ^ 0xA24BAED5u);
    if (!moistureChanged)
    {
        return;
    }

    chunk_atlas[atlasPosition] = packVoxelAtlasMoisturePreserveState(
        aData, aMoisture);
    chunk_atlas[neighbourPosition] = packVoxelAtlasMoisturePreserveState(
        bData, bMoisture);
}
