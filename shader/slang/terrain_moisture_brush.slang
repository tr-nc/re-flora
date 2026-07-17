import terrain_soil;
import voxel_data;
import voxel_types;

static const int WATERING_SURFACE_SEARCH_STEPS = 5;
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

float nearSurfaceAbsorptionFactor(int3 position, uint voxelType)
{
    if (!terrainSoilIsVoxel(voxelType))
    {
        return 0.0;
    }

    int bestDepth = WATERING_SURFACE_SEARCH_STEPS;
    for (int i = 0; i < 6; ++i)
    {
        for (int stepIndex = 1;
             stepIndex <= WATERING_SURFACE_SEARCH_STEPS;
             ++stepIndex)
        {
            if (terrainSoilLoadVoxelType(
                chunk_atlas,
                position + SOIL_NEIGHBOURS[i] * stepIndex) == VOXEL_TYPE_EMPTY)
            {
                bestDepth = min(bestDepth, stepIndex - 1);
                break;
            }
        }
    }
    if (bestDepth >= WATERING_SURFACE_SEARCH_STEPS)
    {
        return 0.0;
    }

    float depth = float(bestDepth) / float(WATERING_SURFACE_SEARCH_STEPS);
    float factor = clamp(1.0 - depth, 0.0, 1.0);
    return factor * factor;
}

[shader("compute")]
[numthreads(8, 8, 8)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint3 local = dispatchThreadId;
    uint3 dimension = pc.dim.xyz;
    bool directionalPair = pc.dim.w != 0u;
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
    float distanceFromStroke;
    if (directionalPair)
    {
        float2 axis = endVoxel.xz - startVoxel.xz;
        float rangeVoxel = length(axis);
        float2 offset = voxelCenter.xz - startVoxel.xz;
        distanceFromStroke = length(offset);
        if (rangeVoxel <= 1.0e-5 || distanceFromStroke > rangeVoxel)
        {
            return;
        }
        if (distanceFromStroke > 1.0e-5)
        {
            float alignment = abs(dot(
                offset / distanceFromStroke, axis / rangeVoxel));
            static const float COS_45_DEGREES = 0.70710678;
            if (alignment < COS_45_DEGREES)
            {
                return;
            }
        }
        radiusVoxel = rangeVoxel;
    }
    else
    {
        distanceFromStroke = terrainSoilDistanceToSegment(
            voxelCenter, startVoxel, endVoxel);
        if (distanceFromStroke > radiusVoxel)
        {
            return;
        }
    }

    uint oldData = chunk_atlas[atlasPosition];
    uint voxelType = voxelTypeFromAtlasData(oldData);
    float absorptionFactor = nearSurfaceAbsorptionFactor(
        int3(atlasPosition), voxelType);
    if (absorptionFactor <= 0.0)
    {
        return;
    }

    float falloff = clamp(
        1.0 - distanceFromStroke / max(radiusVoxel, 0.0001),
        0.0,
        1.0);
    float fineNoise = terrainSoilHash01(atlasPosition, ditherSeed);
    float coarseNoise = terrainSoilHash01(
        atlasPosition / 4u, ditherSeed ^ 0xA511E9B3u);
    float noisyFalloff = clamp(
        falloff + (lerp(fineNoise, coarseNoise, 0.35) - 0.5) * 0.28,
        0.0,
        1.0);
    float continuousAdd = amount * noisyFalloff * absorptionFactor *
                          float(VOXEL_MOISTURE_MAX);
    uint addMoisture = uint(floor(continuousAdd));
    if (terrainSoilHash01(atlasPosition, ditherSeed ^ 0x68BC21EBu) <
        frac(continuousAdd))
    {
        addMoisture += 1u;
    }
    if (addMoisture == 0u)
    {
        return;
    }

    uint oldMoisture = voxelMoistureFromAtlasData(oldData);
    uint newMoisture = min(
        VOXEL_MOISTURE_MAX, oldMoisture + addMoisture);
    if (newMoisture == oldMoisture)
    {
        return;
    }

    chunk_atlas[atlasPosition] = packVoxelAtlasMoisturePreserveState(
        oldData, newMoisture);
}
