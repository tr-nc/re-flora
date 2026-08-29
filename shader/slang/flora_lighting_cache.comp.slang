#ifndef RE_FLORA_GLASS_TRANSPORT
#define RE_FLORA_GLASS_TRANSPORT 0
#endif

import contree_node;
import flora_lighting_cache;
#if RE_FLORA_GLASS_TRANSPORT
import flora_lighting_resources;
#endif
import flora_types;
import flora_vertex;
import surface_flora_vertex;
import local_lighting;

[[vk::binding(43, 0)]] StructuredBuffer<U_LightGpu> local_lights;
[[vk::binding(44, 0)]] StructuredBuffer<ContreeNode> contree_node_data;
[[vk::binding(45, 0)]] StructuredBuffer<uint> contree_leaf_data;
[[vk::binding(46, 0)]] [[vk::image_format("rg32ui")]]
readonly RWTexture3D<uint2> scene_tex;

struct B_ManualFloraInstances
{
    Instance data[];
};

[[vk::binding(0, 1)]]
readonly GLSLShaderStorageBuffer<B_ManualFloraInstances, Std430DataLayout>
    flora_instances;

struct B_ManualFloraVertices
{
    uint2 data[];
};

[[vk::binding(3, 1)]]
readonly GLSLShaderStorageBuffer<B_ManualFloraVertices, Std430DataLayout>
    flora_vertices;

[[vk::binding(2, 1)]]
RWStructuredBuffer<float4> flora_lighting_cache;

[shader("compute")]
[numthreads(8, 4, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    PushConstantFlora floraPc = floraPushConstants();
    uint voxelCount = floraLightingCacheVoxelCount(floraPc);
    uint instanceCount = floraLightingCacheInstanceCount(floraPc);
    if (dispatchThreadId.x >= voxelCount || dispatchThreadId.y >= instanceCount) return;

    uint verticesPerVoxel = floraLightingCacheVerticesPerVoxel(floraPc);
    uint packedVertex =
        flora_vertices.data[dispatchThreadId.x * verticesPerVoxel].x;
    int3 voxelLocalPosition;
    uint3 unusedVertexOffset;
    unpackFloraVertexData(packedVertex, voxelLocalPosition, unusedVertexOffset);

    uint instanceType = floraInstanceType();
    uint localInstanceIndex = dispatchThreadId.y;
    uint instanceIndex = instanceType * MAX_FLORA_INSTANCES_PER_SPECIES +
                         localInstanceIndex;
    uint3 chunkWorldOffset = floraChunkWorldOffset();
    Instance instance = flora_instances.data[instanceIndex];
    uint packedLocalPosition = instance.packed_local_pos;
    uint3 instancePosition = surfaceFloraStemWorldPosition(
        packedLocalPosition, chunkWorldOffset);
    uint seed = instanceSeed(instancePosition);
    uint growthProgress = unpackInstanceGrowthProgress(packedLocalPosition);
    uint3 instanceLocalBase = unpackInstanceLocalPosition(packedLocalPosition);
    float competitionFactor = floraCompetitionGrowthFactor(
        instanceLocalBase, instanceType);
    float environmentFactor = floraEnvironmentGrowthFactor(
        instanceLocalBase, chunkWorldOffset, instanceType);
    uint voxelInfo = lookupFloraVoxelInfo(instanceType, voxelLocalPosition);
    PreparedFloraVertex prepared = prepareFloraVertex(
        voxelLocalPosition, voxelInfo, instancePosition, instanceType, seed,
        growthProgress, competitionFactor, environmentFactor,
        instance.spawn_start_ms);

    float3 irradiance = prepared.should_trim ||
                        !rasterFloraDdgiLightingEnabled()
        ? float3(0.0)
        : sampleFloraEnvironment(
            prepared.voxel_position, prepared.shading_normal);
    if (!prepared.should_trim)
    {
#if RE_FLORA_GLASS_TRANSPORT
        irradiance += evaluateVoxelGlassVisibleLocalLightIrradiance(
            floraLocalLightInfo(), local_lights, contree_node_data,
            contree_leaf_data, scene_tex, chunk_atlas, prepared.voxel_position,
            prepared.shading_normal,
            floraTerrainRayOriginOffsetWorld(), dispatchThreadId.x);
#else
        irradiance += evaluateVoxelVisibleLocalLightIrradiance(
            floraLocalLightInfo(), local_lights, contree_node_data,
            contree_leaf_data, scene_tex, prepared.voxel_position,
            prepared.shading_normal,
            floraTerrainRayOriginOffsetWorld(), dispatchThreadId.x);
#endif
    }
    flora_lighting_cache[
        floraLightingCacheIndex(
            floraPc, localInstanceIndex, dispatchThreadId.x)] =
        float4(irradiance, 1.0);
}
