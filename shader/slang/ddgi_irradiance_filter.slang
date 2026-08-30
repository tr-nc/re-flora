import ddgi_config;
import ddgi_irradiance_filter_owner;
import ddgi_filter_policy;
import ddgi_octahedral;
import ddgi_sampling;
import ddgi_types;

static const uint DDGI_IRRADIANCE_INTERIOR_SIDE = 8u;
static const uint DDGI_IRRADIANCE_STORED_SIDE = 10u;
static const float DDGI_FOUR = 4.0;

[[vk::binding(24, 0)]]
readonly GLSLShaderStorageBuffer<B_DdgiProbeMetadata, Std430DataLayout>
    ddgi_probe_metadata;

[[vk::binding(25, 0)]]
readonly GLSLShaderStorageBuffer<B_DdgiRayData, Std430DataLayout>
    ddgi_transient_ray_data;

struct DdgiAtlasFilterPushConstants
{
    uint first_probe_index;
    uint probe_count;
    uint tile_columns;
    uint terrain_revision;
    uint destination_is_transport_source;
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
ConstantBuffer<DdgiAtlasFilterPushConstants> pc;

[shader("compute")]
[numthreads(8, 8, 1)]
void main(uint3 groupId : SV_GroupID,
          uint3 localInvocationId : SV_GroupThreadID)
{
    uint localProbeIndex = groupId.x;
    if (localProbeIndex >= pc.probe_count) return;

    uint probeIndex = pc.first_probe_index + localProbeIndex;
    uint2 atlasCoordinate =
        ddgiTileOrigin(probeIndex, pc.tile_columns,
                       DDGI_IRRADIANCE_STORED_SIDE) +
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
    float4 history = float4(0.0);
    if (pc.has_history != 0u)
        history = ddgiOwnerLoadIrradiance(pc.source_slot, atlasCoordinate);
    if (ddgiOwnerTryStoreTerminalIrradiance(
            pc.filter_evidence.x, electedEvidenceLane,
            pc.destination_is_transport_source, atlasCoordinate,
            historyDecision, freshAvailable, history))
    {
        // Geometry recovery owns only its explicit dirty partition. Keeping the other tiles
        // byte-for-byte identical avoids turning a local edit into global temporal noise.
        return;
    }

    float2 encoded =
        ((float2(localInvocationId.xy) + 0.5) /
         float(DDGI_IRRADIANCE_INTERIOR_SIDE)) *
            2.0 -
        1.0;
    float3 normal = octDecode(encoded);
    float3 irradiance = float3(0.0);
    [loop]
    for (uint rayIndex = 0u; rayIndex < DDGI_RAYS_PER_PROBE; ++rayIndex)
    {
        float3 direction = ddgiEpochDirection(
            rayIndex, DDGI_RAYS_PER_PROBE, pc.epoch_rotation);
        float cosineWeight = max(0.0, dot(normal, direction));
        float3 radiance =
            ddgi_transient_ray_data
                .data[localProbeIndex * DDGI_RAYS_PER_PROBE + rayIndex]
                .xyz;
        irradiance += radiance * cosineWeight;
    }
    // Uniform-sphere Monte Carlo estimate of diffuse irradiance divided by pi:
    // (1 / pi) * (4 * pi / N) * sum(L * max(0, n dot w)).
    irradiance *= DDGI_FOUR / float(DDGI_RAYS_PER_PROBE);
    ddgiOwnerStoreFreshIrradiance(
        pc.filter_evidence.x, electedEvidenceLane,
        pc.destination_is_transport_source, atlasCoordinate,
        historyDecision, float4(irradiance, 1.0), history);
}
