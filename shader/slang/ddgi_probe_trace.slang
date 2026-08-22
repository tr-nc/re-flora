import contree_node;
import ddgi_config;
import ddgi_query;
import ddgi_sampling;
import ddgi_types;
import marching_result;
import scene_marching;
import skylight;
import terrain_ray_origin;
import tracer_material;

static const uint DDGI_TRACE_WORKGROUP_SIZE = 64u;
static const uint DDGI_RAYS_PER_THREAD =
    DDGI_RAYS_PER_PROBE / DDGI_TRACE_WORKGROUP_SIZE;

[[vk::binding(7, 0)]]
StructuredBuffer<ContreeNode> contree_node_data;

[[vk::binding(8, 0)]]
StructuredBuffer<uint> contree_leaf_data;

[[vk::binding(9, 0)]] [[vk::image_format("rg32ui")]]
readonly RWTexture3D<uint2> scene_tex;

[[vk::binding(25, 0)]]
GLSLShaderStorageBuffer<B_DdgiRayData, Std430DataLayout>
    ddgi_transient_ray_data;

[[vk::binding(26, 0)]]
GLSLShaderStorageBuffer<B_DdgiTraceStats, Std430DataLayout>
    ddgi_trace_stats;

// These builder-only bindings shape the immutable source/destination transport seam. An initial
// epoch has black indirect history; later epochs consume the previous complete field.
[[vk::binding(29, 0)]]
Sampler2D ddgi_transport_source_irradiance_atlas;

[[vk::binding(30, 0)]]
ConstantBuffer<U_DdgiRadianceSun> ddgi_radiance_sun;

[[vk::binding(31, 0)]]
ConstantBuffer<U_DdgiRadianceVoxelPalette> ddgi_radiance_voxel_palette;

[[vk::binding(32, 0)]]
ConstantBuffer<U_DdgiTransportQueryInfo> ddgi_transport_query_info;

[[vk::binding(39, 0)]]
Sampler2D ddgi_transport_source_visibility_atlas;

struct DdgiProbeTracePushConstants
{
    uint first_probe_index;
    uint probe_count;
    uint terrain_revision;
    uint has_history;
    uint source_slot;
    float far_distance_world;
    uint2 padding;
    float4 epoch_rotation;
    uint4 local_refresh_enabled;
    float4 local_refresh_world_min;
    float4 local_refresh_world_max;
};

[[vk::push_constant]]
ConstantBuffer<DdgiProbeTracePushConstants> pc;

void recordTraceStat(uint index)
{
    InterlockedAdd(ddgi_trace_stats.data[index], 1u);
}

float ddgiExactTerrainSunVisibility(MarchingResult receiver,
                                    float3 sunDirection,
                                    uint localInvocationIndex)
{
    float3 origin = terrainRayOriginAlongNormal(
        receiver.center_position, receiver.normal,
        ddgi_radiance_sun.terrain_ray_origin_offset_world);
    MarchingResult shadowHit = marchScene(
        contree_node_data, contree_leaf_data, scene_tex,
        origin, sunDirection, 1.0 / sunDirection, localInvocationIndex);
    return shadowHit.is_hit ? 0.0 : 1.0;
}

