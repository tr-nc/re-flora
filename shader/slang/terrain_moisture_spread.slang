import terrain_soil;
import voxel_data;

[[vk::push_constant]]
ConstantBuffer<TerrainMoistureSpreadPushConstants> pc;

[[vk::binding(1, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

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
    uint seed = pc.offset.w;
    float spreadProbability = clamp(pc.spread_params.x, 0.0, 1.0);
    float mobilityExponent = max(pc.spread_params.y, 1.0);
    float downwardMultiplier = max(pc.spread_params.z, 0.0);
    float upwardMultiplier = max(pc.spread_params.w, 0.0);
    if (spreadProbability <= 0.0)
    {
        return;
    }

    uint axis = min(pc.phase_params.x, 2u);
    uint pairParity = pc.phase_params.y & 1u;
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
    if (aMoisture == bMoisture)
    {
        return;
    }

    uint difference = aMoisture > bMoisture
        ? aMoisture - bMoisture
        : bMoisture - aMoisture;
    if (difference < 2u)
    {
        return;
    }

    uint sourceMoisture = max(aMoisture, bMoisture);
    float gradientFactor = float(difference) / float(VOXEL_MOISTURE_MAX);
    float sourceSaturation =
        float(sourceMoisture) / float(VOXEL_MOISTURE_MAX);
    float unsaturatedMobility = pow(sourceSaturation, mobilityExponent);

    float directionalBias = 1.0;
    if (axis == 1u)
    {
        float freeWater = clamp(
            (float(sourceMoisture) - 1.0) /
                max(float(VOXEL_MOISTURE_MAX - 1u), 1.0),
            0.0,
            1.0);
        bool sourceIsAbove = bMoisture > aMoisture;
        float saturatedDirectionalBias = sourceIsAbove
            ? downwardMultiplier
            : upwardMultiplier;
        directionalBias = lerp(
            1.0, saturatedDirectionalBias, freeWater);
    }

    float transferProbability = clamp(
        spreadProbability * gradientFactor * unsaturatedMobility *
            directionalBias,
        0.0,
        1.0);
    if (terrainSoilHash01(
        atlasPosition ^ (neighbourPosition * 1664525u), seed) >=
        transferProbability)
    {
        return;
    }

    if (aMoisture > bMoisture)
    {
        aMoisture -= 1u;
        bMoisture += 1u;
    }
    else
    {
        aMoisture += 1u;
        bMoisture -= 1u;
    }

    chunk_atlas[atlasPosition] = packVoxelAtlasMoisturePreserveState(
        aData, aMoisture);
    chunk_atlas[neighbourPosition] = packVoxelAtlasMoisturePreserveState(
        bData, bMoisture);
}
