import color;
import flora_motion;
import flora_noise;
import flora_shadow;
import flora_types;
import hash;
import terrain_edit_preview;
import tracer_types;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2, 0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4, 0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(5, 0)]] Sampler2D shadow_map_tex_for_vsm_ping;
[[vk::binding(6, 0)]] ConstantBuffer<U_FloraGrowthInfo> flora_growth_info;
[[vk::binding(7, 0)]] ConstantBuffer<U_WindVolumeInfo> wind_volume_info;
[[vk::binding(8, 0)]] Sampler3D wind_volume_tex;
[[vk::binding(9, 0)]] Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(10, 0)]] Sampler2D leaf_shadow_mask_tex;
[[vk::binding(11, 0)]] Sampler2D cloud_shadow_tex;
[[vk::binding(12, 0)]] ConstantBuffer<U_TerrainEditPreview> terrain_edit_preview;

struct B_FloraVoxelTableDescs
{
    uint4 descs[FLORA_SPECIES_APPLE + 1u];
};

[[vk::binding(13, 0)]]
readonly GLSLShaderStorageBuffer<B_FloraVoxelTableDescs, Std430DataLayout>
    flora_voxel_table_descs;

struct B_FloraVoxelInfos
{
    uint2 entries[];
};

[[vk::binding(14, 0)]]
readonly GLSLShaderStorageBuffer<B_FloraVoxelInfos, Std430DataLayout>
    flora_voxel_infos;

