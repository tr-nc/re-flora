import ddgi_config;
import ddgi_visibility_filter_owner;
import ddgi_filter_policy;
import ddgi_octahedral;
import ddgi_sampling;
import ddgi_types;

static const uint DDGI_VISIBILITY_INTERIOR_SIDE = 16u;
static const uint DDGI_VISIBILITY_STORED_SIDE = 18u;
static const float DDGI_VISIBILITY_FILTER_EXPONENT = 50.0;

[[vk::binding(24, 0)]]
readonly GLSLShaderStorageBuffer<B_DdgiProbeMetadata, Std430DataLayout>
    ddgi_probe_metadata;

[[vk::binding(25, 0)]]
readonly GLSLShaderStorageBuffer<B_DdgiRayData, Std430DataLayout>
    ddgi_transient_ray_data;

struct DdgiVisibilityFilterPushConstants
{
    uint first_probe_index;
    uint probe_count;
    uint tile_columns;
    uint terrain_revision;
    float3 spacing_world;
    float far_distance_world;
    uint destination_slot;
    uint source_slot;
    uint has_history;
    float history_retention;
    float4 epoch_rotation;
    uint4 local_refresh_enabled;
    float4 local_refresh_world_min;
    float4 local_refresh_world_max;
    uint4 filter_evidence;
};

[[vk::push_constant]]
ConstantBuffer<DdgiVisibilityFilterPushConstants> pc;

[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 groupId : SV_GroupID,
          uint3 localInvocationId : SV_GroupThreadID)
{
    uint localProbeIndex = groupId.x;
    if (localProbeIndex >= pc.probe_count) return;

    uint probeIndex = pc.first_probe_index + localProbeIndex;
    uint2 atlasCoordinate =
        ddgiTileOrigin(probeIndex, pc.tile_columns,
                       DDGI_VISIBILITY_STORED_SIDE) +
        localInvocationId.xy + uint2(1u);
    DdgiProbeMetadata metadata = ddgi_probe_metadata.data[probeIndex];
    bool localRecoveryProbe = ddgiFilterProbeNeedsLocalRefresh(
        metadata.nominal_position_and_min_clearance.xyz,
        pc.local_refresh_enabled.x, pc.local_refresh_world_min.xyz,
        pc.local_refresh_world_max.xyz);
    DdgiFilterHistoryDecision historyDecision = ddgiFilterHistoryDecision(
        pc.has_history, pc.local_refresh_enabled.x, localRecoveryProbe,
        pc.local_refresh_enabled.y, pc.history_retention);
    bool electedEvidenceLane = all(localInvocationId.xy == uint2(0u));
    bool freshAvailable =
        metadata.state_and_reserved.x == DDGI_PROBE_STATE_VALID &&
        metadata.state_and_reserved.y == pc.terrain_revision;
    float2 history = float2(0.0);
    if (pc.has_history != 0u)
        history = ddgiOwnerLoadVisibility(pc.source_slot, atlasCoordinate);
    if (ddgiOwnerTryStoreTerminalVisibility(
            pc.filter_evidence.x, electedEvidenceLane,
            pc.destination_slot, atlasCoordinate,
            historyDecision, freshAvailable, history))
    {
        return;
    }

    float2 encoded =
        ((float2(localInvocationId.xy) + 0.5) /
         float(DDGI_VISIBILITY_INTERIOR_SIDE)) *
            2.0 -
        1.0;
    float3 filterDirection = octDecode(encoded);
    float relocationDistance = distance(
        metadata.actual_position_and_clearance.xyz,
        metadata.nominal_position_and_min_clearance.xyz);
    float supportDistance = length(pc.spacing_world) + relocationDistance;
    DdgiOwnedVisibilityAccumulator visibilityAccumulator =
        ddgiOwnerBeginVisibility();

    [loop]
    for (uint rayIndex = 0u; rayIndex < DDGI_RAYS_PER_PROBE; ++rayIndex)
    {
        float3 rayDirection = ddgiEpochDirection(
            rayIndex, DDGI_RAYS_PER_PROBE, pc.epoch_rotation);
        float cosineWeight = max(0.0, dot(filterDirection, rayDirection));
        cosineWeight = pow(cosineWeight, DDGI_VISIBILITY_FILTER_EXPONENT);
        float signedDistance =
            ddgi_transient_ray_data
                .data[localProbeIndex * DDGI_RAYS_PER_PROBE + rayIndex]
                .w;
        DdgiFilterVisibilityDecision visibilityDecision =
            ddgiFilterVisibilityDecision(
            signedDistance, supportDistance, pc.far_distance_world);
        // Rohacek's cage-support correction: a distant directional sample
        // cannot claim support beyond this probe's relocated nominal cage.
        // Preserve sky misses as unoccluded support; reject only distant
        // geometry hits that would pull a local depth lobe outward.
        ddgiOwnerAccumulateVisibilitySample(
            pc.filter_evidence.x, electedEvidenceLane,
            visibilityDecision, cosineWeight, visibilityAccumulator);
    }

    float2 fresh = ddgiOwnerFinishVisibility(
        visibilityAccumulator, supportDistance);
    ddgiOwnerStoreFreshVisibility(
        pc.filter_evidence.x, electedEvidenceLane,
        pc.destination_slot, atlasCoordinate,
        historyDecision, fresh, history);
}
