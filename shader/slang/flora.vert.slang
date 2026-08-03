import flora_types;
import flora_lighting_cache;
import flora_vertex;
import surface_flora_vertex;

struct B_ManualFloraInstances
{
    Instance data[];
};

[[vk::binding(0, 1)]]
readonly GLSLShaderStorageBuffer<B_ManualFloraInstances, Std430DataLayout>
    manual_flora_instances;

struct B_ManualFloraLightingCache
{
    float4 irradiance[];
};

[[vk::binding(2, 1)]]
readonly GLSLShaderStorageBuffer<B_ManualFloraLightingCache, Std430DataLayout>
    manual_flora_lighting_cache;

struct VertexInput
{
    [[vk::location(0)]] uint packed_data : TEXCOORD0;
    [[vk::location(1)]] uint voxel_index : TEXCOORD1;
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
    Instance instance = manual_flora_instances.data[input.instance_index];
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
    uint voxelInfo = lookupFloraVoxelInfo(
        instanceType, voxelLocalPosition);
    PreparedFloraVertex prepared = prepareFloraVertex(
        voxelLocalPosition, voxelInfo, instancePosition, instanceType, seed,
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
                            float3(vertexOffset) * FLORA_SCALING_FACTOR;
    output.position = applyFloraDepthOffset(vertexPosition, instancePosition);
    float3 baseColor = sampleFloraBaseColor(
        prepared.is_grass, instanceType, seed, voxelLocalPosition,
        instancePosition, prepared.color_gradient, voxelInfo);
    baseColor = applyGrassGrowthStressTint(
        baseColor, prepared.is_grass,
        effectiveGrowthPotential(true, competitionFactor, environmentFactor));
    uint localInstanceIndex = input.instance_index -
                              instanceType * MAX_FLORA_INSTANCES_PER_SPECIES;
    PushConstantFlora floraPc = floraPushConstants();
    float3 environmentLight = manual_flora_lighting_cache.irradiance[
        floraLightingCacheIndex(
            floraPc, localInstanceIndex, input.voxel_index)].xyz;
    output.color = shadeFloraVertexWithEnvironment(
        baseColor, prepared.shadow_weight, environmentLight,
        prepared.voxel_position);
    return output;
}
