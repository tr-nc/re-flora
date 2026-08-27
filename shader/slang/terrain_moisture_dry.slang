import terrain_soil;
import terrain_ray_origin;
import tracer_shadowing;
import tracer_types;
import voxel_data;
import voxel_types;

struct B_ManualContreeLeafData
{
    uint data[];
};

struct B_ManualSurfaceLeafCoords
{
    uint data[];
};

struct B_ManualSurfaceLeafChunkInfo
{
    uint4 data[];
};

static const int3 SOIL_NEIGHBOURS[6] =
{
    int3(1, 0, 0), int3(-1, 0, 0), int3(0, 1, 0),
    int3(0, -1, 0), int3(0, 0, 1), int3(0, 0, -1)
};

[[vk::push_constant]]
ConstantBuffer<TerrainMoistureDryPushConstants> pc;

[[vk::binding(0, 0)]]
ConstantBuffer<U_GuiInput> gui_input;

[[vk::binding(1, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

[[vk::binding(2, 0)]]
ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;

[[vk::binding(3, 0)]]
Sampler2D shadow_map_tex_for_vsm_ping;

[[vk::binding(4, 0)]]
readonly GLSLShaderStorageBuffer<B_ManualContreeLeafData, Std430DataLayout>
    contree_leaf_data;

[[vk::binding(5, 0)]]
readonly GLSLShaderStorageBuffer<B_ManualSurfaceLeafCoords, Std430DataLayout>
    surface_leaf_coords;

[[vk::binding(6, 0)]]
readonly GLSLShaderStorageBuffer<B_ManualSurfaceLeafChunkInfo, Std430DataLayout>
    surface_leaf_chunk_info;

[[vk::binding(7, 0)]]
Sampler2D leaf_shadow_opacity_blended_tex;

[[vk::binding(8, 0)]]
Sampler2D leaf_shadow_mask_tex;

[[vk::binding(9, 0)]]
Sampler2D cloud_shadow_tex;

uint loadAtlasMoisture(int3 position)
{
    if (!all(position >= int3(0)))
    {
        return 0u;
    }
    uint width;
    uint height;
    uint depth;
    chunk_atlas.GetDimensions(width, height, depth);
    if (!all(position < int3(width, height, depth)))
    {
        return 0u;
    }
    uint atlasData = chunk_atlas[position];
    if (!terrainSoilIsVoxel(voxelTypeFromAtlasData(atlasData)))
    {
        return 0u;
    }
    return voxelMoistureFromAtlasData(atlasData);
}

bool hasWetterMoistureNeighbour(int3 position, uint moisture)
{
    for (int i = 0; i < 6; ++i)
    {
        if (loadAtlasMoisture(position + SOIL_NEIGHBOURS[i]) > moisture)
        {
            return true;
        }
    }
    return false;
}

bool sunRayIsOccluded(float3 originVoxel, float3 sunDirection)
{
    static const int MAX_STEPS = 768;
    float3 samplePosition = originVoxel + sunDirection * 0.25;
    uint width;
    uint height;
    uint depth;
    chunk_atlas.GetDimensions(width, height, depth);
    int3 atlasDimension = int3(width, height, depth);
    for (int stepIndex = 0; stepIndex < MAX_STEPS; ++stepIndex)
    {
        samplePosition += sunDirection;
        int3 sampleVoxel = int3(floor(samplePosition));
        if (any(sampleVoxel < int3(0)) || any(sampleVoxel >= atlasDimension))
        {
            return false;
        }
        if (terrainSoilLoadVoxelType(
            chunk_atlas, sampleVoxel) != VOXEL_TYPE_EMPTY)
        {
            return true;
        }
    }
    return false;
}

bool shadowCameraContains(float3 worldPosition)
{
    float4 lightSpace = mul(
        shadow_camera_info.view_proj_mat,
        float4(worldPosition, 1.0));
    if (abs(lightSpace.w) <= 1.0e-6)
    {
        return false;
    }
    float2 uv = (lightSpace.xyz / lightSpace.w).xy * 0.5 + 0.5;
    return !any(uv < float2(0.0)) && !any(uv > float2(1.0));
}

float directShadowExposure(float3 worldPosition,
                           uint sourceMask,
                           uint availableMask)
{
    float4 receiverPosition = float4(worldPosition, 1.0);
    DirectSunShadowReceiver receiver = makeDirectSunShadowReceiver(
        receiverPosition,
        receiverPosition,
        receiverPosition,
        int3(0),
        sourceMask,
        DIRECT_TERRAIN_SHADOW_VSM,
        availableMask,
        DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE);
    return directSunShadowTransmittanceVsmLeafCloud(
        receiver,
        gui_input,
        shadow_camera_info.view_proj_mat,
        shadow_map_tex_for_vsm_ping,
        leaf_shadow_opacity_blended_tex,
        leaf_shadow_mask_tex,
        cloud_shadow_tex);
}

int3 unpackSurfaceLeafLocalCoordinate(uint packedCoordinate)
{
    return int3(
        int(packedCoordinate & 0x3ffu),
        int((packedCoordinate >> 10u) & 0x3ffu),
        int((packedCoordinate >> 20u) & 0x3ffu));
}

float3 surfaceShadowSamplePosition(float3 voxelCenterWorld,
                                   float3 normal,
                                   float voxelsPerWorldUnit)
{
    return terrainRayOriginAlongNormal(
        voxelCenterWorld, normal, 1.0 / voxelsPerWorldUnit,
        gui_input.terrain_ray_origin_offset_world);
}

float surfaceSunExposure(float3 sampleWorldPosition,
                         float3 normal,
                         float3 sunDirection,
                         float voxelsPerWorldUnit,
                         uint sourceMask,
                         uint availableMask)
{
    if (sunDirection.y <= 0.0 || dot(normal, sunDirection) <= 0.05)
    {
        return 0.0;
    }

    uint directSourceMask = sourceMask;
    float terrainVisibility = 1.0;
    bool wantsTerrain =
        (sourceMask & DIRECT_SUN_SHADOW_SOURCE_TERRAIN) != 0u;
    bool terrainVsmAvailable = wantsTerrain &&
        (availableMask & DIRECT_SUN_SHADOW_SOURCE_TERRAIN) != 0u &&
        shadowCameraContains(sampleWorldPosition);
    if (wantsTerrain && !terrainVsmAvailable)
    {
        directSourceMask &= ~DIRECT_SUN_SHADOW_SOURCE_TERRAIN;
        terrainVisibility = !sunRayIsOccluded(
            sampleWorldPosition * voxelsPerWorldUnit, sunDirection)
            ? 1.0
            : 0.0;
    }

    return terrainVisibility * directShadowExposure(
        sampleWorldPosition, directSourceMask, availableMask);
}

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint surfaceLeafLocalIndex = dispatchThreadId.x;
    uint surfaceLeafCount = pc.dim.x;
    uint chunkInfoIndex = pc.dim.y;
    if (surfaceLeafLocalIndex >= surfaceLeafCount)
    {
        return;
    }

    uint4 chunkInfo = surface_leaf_chunk_info.data[chunkInfoIndex];
    uint leafOffset = chunkInfo.x;
    uint leafCountFromInfo = chunkInfo.y;
    bool chunkSurfaceReady = chunkInfo.z != 0u;
    if (!chunkSurfaceReady || surfaceLeafLocalIndex >= leafCountFromInfo)
    {
        return;
    }

    uint globalLeafIndex = leafOffset + surfaceLeafLocalIndex;
    int3 localCoordinate = unpackSurfaceLeafLocalCoordinate(
        surface_leaf_coords.data[globalLeafIndex]);
    int3 atlasCoordinate = int3(pc.offset.xyz) + localCoordinate;
    uint atlasWidth;
    uint atlasHeight;
    uint atlasDepth;
    chunk_atlas.GetDimensions(atlasWidth, atlasHeight, atlasDepth);
    if (any(atlasCoordinate < int3(0)) ||
        any(atlasCoordinate >= int3(atlasWidth, atlasHeight, atlasDepth)))
    {
        return;
    }

    float dryProbability = clamp(pc.dry_params.x, 0.0, 1.0);
    float residualProbabilityMultiplier = max(pc.dry_params.y, 1.0);
    float voxelsPerWorldUnit = max(pc.dry_params.z, 1.0);
    uint shadowSourceMask = pc.shadow_params.x;
    uint shadowAvailableMask = pc.shadow_params.y;
    float sunDirectionLengthSquared = dot(
        pc.sun_dir_params.xyz, pc.sun_dir_params.xyz);
    float3 sunDirection = sunDirectionLengthSquared > 0.0
        ? pc.sun_dir_params.xyz * rsqrt(sunDirectionLengthSquared)
        : float3(0.0);
    float sunlitProbabilityMultiplier = max(pc.sun_dir_params.w, 1.0);
    if (dryProbability <= 0.0)
    {
        return;
    }

    uint oldData = chunk_atlas[atlasCoordinate];
    uint oldMoisture = voxelMoistureFromAtlasData(oldData);
    if (oldMoisture == 0u ||
        !terrainSoilIsVoxel(voxelTypeFromAtlasData(oldData)))
    {
        return;
    }

    uint surfaceData = contree_leaf_data.data[globalLeafIndex];
    float3 normal = voxelSurfaceNormalFromData(surfaceData);
    float3 voxelCenterWorld =
        (float3(atlasCoordinate) + float3(0.5)) / voxelsPerWorldUnit;
    float3 sampleWorldPosition = surfaceShadowSamplePosition(
        voxelCenterWorld, normal, voxelsPerWorldUnit);
    float sunExposure = surfaceSunExposure(
        sampleWorldPosition,
        normal,
        sunDirection,
        voxelsPerWorldUnit,
        shadowSourceMask,
        shadowAvailableMask);
    dryProbability *= lerp(
        1.0, sunlitProbabilityMultiplier, sunExposure);
    dryProbability = clamp(dryProbability, 0.0, 1.0);

    uint3 atlasPosition = uint3(atlasCoordinate);
    uint seed = pc.offset.w;
    float dryRoll = terrainSoilHash01(atlasPosition, seed);
    bool shouldDry = dryRoll < dryProbability;
    if (!shouldDry && oldMoisture == 1u)
    {
        float residualDryProbability = clamp(
            dryProbability * residualProbabilityMultiplier, 0.0, 1.0);
        if (dryRoll < residualDryProbability)
        {
            shouldDry = !hasWetterMoistureNeighbour(
                atlasCoordinate, oldMoisture);
        }
    }
    if (!shouldDry)
    {
        return;
    }

    chunk_atlas[atlasCoordinate] = packVoxelAtlasMoisturePreserveState(
        oldData, oldMoisture - 1u);
}
