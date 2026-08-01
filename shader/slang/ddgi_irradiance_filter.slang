import ddgi_octahedral;
import ddgi_types;

static const uint DDGI_RAYS_PER_PROBE = 256u;
static const uint DDGI_IRRADIANCE_INTERIOR_SIDE = 8u;
static const uint DDGI_IRRADIANCE_STORED_SIDE = 10u;
static const float DDGI_FOUR = 4.0;

[[vk::binding(24, 0)]]
readonly GLSLShaderStorageBuffer<B_DdgiProbeMetadata, Std430DataLayout>
    ddgi_probe_metadata;

[[vk::binding(25, 0)]]
readonly GLSLShaderStorageBuffer<B_DdgiRayData, Std430DataLayout>
    ddgi_transient_ray_data;

[[vk::binding(27, 0)]]
RWTexture2D<float4> ddgi_irradiance_atlas;

struct DdgiAtlasFilterPushConstants
{
    uint first_probe_index;
    uint probe_count;
    uint tile_columns;
    uint terrain_revision;
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
    if (metadata.state_and_reserved.x != DDGI_PROBE_STATE_VALID ||
        metadata.state_and_reserved.y != pc.terrain_revision)
    {
        ddgi_irradiance_atlas[atlasCoordinate] = float4(0.0);
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
        float3 direction =
            ddgiFibonacciDirection(rayIndex, DDGI_RAYS_PER_PROBE);
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
    ddgi_irradiance_atlas[atlasCoordinate] = float4(irradiance, 1.0);
}
