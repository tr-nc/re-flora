import flora_types;
import flora_vertex;

struct B_TreeLeafInstances
{
    TreeLeafInstance data[];
};

[[vk::binding(0, 1)]]
readonly GLSLShaderStorageBuffer<B_TreeLeafInstances, Std430DataLayout>
    manual_tree_leaf_instances;

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

[shader("vertex")]
VertexOutput main(VertexInput input)
{
    int3 voxelLocalPosition;
    uint3 vertexOffset;
    unpackFloraVertexData(
        input.packed_data, voxelLocalPosition, vertexOffset);

    uint instanceType = floraInstanceType();
    uint3 chunkWorldOffset = floraChunkWorldOffset();
    TreeLeafInstance leafInstance =
        manual_tree_leaf_instances.data[input.instance_index];
    uint3 leafVoxelWorldPosition = treeLeafWorldPosition(
        leafInstance.packed_local_pos, chunkWorldOffset);
    bool isTreeLeaf = instanceType == FLORA_SPECIES_TREE_LEAF;
    bool isApple = instanceType == FLORA_SPECIES_APPLE;
    int3 leafVoxelLocalPosition = isTreeLeaf
        ? unpackTreeLeafVoxelLocalPosition(leafInstance.packed_leaf_local_pos)
        : voxelLocalPosition;
    uint3 instancePosition = isTreeLeaf
        ? uint3(int3(leafVoxelWorldPosition) - leafVoxelLocalPosition)
        : leafVoxelWorldPosition;
    uint seed = instanceSeed(instancePosition);
    uint voxelInfo = lookupFloraVoxelInfo(
        instanceType, leafVoxelLocalPosition);
    PreparedFloraVertex prepared = prepareFloraVertex(
        leafVoxelLocalPosition, voxelInfo, instancePosition, instanceType, seed,
        INSTANCE_GROWTH_PROGRESS_MATURE, 1.0, 1.0,
        INSTANCE_SPAWN_INACTIVE);

    VertexOutput output;
    if (prepared.should_trim)
    {
        output.position = float4(2.0, 2.0, 2.0, 1.0);
        output.color = float3(0.0);
        return output;
    }

    float3 vertexPosition = floraBillboardVertexPosition(
        prepared.voxel_position, vertexOffset);
    if (isApple)
    {
        float scale = unpackTreeFruitScale(leafInstance.packed_leaf_local_pos);
        float3 fruitBase = prepared.voxel_position -
            (float3(leafVoxelLocalPosition) + 0.5) * FLORA_SCALING_FACTOR;
        vertexPosition = fruitBase + (vertexPosition - fruitBase) * scale;
    }
    output.position = projectFloraPosition(vertexPosition);
    float3 baseColor = sampleFloraBaseColor(
        prepared.is_grass, instanceType, seed, leafVoxelLocalPosition,
        instancePosition, prepared.color_gradient, voxelInfo);
    output.color = shadeFloraVertex(
        baseColor, prepared.shadow_weight, prepared.voxel_position);
    return output;
}
