import flora_lighting_cache;
import flora_types;
import flora_vertex;
import surface_flora_vertex;

struct B_ManualFloraInstances
{
    Instance data[];
};

[[vk::binding(0, 1)]]
readonly GLSLShaderStorageBuffer<B_ManualFloraInstances, Std430DataLayout>
    manual_flora_instances;

struct B_ManualFloraVertices
{
    uint2 data[];
};

[[vk::binding(3, 1)]]
readonly GLSLShaderStorageBuffer<B_ManualFloraVertices, Std430DataLayout>
    manual_flora_vertices;

[[vk::binding(2, 1)]]
RWStructuredBuffer<float4> manual_flora_lighting_cache;

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
        manual_flora_vertices.data[dispatchThreadId.x * verticesPerVoxel].x;
    int3 voxelLocalPosition;
    uint3 unusedVertexOffset;
    unpackFloraVertexData(packedVertex, voxelLocalPosition, unusedVertexOffset);

    uint instanceType = floraInstanceType();
    uint localInstanceIndex = dispatchThreadId.y;
    uint instanceIndex = instanceType * MAX_FLORA_INSTANCES_PER_SPECIES +
                         localInstanceIndex;
    uint3 chunkWorldOffset = floraChunkWorldOffset();
    Instance instance = manual_flora_instances.data[instanceIndex];
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

    float3 irradiance = prepared.should_trim
        ? float3(0.0)
        : sampleFloraEnvironment(
            prepared.voxel_position, prepared.shading_normal);
    manual_flora_lighting_cache[
        floraLightingCacheIndex(
            floraPc, localInstanceIndex, dispatchThreadId.x)] =
        float4(irradiance, 1.0);
}
