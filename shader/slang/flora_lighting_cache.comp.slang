import plant_hydration;
import grass_visibility;
#ifndef RE_FLORA_GLASS_TRANSPORT
#define RE_FLORA_GLASS_TRANSPORT 0
#endif

// Separate compiled programs: disabled experiments and disabled instrumentation
// must not inflate the original/cached kernel's live state or resource barriers.
#ifndef RE_FLORA_LOCAL_VISIBILITY
#define RE_FLORA_LOCAL_VISIBILITY 0
#endif

#ifndef RE_FLORA_LIGHT_RIS
#define RE_FLORA_LIGHT_RIS 0
#endif
#ifndef RE_FLORA_RIS_AUDIT
#define RE_FLORA_RIS_AUDIT 0
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
#if RE_FLORA_LOCAL_VISIBILITY == 2 || RE_FLORA_RIS_AUDIT
StructuredBuffer<float4> flora_lighting_cache;
#else
RWStructuredBuffer<float4> flora_lighting_cache;
#endif

#if RE_FLORA_LOCAL_VISIBILITY
[[vk::binding(11, 1)]] StructuredBuffer<float4> local_light_field;
#if RE_FLORA_LOCAL_VISIBILITY == 2
[[vk::binding(12, 1)]] RWStructuredBuffer<uint> local_light_field_audit;
#endif
#include "local_visibility_query.slang"
#endif

#if RE_FLORA_LIGHT_RIS
#include "flora_light_ris.slang"
#endif
#if RE_FLORA_RIS_AUDIT
#include "flora_ris_audit.slang"
#endif

[shader("compute")]
[numthreads(8, 4, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID, uint localIndex : SV_GroupIndex,
    uint3 groupId : SV_GroupID)
{
    PushConstantFlora floraPc = floraPushConstants();
    uint voxelCount = floraLightingCacheVoxelCount(floraPc);
    uint instanceCount = grassVisibleCount(floraPc);
#if RE_FLORA_LIGHT_RIS
    presampleFloraLights(localIndex,groupId,floraPc);
#endif
    if (dispatchThreadId.x >= voxelCount || dispatchThreadId.y >= instanceCount) return;

    // Grass bands are authored axial layers, not vertices in a voxel mesh.
    int3 voxelLocalPosition = int3(0, int(dispatchThreadId.x), 0);

    uint instanceType = floraInstanceType();
    uint localInstanceIndex = grassSourceIndex(floraPc, dispatchThreadId.y);
#if RE_FLORA_LOCAL_VISIBILITY == 2
    // Verify the normal program's actual output, in the same frame. This
    // independent dispatch is opt-in and never replaces production rendering.
    bool audit=local_light_field_audit[15]!=0u &&
        ((localInstanceIndex*1664525u+dispatchThreadId.x*1013904223u)&1023u)==0u;
    if(!audit) return;
#endif
#if RE_FLORA_RIS_AUDIT
    // Rotate the audit subset, not the production sampler. Partial groups must
    // already have crossed the shared-proposal barrier before this early exit.
    if((lightSamplingHash(localInstanceIndex ^ dispatchThreadId.x*1013904223u ^
        uint(floraPc.time*60.0)) & 255u)!=0u) return;
#endif
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
#if RE_FLORA_RIS_AUDIT
    float3 environment=irradiance;
#endif
    if (!prepared.should_trim)
    {
#if RE_FLORA_LIGHT_RIS
        irradiance += evaluateFloraRis(floraLocalLightInfo(),prepared.voxel_position,
            prepared.shading_normal,floraTerrainRayOriginOffsetWorld(),
            lightSamplingHash(seed ^ lightSamplingHash(dispatchThreadId.x)),localIndex);
#elif RE_FLORA_LOCAL_VISIBILITY
#if RE_FLORA_LOCAL_VISIBILITY != 2
        const bool audit=false;
#endif
        irradiance += evaluateCachedLocalVisibility(floraLocalLightInfo(),
            shading_info.ddgi_geometry_revision, prepared.voxel_position,
            prepared.shading_normal, floraTerrainRayOriginOffsetWorld(), localIndex, audit);
#else
#if RE_FLORA_GLASS_TRANSPORT
        irradiance += evaluateVoxelGlassVisibleLocalLightIrradiance(
            floraLocalLightInfo(), local_lights, contree_node_data,
            contree_leaf_data, scene_tex, chunk_atlas, prepared.voxel_position,
            prepared.shading_normal,
            floraTerrainRayOriginOffsetWorld(), localIndex);
#else
        irradiance += evaluateVoxelVisibleLocalLightIrradiance(
            floraLocalLightInfo(), local_lights, contree_node_data,
            contree_leaf_data, scene_tex, prepared.voxel_position,
            prepared.shading_normal,
            floraTerrainRayOriginOffsetWorld(), localIndex);
#endif
#endif
    }
    uint outputIndex=floraLightingCacheIndex(floraPc,dispatchThreadId.y,dispatchThreadId.x);
#if RE_FLORA_RIS_AUDIT
    auditFloraRis(environment,irradiance,flora_lighting_cache[outputIndex].xyz,prepared,localIndex);
#elif RE_FLORA_LOCAL_VISIBILITY == 2
    float3 rendered=flora_lighting_cache[outputIndex].xyz;
    float3 delta=abs(rendered-irradiance);
    float error=all(isfinite(rendered)) && all(isfinite(irradiance))
        ? max(delta.x,max(delta.y,delta.z)) : 3.402823e38;
    if(error>1.0e-4+1.0e-4*max(irradiance.x,max(irradiance.y,irradiance.z)))
        InterlockedAdd(local_light_field_audit[13],1u);
    InterlockedMax(local_light_field_audit[14],asuint(error));
#else
    flora_lighting_cache[outputIndex]=float4(irradiance,1.0);
#endif
#if RE_FLORA_STEM_BAND_CACHE && RE_FLORA_LOCAL_VISIBILITY != 2
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
