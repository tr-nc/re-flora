import contree_node;
import flora_lighting_cache;
import flora_types;
import flora_vertex;
import local_lighting;

[[vk::binding(43, 0)]] StructuredBuffer<U_LightGpu> local_lights;
[[vk::binding(44, 0)]] StructuredBuffer<ContreeNode> contree_node_data;
[[vk::binding(45, 0)]] StructuredBuffer<uint> contree_leaf_data;
[[vk::binding(46, 0)]] [[vk::image_format("rg32ui")]]
readonly RWTexture3D<uint2> scene_tex;

struct B_TreeLeafInstances
{
    TreeLeafInstance data[];
};

[[vk::binding(0, 1)]]
readonly GLSLShaderStorageBuffer<B_TreeLeafInstances, Std430DataLayout>
    tree_leaf_instances;

[[vk::binding(2, 1)]]
RWStructuredBuffer<float4> flora_lighting_cache;

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    PushConstantFlora floraPc = floraPushConstants();
    uint instanceCount = floraLightingCacheInstanceCount(floraPc);
    uint localInstanceIndex = dispatchThreadId.x;
    if (localInstanceIndex >= instanceCount) return;

    uint3 chunkWorldOffset = floraChunkWorldOffset();
    TreeLeafInstance leafInstance = tree_leaf_instances.data[localInstanceIndex];
    uint3 leafVoxelWorldPosition = treeLeafWorldPosition(
        leafInstance.packed_local_pos, chunkWorldOffset);
    int3 leafVoxelLocalPosition = unpackTreeLeafVoxelLocalPosition(
        leafInstance.packed_leaf_local_pos);
    uint3 instancePosition = uint3(
        int3(leafVoxelWorldPosition) - leafVoxelLocalPosition);
    uint seed = instanceSeed(instancePosition);
    uint voxelInfo = lookupFloraVoxelInfo(
        FLORA_SPECIES_TREE_LEAF, leafVoxelLocalPosition);
    PreparedFloraVertex prepared = prepareFloraVertex(
        leafVoxelLocalPosition, voxelInfo, instancePosition,
        FLORA_SPECIES_TREE_LEAF, seed, INSTANCE_GROWTH_PROGRESS_MATURE,
        1.0, 1.0, INSTANCE_SPAWN_INACTIVE);

    float3 irradiance = prepared.should_trim ||
                        !rasterFloraDdgiLightingEnabled()
        ? float3(0.0)
        : sampleFloraEnvironment(
            prepared.voxel_position, prepared.shading_normal);
    if (!prepared.should_trim)
    {
        irradiance += evaluateVoxelVisibleLocalLightIrradiance(
            floraLocalLightInfo(), local_lights, contree_node_data,
            contree_leaf_data, scene_tex, prepared.voxel_position,
            prepared.shading_normal,
            floraTerrainRayOriginOffsetWorld(), localInstanceIndex);
    }
    flora_lighting_cache[
        floraLightingCacheIndex(floraPc, localInstanceIndex, 0u)] =
        float4(irradiance, 1.0);
}
