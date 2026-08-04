import flora_motion;
import flora_types;
import hash;
import tracer_types;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(3, 0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(7, 0)]] ConstantBuffer<U_WindVolumeInfo> wind_volume_info;
[[vk::binding(8, 0)]] Sampler3D wind_volume_tex;

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

struct B_TreeLeafInstances
{
    TreeLeafInstance data[];
};

[[vk::binding(0, 1)]]
readonly GLSLShaderStorageBuffer<B_TreeLeafInstances, Std430DataLayout>
    tree_leaf_instances;

[[vk::push_constant]]
ConstantBuffer<PushConstantFlora> pc;

static const float SCALING_FACTOR = 1.0 / 256.0;
static const uint FLORA_VOXEL_LOOKUP_EMPTY_KEY = 0xffffffffu;
static const uint FLORA_VOXEL_LOOKUP_MAX_PROBES = 64u;
static const uint FLORA_LOOKUP_BITS_PER_AXIS = 10u;
static const uint FLORA_LOOKUP_AXIS_MASK =
    (1u << FLORA_LOOKUP_BITS_PER_AXIS) - 1u;
static const int FLORA_LOOKUP_AXIS_BIAS =
    1 << (FLORA_LOOKUP_BITS_PER_AXIS - 1u);

struct VertexInput
{
    [[vk::location(0)]] uint packed_data : TEXCOORD0;
    uint instance_index : SV_VulkanInstanceID;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float opacity : TEXCOORD0;
};

void unpackVertexData(uint packedData, out int3 voxelLocalPosition,
                      out uint3 vertexOffset)
{
    const uint positionMask = 0x7fu;
    voxelLocalPosition = int3(
        packedData & positionMask,
        (packedData >> 7u) & positionMask,
        (packedData >> 14u) & positionMask) - 64;
    uint packedOffset = (packedData >> 21u) & 0x7u;
    vertexOffset = uint3(
        packedOffset & 1u,
        (packedOffset >> 1u) & 1u,
        (packedOffset >> 2u) & 1u);
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
        uint2 entry = flora_voxel_infos.entries[
            offset + ((startSlot + probe) & mask)];
        if (entry.x == key) return entry.y;
        if (entry.x == FLORA_VOXEL_LOOKUP_EMPTY_KEY) return fallbackInfo;
    }
    return fallbackInfo;
}

float3 billboardVertexPosition(float3 voxelPosition, uint3 vertexOffset)
{
    float2 mappedOffset =
        (float2(vertexOffset.xy) - 0.5) * SCALING_FACTOR * 1.225;
    float3 right = normalize(float3(
        shadow_camera_info.view_mat[0][0],
        shadow_camera_info.view_mat[0][1],
        shadow_camera_info.view_mat[0][2]));
    float3 up = normalize(float3(
        shadow_camera_info.view_mat[1][0],
        shadow_camera_info.view_mat[1][1],
        shadow_camera_info.view_mat[1][2]));
    return voxelPosition + right * mappedOffset.x + up * mappedOffset.y;
}

[shader("vertex")]
VertexOutput main(VertexInput input)
{
    int3 voxelLocalPosition;
    uint3 vertexOffset;
    unpackVertexData(input.packed_data, voxelLocalPosition, vertexOffset);

    TreeLeafInstance leafInstance =
        tree_leaf_instances.data[input.instance_index];
    uint3 leafVoxelWorldPosition = treeLeafWorldPosition(
        leafInstance.packed_local_pos, pc.chunk_world_offset);
    bool isTreeLeaf = pc.instance_ty == FLORA_SPECIES_TREE_LEAF;
    int3 leafVoxelLocalPosition = isTreeLeaf
        ? unpackTreeLeafVoxelLocalPosition(leafInstance.packed_leaf_local_pos)
        : voxelLocalPosition;
    uint voxelInfo = lookupFloraVoxelInfo(
        pc.instance_ty, leafVoxelLocalPosition);
    float windGradient = pc.instance_ty == FLORA_SPECIES_APPLE
        ? 1.0
        : floraVoxelWindGradient(voxelInfo);

    uint3 instancePositionVoxels = isTreeLeaf
        ? uint3(int3(leafVoxelWorldPosition) - leafVoxelLocalPosition)
        : leafVoxelWorldPosition;
    float3 instancePosition =
        float3(instancePositionVoxels) * SCALING_FACTOR;
    uint seed = instanceSeed(instancePositionVoxels);
    bool isApple = pc.instance_ty == FLORA_SPECIES_APPLE;
    if (isApple && !treeFruitVoxelVisible(
            leafVoxelLocalPosition,
            unpackTreeFruitRadiusVoxels(leafInstance.packed_leaf_local_pos)))
    {
        VertexOutput output;
        output.position = float4(2.0, 2.0, 2.0, 1.0);
        output.opacity = 0.0;
        return output;
    }
    float3 windSamplePosition = isApple
        ? instancePosition
        : instancePosition + float3(leafVoxelLocalPosition) * SCALING_FACTOR;
    uint windSeed = isApple
        ? seed
        : windVolumeVoxelSeed(seed, leafVoxelLocalPosition);
    float3 wind = sampleWindVolume(
        wind_volume_info, wind_volume_tex, windSamplePosition, windSeed);
    float3 windOffset = wind * windGradient * windGradient;
    float motionTime = windVolumeBucketUpdateTime(
        gui_input, windVolumeBucketIndex(windSeed), pc.time);
    if (isApple)
    {
        windOffset = appleWindSwing(gui_input, wind, seed, motionTime);
    }
    else
    {
        windOffset += leafWindPaddling(
            gui_input, wind, windGradient, seed, leafVoxelLocalPosition,
            motionTime);
    }

    float3 anchorPosition =
        (float3(leafVoxelLocalPosition) + windOffset) * SCALING_FACTOR +
        instancePosition;
    float3 voxelPosition = anchorPosition + 0.5 * SCALING_FACTOR;
    float3 vertexPosition = billboardVertexPosition(voxelPosition, vertexOffset);
    VertexOutput output;
    output.position = mul(
        shadow_camera_info.view_proj_mat, float4(vertexPosition, 1.0));
    output.opacity = clamp(gui_input.leaf_shadow_fragment_opacity, 0.0, 1.0);

    uint paletteSeed = wellonsHash(seed);
    output.position.z += float(paletteSeed & 1u) * 1.0e-8;

    return output;
}