float3 ddgiTransportHitRadiance(MarchingResult result,
                                uint localInvocationIndex,
                                bool useHistory)
{
    float3 normal = normalize(result.normal);
    float3 sunDirection = normalize(ddgi_radiance_sun.direction);
    float cosine = max(0.0, dot(normal, sunDirection));
    float sunAboveHorizon = ddgi_radiance_sun.direction.y > 0.0 ? 1.0 : 0.0;
    float visibility = (cosine > 0.0 && sunAboveHorizon > 0.0)
        ? ddgiExactTerrainSunVisibility(
              result, sunDirection, localInvocationIndex)
        : 0.0;
    float3 directIrradiance = ddgi_radiance_sun.color *
        ddgi_radiance_sun.luminance * sunAboveHorizon * cosine * visibility;
    float3 ddgiReceiverPosition = terrainVoxelSurfacePositionAlongNormal(
        result.center_position, normal);
    float3 ddgiHardVisibilityPosition = terrainRayOriginAlongNormal(
        result.center_position, normal,
        ddgi_radiance_sun.terrain_ray_origin_offset_world);
    float3 indirectIrradiance = float3(0.0);
    if (useHistory && pc.source_slot == 0u)
    {
        indirectIrradiance = sampleDdgiTransportAtlas0(
            ddgi_transport_query_info, ddgiReceiverPosition, normal,
            ddgiHardVisibilityPosition).irradiance;
    }
    else if (useHistory)
    {
        indirectIrradiance = sampleDdgiTransportSource(
            ddgi_transport_query_info,
            ddgi_transport_source_irradiance_atlas,
            ddgi_transport_source_visibility_atlas,
            ddgiReceiverPosition, normal, ddgiHardVisibilityPosition).irradiance;
    }
    float3 albedo = ddgiVoxelColorByTypeAndHash(
        ddgi_radiance_voxel_palette, result.voxel_type, result.hash_id);
    return albedo * (directIrradiance + indirectIrradiance);
}

[shader("compute")]
[numthreads(DDGI_TRACE_WORKGROUP_SIZE, 1, 1)]
void main(uint3 groupId : SV_GroupID,
          uint3 localInvocationId : SV_GroupThreadID)
{
    uint localProbeIndex = groupId.x;
    if (localProbeIndex >= pc.probe_count) return;

    uint probeIndex = pc.first_probe_index + localProbeIndex;
    DdgiProbeMetadata metadata = getDdgiProbeMetadata(probeIndex);
    bool probeIsValid =
        metadata.state_and_reserved.x == DDGI_PROBE_STATE_VALID &&
        metadata.state_and_reserved.y == pc.terrain_revision;
    float3 nominalProbePosition =
        metadata.nominal_position_and_min_clearance.xyz;
    bool probeNeedsLocalRefresh = pc.local_refresh_enabled.x != 0u &&
        all(nominalProbePosition >= pc.local_refresh_world_min.xyz) &&
        all(nominalProbePosition <= pc.local_refresh_world_max.xyz);
    bool useHistory = pc.has_history != 0u && !probeNeedsLocalRefresh;

    [unroll]
    for (uint rayInThread = 0u; rayInThread < DDGI_RAYS_PER_THREAD;
         ++rayInThread)
    {
        uint rayIndex = localInvocationId.x +
                        rayInThread * DDGI_TRACE_WORKGROUP_SIZE;
        uint outputIndex = localProbeIndex * DDGI_RAYS_PER_PROBE + rayIndex;
        float4 rayRecord = float4(0.0, 0.0, 0.0, pc.far_distance_world);
        recordTraceStat(0u);

        if (!probeIsValid)
        {
            recordTraceStat(6u);
            ddgi_transient_ray_data.data[outputIndex] = rayRecord;
            continue;
        }

        recordTraceStat(1u);
        float3 direction = ddgiEpochDirection(
            rayIndex, DDGI_RAYS_PER_PROBE, pc.epoch_rotation);
        MarchingResult result = marchScene(
            contree_node_data, contree_leaf_data, scene_tex,
            metadata.actual_position_and_clearance.xyz, direction,
            1.0 / direction, localInvocationId.x);
        if (!result.is_hit)
        {
            rayRecord.xyz = getAuthoredSkyRadiance(
                direction, normalize(ddgi_radiance_sun.direction));
            recordTraceStat(2u);
        }
        else
        {
            float hitDistance = max(result.distance, 1.0e-5);
            bool backface = dot(direction, result.normal) > 0.0;
            rayRecord.w = backface ? -hitDistance : hitDistance;
            if (!backface)
            {
                rayRecord.xyz = ddgiTransportHitRadiance(
                    result, localInvocationId.x, useHistory);
            }
            recordTraceStat(backface ? 4u : 3u);
        }

        if (any(isnan(rayRecord)) || any(isinf(rayRecord)))
        {
            recordTraceStat(5u);
        }
        ddgi_transient_ray_data.data[outputIndex] = rayRecord;
    }
}
