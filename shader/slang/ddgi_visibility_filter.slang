import ddgi_octahedral;
import ddgi_types;

static const uint DDGI_RAYS_PER_PROBE = 256u;
static const uint DDGI_VISIBILITY_INTERIOR_SIDE = 16u;
static const uint DDGI_VISIBILITY_STORED_SIDE = 18u;
static const float DDGI_VISIBILITY_FILTER_EXPONENT = 50.0;

[[vk::binding(24, 0)]]
readonly GLSLShaderStorageBuffer<B_DdgiProbeMetadata, Std430DataLayout>
    ddgi_probe_metadata;

[[vk::binding(25, 0)]]
readonly GLSLShaderStorageBuffer<B_DdgiRayData, Std430DataLayout>
    ddgi_transient_ray_data;

[[vk::binding(28, 0)]]
RWTexture2D<float2> ddgi_visibility_atlas;

struct DdgiVisibilityFilterPushConstants
{
    uint first_probe_index;
    uint probe_count;
    uint tile_columns;
    uint terrain_revision;
    float3 spacing_world;
    float padding;
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
    if (metadata.state_and_reserved.x != DDGI_PROBE_STATE_VALID ||
        metadata.state_and_reserved.y != pc.terrain_revision)
    {
        ddgi_visibility_atlas[atlasCoordinate] = float2(0.0);
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
    float weightedDistance = 0.0;
    float weightedSquaredDistance = 0.0;
    float accumulatedWeight = 0.0;

    [loop]
    for (uint rayIndex = 0u; rayIndex < DDGI_RAYS_PER_PROBE; ++rayIndex)
    {
        float3 rayDirection =
            ddgiFibonacciDirection(rayIndex, DDGI_RAYS_PER_PROBE);
        float cosineWeight = max(0.0, dot(filterDirection, rayDirection));
        cosineWeight = pow(cosineWeight, DDGI_VISIBILITY_FILTER_EXPONENT);
        float signedDistance =
            ddgi_transient_ray_data
                .data[localProbeIndex * DDGI_RAYS_PER_PROBE + rayIndex]
                .w;
        float hitDistance = signedDistance < 0.0
            ? abs(signedDistance) * 0.2
            : signedDistance;
        // Rohacek's cage-support correction: a distant directional sample
        // cannot claim support beyond this probe's relocated nominal cage.
        hitDistance = min(hitDistance, supportDistance);
        weightedDistance += hitDistance * cosineWeight;
        weightedSquaredDistance +=
            hitDistance * hitDistance * cosineWeight;
        accumulatedWeight += cosineWeight;
    }

    float inverseWeight = rcp(max(accumulatedWeight, 1.0e-8));
    ddgi_visibility_atlas[atlasCoordinate] =
        float2(weightedDistance, weightedSquaredDistance) * inverseWeight;
}
