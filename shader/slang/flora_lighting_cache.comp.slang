import plant_hydration;
import grass_visibility;
#ifndef RE_FLORA_GLASS_TRANSPORT
#define RE_FLORA_GLASS_TRANSPORT 0
#endif

#ifndef RE_FLORA_STEM_BAND_CACHE
#define RE_FLORA_STEM_BAND_CACHE 0
#endif
#if RE_FLORA_STEM_BAND_CACHE
import grass_band_cache;
[[vk::binding(9, 1)]] RWStructuredBuffer<GrassBandPose> grass_band_pose_cache;
#endif

import contree_node;
import flora_lighting_cache;
#if RE_FLORA_GLASS_TRANSPORT
import flora_lighting_resources;
#endif
import flora_types;
import flora_vertex;
import grass_growth_shape;
import surface_flora_vertex;
import grass_moisture_material;
import vegetation_response_sample;
import local_lighting;
import ddgi_query;
import local_light_field_query;

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

[[vk::binding(2, 1)]]
RWStructuredBuffer<float4> flora_lighting_cache;

[[vk::binding(11, 1)]] StructuredBuffer<uint2> local_light_field;
[[vk::binding(12, 1)]] RWStructuredBuffer<uint> local_light_field_audit;

float3 originalLocalLightResponse(U_LocalLightInfo info, uint lightIndex,
    float3 position, float3 normal, uint invocation)
{
#if RE_FLORA_GLASS_TRANSPORT
    return evaluateVoxelGlassVisibleLocalLightAtIndex(
        info, local_lights, contree_node_data, contree_leaf_data, scene_tex,
        chunk_atlas, position, normal,
        floraTerrainRayOriginOffsetWorld(), invocation, lightIndex).irradiance;
#else
    return evaluateVoxelVisibleLocalLightAtIndex(
        info, local_lights, contree_node_data, contree_leaf_data, scene_tex,
        position, normal, floraTerrainRayOriginOffsetWorld(), invocation, lightIndex).irradiance;
#endif
}

[shader("compute")]
[numthreads(8, 4, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    PushConstantFlora floraPc = floraPushConstants();
    uint voxelCount = floraLightingCacheVoxelCount(floraPc);
    uint instanceCount = grassVisibleCount(floraPc);
    if (dispatchThreadId.x >= voxelCount || dispatchThreadId.y >= instanceCount) return;

    // Grass bands are authored axial layers, not vertices in a voxel mesh.
    int3 voxelLocalPosition = int3(0, int(dispatchThreadId.x), 0);

    uint instanceType = floraInstanceType();
    uint localInstanceIndex = grassSourceIndex(floraPc, dispatchThreadId.y);
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
    FloraSupportSample support = floraSupportSample(
        instanceLocalBase, chunkWorldOffset, instanceType);
    float environmentFactor = plantHydrationGrowthFactor(flora_growth_info,instanceType,localInstanceIndex);
    uint voxelInfo = lookupFloraVoxelInfo(instanceType, voxelLocalPosition);
    PreparedFloraVertex prepared = prepareFloraVertex(
        voxelLocalPosition, voxelInfo, instancePosition, instanceType, seed,
        growthProgress, competitionFactor, environmentFactor,
        instance.spawn_start_ms, vegetationResponseEnabled(),
        sampleVegetationResponse(instancePosition, seed, instanceType,
            localInstanceIndex, floraPc.response_offset), 0.0, plantHydrationDeficit(instanceType,localInstanceIndex,gui_input.plant_wilt_start_hydration));

    float3 irradiance = prepared.should_trim
        ? float3(0.0)
        : sampleFloraEnvironment(
            prepared.voxel_position, prepared.shading_normal);
    if (!prepared.should_trim)
    {
        DdgiReceiverProbeSupport probeSupport;
        bool sharedSupport = gui_input.grass_local_light_field != 0u &&
            tryGetDdgiReceiverProbeSupport(shading_info, prepared.voxel_position,
                prepared.shading_normal, probeSupport);
        U_LocalLightInfo info = floraLocalLightInfo();
        // Opt-in, deterministic 1/1024 receiver sampling. Never enabled in perf
        // comparisons; include real grass positions, not just probe positions.
        bool receiverAudit = gui_input.grass_local_light_field != 0u &&
            ((seed ^ (dispatchThreadId.x * 0x9e3779b9u)) & 1023u) == 0u &&
            localLightFieldReceiverAuditEnabled(local_light_field);
        float3 localDirect = float3(0.0);
        for (uint lightIndex = 0u; lightIndex < localLightCount(info); ++lightIndex)
        {
            float3 direct;
            bool usedField = sharedSupport && trySampleLocalLightConnectionResponse(
                probeSupport, local_light_field, info, local_lights[lightIndex], lightIndex,
                prepared.voxel_position, prepared.shading_normal, direct);
            if (!usedField) direct = originalLocalLightResponse(info, lightIndex,
                prepared.voxel_position, prepared.shading_normal, dispatchThreadId.x);
            localDirect += direct;
            if (receiverAudit)
            {
                float3 reference = usedField ? originalLocalLightResponse(info, lightIndex,
                    prepared.voxel_position, prepared.shading_normal, dispatchThreadId.x) : direct;
                float3 difference = abs(direct-reference);
                float error = max(difference.x,max(difference.y,difference.z));
                float scale = max(reference.x,max(reference.y,reference.z));
                InterlockedAdd(local_light_field_audit[4],1u);
                if (any(direct>float3(0.0)))
                    InterlockedAdd(local_light_field_audit[usedField?5u:6u],1u);
                if (error > 1e-4 + .05*scale) InterlockedAdd(local_light_field_audit[7],1u);
                InterlockedMax(local_light_field_audit[8],asuint(error));
                InterlockedMax(local_light_field_audit[9],asuint(error/max(scale,1e-4)));
                InterlockedAdd(local_light_field_audit[usedField?10u:11u],1u);
            }
        }
        irradiance += localDirect;
    }
    flora_lighting_cache[
        floraLightingCacheIndex(
            floraPc, dispatchThreadId.y, dispatchThreadId.x)] =
        float4(irradiance, 1.0);
#if RE_FLORA_STEM_BAND_CACHE
    // Reuse the existing once-per-layer preparation and lighting work. The
    // Irradiance-only preparation remains for the non-cached mesh consumer.
    float3 color = sampleFloraBaseColor(true, instanceType, seed, voxelLocalPosition,
        instancePosition, prepared.color_gradient, voxelInfo);
    color = applyGrassMoistureMaterial(color, instanceType, support.moisture_level,
        gui_input.grass_moisture_response != 0u);
    GrassBandPose pose;
    pose.position_height = float4(prepared.voxel_position,
        float(sampleGrassHeight(instanceType, seed)));
    pose.color_valid = float4(shadeFloraVertexWithEnvironment(color,
        prepared.shadow_weight, irradiance, prepared.voxel_position),
        prepared.should_trim ? 0.0 : 1.0);
    grass_band_pose_cache[floraLightingCacheIndex(
        floraPc, dispatchThreadId.y, dispatchThreadId.x)] = pose;
#endif
}