[[vk::binding(15, 0)]]
[[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;

struct B_ManualFloraInstances
{
    Instance data[];
};

[[vk::binding(0, 1)]]
readonly GLSLShaderStorageBuffer<B_ManualFloraInstances, Std430DataLayout>
    manual_flora_instances;

struct B_GrassGrowthPotentialLevels
{
    uint words[];
};

[[vk::binding(1, 1)]]
readonly GLSLShaderStorageBuffer<B_GrassGrowthPotentialLevels, Std430DataLayout>
    manual_grass_growth_potential_levels;

[[vk::push_constant]]
ConstantBuffer<PushConstantFlora> pc;

static const float SCALING_FACTOR = 1.0 / 256.0;
static const uint TALL_GRASS_HEIGHT_VOXELS = 8u;
static const uint SHORT_GRASS_HEIGHT_VOXELS = 4u;
static const uint TALL_GRASS_MIN_HEIGHT_VOXELS = 3u;
static const uint SHORT_GRASS_MIN_HEIGHT_VOXELS = 2u;
static const uint FLORA_VOXEL_LOOKUP_EMPTY_KEY = 0xffffffffu;
static const uint FLORA_VOXEL_LOOKUP_MAX_PROBES = 64u;
static const uint FLORA_LOOKUP_BITS_PER_AXIS = 10u;
static const uint FLORA_LOOKUP_AXIS_MASK =
    (1u << FLORA_LOOKUP_BITS_PER_AXIS) - 1u;
static const int FLORA_LOOKUP_AXIS_BIAS =
    1 << (FLORA_LOOKUP_BITS_PER_AXIS - 1u);
static const uint GRASS_GROWTH_POTENTIAL_LEVEL_MAX = 15u;
static const uint3 GRASS_GROWTH_POTENTIAL_CHUNK_DIM = uint3(256u);

struct VertexInput
{
    [[vk::location(0)]] uint packed_data : TEXCOORD0;
    uint instance_index : SV_VulkanInstanceID;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float3 color : COLOR0;
};

void unpackVertexData(uint packedData, out int3 voxelLocalPosition,
                      out uint3 vertexOffsetInVoxel)
{
    const uint bitsPerPosition = 7u;
    const uint positionMask = (1u << bitsPerPosition) - 1u;
    uint positionX = packedData & positionMask;
    uint positionY = (packedData >> bitsPerPosition) & positionMask;
    uint positionZ = (packedData >> (bitsPerPosition * 2u)) & positionMask;
    voxelLocalPosition = int3(positionX, positionY, positionZ) -
                         (1 << (bitsPerPosition - 1u));

    const uint positionBits = bitsPerPosition * 3u;
    uint packedOffset = (packedData >> positionBits) & 0x7u;
    vertexOffsetInVoxel = uint3(packedOffset & 1u,
                                (packedOffset >> 1u) & 1u,
                                (packedOffset >> 2u) & 1u);
}

uint grassGrowthPotentialLevel(uint3 localBase)
{
    uint linearIndex = localBase.x + GRASS_GROWTH_POTENTIAL_CHUNK_DIM.x *
        (localBase.y + GRASS_GROWTH_POTENTIAL_CHUNK_DIM.y * localBase.z);
    uint word = manual_grass_growth_potential_levels.words[linearIndex >> 3u];
    return (word >> ((linearIndex & 7u) * 4u)) & 0x0fu;
}

float floraCompetitionGrowthFactor(uint3 localBase, uint targetSpecies)
{
    bool isGrass = targetSpecies == FLORA_SPECIES_TALL_GRASS ||
                   targetSpecies == FLORA_SPECIES_SHORT_GRASS;
    return isGrass
        ? float(grassGrowthPotentialLevel(localBase)) /
          float(GRASS_GROWTH_POTENTIAL_LEVEL_MAX)
        : 1.0;
}

float floraEnvironmentGrowthFactor(uint3 localBase, uint3 chunkWorldOffset,
                                   uint targetSpecies)
{
    if (targetSpecies >= FLORA_SPECIES_COUNT) return 1.0;
    uint3 atlasPosition = chunkWorldOffset + localBase;
    uint width;
    uint height;
    uint depth;
    chunk_atlas.GetDimensions(width, height, depth);
    if (any(atlasPosition >= uint3(width, height, depth))) return 1.0;

    uint atlasData = chunk_atlas[atlasPosition];
    uint moistureLevel = min((atlasData & 0x30u) >> 4u, 3u);
    return flora_growth_info.moisture_growth_factors[targetSpecies][moistureLevel];
}

uint floraLookupHash(uint key)
{
    uint value = key ^ 0x9E3779B9u;
    value ^= value >> 16u;
    value *= 0x7FEB352Du;
    value ^= value >> 15u;
    value *= 0x846CA68Bu;
    return value ^ (value >> 16u);
}

uint encodeFloraLookupPositionKey(int3 position)
{
    int3 shifted = position + int3(FLORA_LOOKUP_AXIS_BIAS);
    if (any(shifted < int3(0)) ||
        any(shifted > int3(int(FLORA_LOOKUP_AXIS_MASK))))
    {
        return FLORA_VOXEL_LOOKUP_EMPTY_KEY;
    }
    uint3 value = uint3(shifted);
    return value.x | (value.y << FLORA_LOOKUP_BITS_PER_AXIS) |
           (value.z << (FLORA_LOOKUP_BITS_PER_AXIS * 2u));
}

uint lookupFloraVoxelInfo(uint instanceType, int3 voxelLocalPosition)
{
    if (instanceType > FLORA_SPECIES_APPLE) return 0u;
    uint4 descriptor = flora_voxel_table_descs.descs[instanceType];
    if (instanceType == FLORA_SPECIES_TREE_LEAF)
    {
        float gradient = clamp(length(float3(voxelLocalPosition)) /
                               max(float(descriptor.w), 1.0), 0.0, 1.0);
        uint gradientU8 = uint(round(gradient * 255.0));
        return gradientU8 | (gradientU8 << 8u) | (gradientU8 << 16u);
    }

    uint offset = descriptor.x;
    uint capacity = descriptor.y;
    uint fallbackInfo = descriptor.z;
    if (capacity == 0u) return fallbackInfo;

    uint key = encodeFloraLookupPositionKey(voxelLocalPosition);
    if (key == FLORA_VOXEL_LOOKUP_EMPTY_KEY) return fallbackInfo;

    uint mask = capacity - 1u;
    uint startSlot = floraLookupHash(key) & mask;
    for (uint probe = 0u; probe < FLORA_VOXEL_LOOKUP_MAX_PROBES; ++probe)
    {
        uint slot = offset + ((startSlot + probe) & mask);
        uint2 entry = flora_voxel_infos.entries[slot];
        if (entry.x == key) return entry.y;
        if (entry.x == FLORA_VOXEL_LOOKUP_EMPTY_KEY) return fallbackInfo;
    }
    return fallbackInfo;
}

float floraVoxelLookupMaxLength(uint instanceType)
{
    if (instanceType > FLORA_SPECIES_APPLE) return 1.0;
    return max(float(flora_voxel_table_descs.descs[instanceType].w), 1.0);
}

float sampleStandardNormal(uint seed)
{
    uint stateA = wellonsHash(seed ^ 0xA511E9B3u);
    uint stateB = wellonsHash(seed ^ 0x63D83595u);
    return (constructFloat01(stateA) + constructFloat01(stateB) - 1.0) * 2.4494898;
}

uint sampleGrassHeight(uint instanceType, uint seed)
{
    bool isShortGrass = instanceType == FLORA_SPECIES_SHORT_GRASS;
    float meanHeight = isShortGrass ? 3.0 : 5.0;
    float standardDeviation = isShortGrass ? 0.6 : 1.0;
    float minimumHeight = float(isShortGrass
        ? SHORT_GRASS_MIN_HEIGHT_VOXELS
        : TALL_GRASS_MIN_HEIGHT_VOXELS);
    float maximumHeight = float(isShortGrass
        ? SHORT_GRASS_HEIGHT_VOXELS
        : TALL_GRASS_HEIGHT_VOXELS);
    return uint(round(clamp(meanHeight + sampleStandardNormal(seed) * standardDeviation,
                            minimumHeight, maximumHeight)));
}

float effectiveGrowthPotential(bool isSurfaceFlora, float competitionFactor,
                               float environmentFactor)
{
    if (isSurfaceFlora && gui_input.flora_growth_override_enabled != 0u)
    {
        return clamp(gui_input.flora_growth_override, 0.0, 1.0);
    }
    return min(competitionFactor, environmentFactor);
}

struct PreparedFloraVertex
{
    bool is_grass;
    float color_gradient;
    float3 voxel_position;
    float3 anchor_position;
    float shadow_weight;
    bool should_trim;
};

PreparedFloraVertex prepareFloraVertex(
    int3 voxelLocalPosition, uint voxelInfo, uint3 instancePositionVoxels,
    uint instanceType, uint seed, uint growthProgress,
    float competitionGrowthFactor, float environmentGrowthFactor,
    uint spawnStartMs)
{
    PreparedFloraVertex result;
    result.is_grass = instanceType == FLORA_SPECIES_TALL_GRASS ||
                      instanceType == FLORA_SPECIES_SHORT_GRASS;
    bool isSurfaceFlora = instanceType < FLORA_SPECIES_COUNT;
    bool isKochia = instanceType == FLORA_SPECIES_KOCHIA;
    bool isApple = instanceType == FLORA_SPECIES_APPLE;

    float windGradient = isApple ? 1.0 : floraVoxelWindGradient(voxelInfo);
    result.color_gradient = floraVoxelColorGradient(voxelInfo);
    uint grassHeightVoxels = result.is_grass
        ? sampleGrassHeight(instanceType, seed)
        : TALL_GRASS_HEIGHT_VOXELS;
    float growthPotential = effectiveGrowthPotential(
        isSurfaceFlora, competitionGrowthFactor, environmentGrowthFactor);
    float growthFactor = min(float(growthProgress) /
                             float(INSTANCE_GROWTH_PROGRESS_MATURE), growthPotential);
    result.should_trim = false;
    if (result.is_grass)
    {
        float grownHeight = floor(float(grassHeightVoxels) * growthFactor + 0.001);
        result.should_trim = float(voxelLocalPosition.y) >= grownHeight;
    }
    else if (growthFactor <= 0.0)
    {
        result.should_trim = true;
    }
    else
    {
        result.should_trim = floraVoxelGrowthGradient(voxelInfo) > growthFactor;
    }

    float3 instancePosition = float3(instancePositionVoxels) * SCALING_FACTOR;
    uint animationGroup = floraVoxelAnimationGroup(voxelInfo);
    if (isKochia && animationGroup > max(gui_input.kochia_branch_count, 1u))
    {
        result.should_trim = true;
    }

    uint windTopology = floraSpeciesWindMotionTopology(instanceType);
    float3 windSamplePosition = windTopology != 2u
        ? instancePosition
        : instancePosition + float3(voxelLocalPosition) * SCALING_FACTOR;
    uint windSeed = seed;
    if (windTopology == 1u)
    {
        windSeed = floraGroupedStemWindSeed(seed, animationGroup);
    }
    else if (windTopology == 2u)
    {
        windSeed = windVolumeVoxelSeed(seed, voxelLocalPosition);
    }
    float3 wind = sampleWindVolume(wind_volume_info, wind_volume_tex,
                                   windSamplePosition, windSeed);
    float grassRootedHeight = float(voxelLocalPosition.y) /
                              max(float(grassHeightVoxels) - 1.0, 1.0);
    float bendHeight = result.is_grass ? grassRootedHeight : windGradient;
    float bendWeight = pow(clamp(bendHeight, 0.0, 1.0),
                           max(gui_input.flora_bend_height_power, 0.05));
    float windBendWeight = isSurfaceFlora
        ? bendWeight
        : windGradient * windGradient;
    float speciesWindAffect = isSurfaceFlora
        ? floraSpeciesVoxelWindAffectMultiplier(gui_input, instanceType)
        : 1.0;
    float3 windOffset = isApple
        ? float3(0.0)
        : wind * windBendWeight * speciesWindAffect;
    float motionTime = windVolumeBucketUpdateTime(
        gui_input, windVolumeBucketIndex(windSeed), pc.time);

    if (isSurfaceFlora)
    {
        float naturalBendHeight = result.is_grass
            ? float(grassHeightVoxels)
            : floraVoxelLookupMaxLength(instanceType);
        float naturalBendValue = result.is_grass ? grassRootedHeight : windGradient;
        uint restBendSeed = windTopology == 1u ? windSeed : seed;
        windOffset += floraNaturalRestBend(
            gui_input, restBendSeed, naturalBendValue, naturalBendHeight) *
            speciesWindAffect;
        if (isKochia)
        {
            windOffset += kochiaBranchJellyMotion(
                gui_input, wind, windGradient, seed, animationGroup, motionTime);
        }
        else
        {
            windOffset += floraWindVibration(
                gui_input, wind, bendHeight, seed, voxelLocalPosition, motionTime) *
                speciesWindAffect;
        }
    }
    else if (instanceType == FLORA_SPECIES_TREE_LEAF)
    {
        windOffset += leafWindPaddling(
            gui_input, wind, windGradient, seed, voxelLocalPosition, motionTime);
    }
    else if (isApple)
    {
        windOffset += appleWindSwing(gui_input, wind, seed, motionTime);
    }

    float plantHeightVoxels = result.is_grass
        ? float(grassHeightVoxels)
        : floraVoxelLookupMaxLength(instanceType) + 1.0;
    FloraSpawnAnimationPose spawnPose;
    if (isSurfaceFlora)
    {
        spawnPose = sampleFloraSpawnAnimation(
            flora_growth_info, spawnStartMs, seed, plantHeightVoxels);
    }
    else
    {
        spawnPose.translation_voxels = float3(0.0);
        spawnPose.scale = float3(1.0);
        spawnPose.visibility = 1.0;
    }

    result.anchor_position =
        (float3(voxelLocalPosition) + windOffset + spawnPose.translation_voxels) *
        SCALING_FACTOR + instancePosition;
    result.voxel_position = result.anchor_position + 0.5 * SCALING_FACTOR;
    result.shadow_weight = stylizedVoxelShadowWeight(
        gui_input, sun_info, shadow_camera_info,
        shadow_map_tex_for_vsm_ping, leaf_shadow_opacity_blended_tex,
        leaf_shadow_mask_tex, cloud_shadow_tex,
        result.voxel_position, float3(voxelLocalPosition));
    return result;
}

uint floraHeightColorRow(float colorGradient)
{
    return uint(round(clamp(colorGradient, 0.0, 1.0) *
                      float(FLORA_HEIGHT_COLOR_TABLE_LEN - 1)));
}

float3 unpackLinearRgb10(uint packedColor)
{
    return float3(float(packedColor & 0x3ffu),
                  float((packedColor >> 10u) & 0x3ffu),
                  float((packedColor >> 20u) & 0x3ffu)) * (1.0 / 1023.0);
}

float3 sampleFloraBaseColor(bool isGrass, uint instanceType, uint seed,
                            int3 voxelLocalPosition, uint3 instancePositionVoxels,
                            float colorGradient, uint voxelInfo)
{
    uint materialId = floraVoxelMaterialId(voxelInfo);
    uint colorRow = floraHeightColorRow(colorGradient);
    uint darkPackedColor = pc.height_dark_color_rgb10[colorRow];
    if (instanceType == FLORA_SPECIES_LAVENDER)
    {
        darkPackedColor = sampleLavenderHeightPaletteRgb10(seed, colorRow);
    }
    float3 darkHeightColor = unpackLinearRgb10(darkPackedColor);

    if (instanceType == FLORA_SPECIES_KOCHIA)
    {
        float3 warmRed = unpackLinearRgb10(pc.height_light_color_rgb10[colorRow]);
        float paletteValue = constructFloat01(wellonsHash(seed ^ 0xD1B54A32u));
        float3 kochiaColor = lerp(darkHeightColor, warmRed, paletteValue);
        uint animationGroup = floraVoxelAnimationGroup(voxelInfo);
        uint branchSeed = wellonsHash(seed ^ (animationGroup * 0x9E3779B9u));
        float branchNoise = constructFloat01(branchSeed) * 2.0 - 1.0;
        float voxelNoise = signedUnitNoise(
            float4(float3(voxelLocalPosition), float(seed ^ branchSeed))).z;
        float branchHeight = floraSmootherstep(floraVoxelWindGradient(voxelInfo));
        float verticalOffset = -max(gui_input.kochia_bottom_darkening, 0.0) *
                               (1.0 - branchHeight);
        float valueOffset = verticalOffset +
            branchNoise * max(gui_input.kochia_branch_value_variation, 0.0) +
            voxelNoise * max(gui_input.kochia_voxel_value_variation, 0.0);
        return applyHsvOffset(kochiaColor, float3(0.0, 0.0, valueOffset));
    }

    if (instanceType == FLORA_SPECIES_EMBER_BLOOM &&
        materialId == FLORA_VOXEL_MATERIAL_ALLIUM_CORE)
    {
        const uint flowerColorRow = 8u;
        float3 flowerA = unpackLinearRgb10(pc.height_dark_color_rgb10[flowerColorRow]);
        float3 flowerB = unpackLinearRgb10(pc.height_light_color_rgb10[flowerColorRow]);
        if (constructFloat01(wellonsHash(seed ^ 0x63D83595u)) >= 0.5)
        {
            flowerA = srgbToLinear(float3(0.89, 0.72, 0.29));
            flowerB = srgbToLinear(float3(0.98, 0.96, 0.86));
        }
        float instanceOffset = constructFloat01(wellonsHash(seed ^ 0xA511E9B3u));
        float blendFactor = frac(dot(float3(voxelLocalPosition),
                                     float3(0.754877666, 0.569840296, 0.438289173)) +
                                 instanceOffset);
        return lerp(flowerA, flowerB, blendFactor);
    }

    if (isGrass)
    {
        float bandValue = sampleGrassBandInterpolation(
            float2(float(instancePositionVoxels.x), float(instancePositionVoxels.z)));
        float3 lightHeightColor =
            unpackLinearRgb10(pc.height_light_color_rgb10[colorRow]);
        return lerp(darkHeightColor, lightHeightColor, bandValue);
    }

    if (instanceType == FLORA_SPECIES_APPLE)
    {
        float speckle = signedUnitNoise(
            float4(float3(voxelLocalPosition), float(seed))).x;
        return applyHsvOffset(
            darkHeightColor, float3(speckle * 0.018, 0.03, speckle * 0.035));
    }

    float3 instanceVariation = signedUnitNoise(float(seed)) *
                               gui_input.flora_instance_hsv_offset_max;
    float3 voxelVariation = signedUnitNoise(
        float4(float3(voxelLocalPosition), float(seed))) *
        gui_input.flora_voxel_hsv_offset_max;
    return applyHsvOffset(darkHeightColor, instanceVariation + voxelVariation);
}

float3 applyGrassGrowthStressTint(float3 baseColor, bool isGrass,
                                  float growthFactor)
{
    if (!isGrass) return baseColor;
    float stress = 1.0 - clamp(growthFactor, 0.0, 1.0);
    return lerp(baseColor, srgbToLinear(float3(0.62, 0.68, 0.275)),
                stress * 0.55);
}

float4 applyDepthOffset(float3 worldPosition, uint3 instancePosition)
{
    uint hash = instancePosition.x * 73856093u ^
                instancePosition.y * 19349663u ^
                instancePosition.z * 83492791u;
    float depthOffset = frac(float(hash) * 0.0001) * 5.0e-4;
    float4 viewPosition = mul(camera_info.view_mat, float4(worldPosition, 1.0));
    viewPosition.z -= depthOffset;
    return mul(camera_info.proj_mat, viewPosition);
}

[shader("vertex")]
VertexOutput main(VertexInput input)
{
    int3 voxelLocalPosition;
    uint3 vertexOffsetInVoxel;
    unpackVertexData(input.packed_data, voxelLocalPosition, vertexOffsetInVoxel);

    Instance instance = manual_flora_instances.data[input.instance_index];
    uint packedLocalPosition = instance.packed_local_pos;
    uint3 instancePosition = surfaceFloraStemWorldPosition(
        packedLocalPosition, pc.chunk_world_offset);
    uint seed = instanceSeed(instancePosition);
    uint growthProgress = unpackInstanceGrowthProgress(packedLocalPosition);
    uint3 instanceLocalBase = unpackInstanceLocalPosition(packedLocalPosition);
    float competitionFactor = floraCompetitionGrowthFactor(
        instanceLocalBase, pc.instance_ty);
    float environmentFactor = floraEnvironmentGrowthFactor(
        instanceLocalBase, pc.chunk_world_offset, pc.instance_ty);
    uint voxelInfo = lookupFloraVoxelInfo(pc.instance_ty, voxelLocalPosition);
    PreparedFloraVertex prepared = prepareFloraVertex(
        voxelLocalPosition, voxelInfo, instancePosition, pc.instance_ty, seed,
        growthProgress, competitionFactor, environmentFactor,
        instance.spawn_start_ms);

    VertexOutput output;
    if (prepared.should_trim)
    {
        output.position = float4(2.0, 2.0, 2.0, 1.0);
        output.color = float3(0.0);
        return output;
    }

    float3 vertexPosition = prepared.anchor_position +
                            float3(vertexOffsetInVoxel) * SCALING_FACTOR;
    output.position = applyDepthOffset(vertexPosition, instancePosition);
    float3 baseColor = sampleFloraBaseColor(
        prepared.is_grass, pc.instance_ty, seed, voxelLocalPosition,
        instancePosition, prepared.color_gradient, voxelInfo);
    baseColor = applyGrassGrowthStressTint(
        baseColor, prepared.is_grass,
        effectiveGrowthPotential(true, competitionFactor, environmentFactor));
    float3 litColor = applyStylizedVoxelLighting(
        sun_info, shading_info, baseColor, prepared.shadow_weight);
    output.color = applyTerrainEditPreviewTint(
        terrain_edit_preview, litColor, prepared.voxel_position);
    return output;
}
