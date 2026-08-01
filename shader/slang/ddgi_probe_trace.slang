import contree_node;
import ddgi_types;
import marching_result;
import scene_marching;
import skylight;

static const uint DDGI_TRACE_WORKGROUP_SIZE = 64u;
static const uint DDGI_RAYS_PER_PROBE = 256u;
static const uint DDGI_RAYS_PER_THREAD =
    DDGI_RAYS_PER_PROBE / DDGI_TRACE_WORKGROUP_SIZE;
static const float DDGI_GOLDEN_ANGLE = 2.39996322972865332;
// DDGI stores diffuse irradiance divided by pi so diffuse consumers apply
// albedo exactly once. Preserve the existing authored sky presentation by
// treating the current sky function as an engine-authored value that needs
// this radiance calibration before the normalized filter integrates it.
static const float DDGI_AUTHORED_SKY_RADIANCE_CALIBRATION =
    3.14159265358979323846;

[[vk::binding(7, 0)]]
StructuredBuffer<ContreeNode> contree_node_data;

[[vk::binding(8, 0)]]
StructuredBuffer<uint> contree_leaf_data;

[[vk::binding(9, 0)]] [[vk::image_format("rg32ui")]]
readonly RWTexture3D<uint2> scene_tex;

[[vk::binding(24, 0)]]
readonly GLSLShaderStorageBuffer<B_DdgiProbeMetadata, Std430DataLayout>
    ddgi_probe_metadata;

[[vk::binding(25, 0)]]
GLSLShaderStorageBuffer<B_DdgiRayData, Std430DataLayout>
    ddgi_transient_ray_data;

[[vk::binding(26, 0)]]
GLSLShaderStorageBuffer<B_DdgiTraceStats, Std430DataLayout>
    ddgi_trace_stats;

// These builder-only bindings shape the immutable source/destination transport seam. S0 does not
// sample the source atlas; S1 will consume it through sampleDdgiTransportSource in ddgi_query.
[[vk::binding(29, 0)]]
Sampler2D ddgi_transport_source_irradiance_atlas;

[[vk::binding(30, 0)]]
ConstantBuffer<U_DdgiRadianceSun> ddgi_radiance_sun;

[[vk::binding(31, 0)]]
ConstantBuffer<U_DdgiRadianceVoxelPalette> ddgi_radiance_voxel_palette;

[[vk::binding(32, 0)]]
ConstantBuffer<U_DdgiTransportQueryInfo> ddgi_transport_query_info;

struct DdgiProbeTracePushConstants
{
    uint first_probe_index;
    uint probe_count;
    uint terrain_revision;
    uint padding;
    float far_distance_world;
    float3 padding_float;
};

[[vk::push_constant]]
ConstantBuffer<DdgiProbeTracePushConstants> pc;

float3 fibonacciDirection(uint rayIndex)
{
    float y = 1.0 - (2.0 * float(rayIndex) + 1.0) /
                        float(DDGI_RAYS_PER_PROBE);
    float radius = sqrt(max(0.0, 1.0 - y * y));
    float azimuth = DDGI_GOLDEN_ANGLE * float(rayIndex);
    return float3(radius * cos(azimuth), y, radius * sin(azimuth));
}

void recordTraceStat(uint index)
{
    InterlockedAdd(ddgi_trace_stats.data[index], 1u);
}

[shader("compute")]
[numthreads(DDGI_TRACE_WORKGROUP_SIZE, 1, 1)]
void main(uint3 groupId : SV_GroupID,
          uint3 localInvocationId : SV_GroupThreadID)
{
    uint localProbeIndex = groupId.x;
    if (localProbeIndex >= pc.probe_count) return;

    uint probeIndex = pc.first_probe_index + localProbeIndex;
    DdgiProbeMetadata metadata = ddgi_probe_metadata.data[probeIndex];
    bool probeIsValid =
        metadata.state_and_reserved.x == DDGI_PROBE_STATE_VALID &&
        metadata.state_and_reserved.y == pc.terrain_revision;

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
        float3 direction = fibonacciDirection(rayIndex);
        MarchingResult result = marchScene(
            contree_node_data, contree_leaf_data, scene_tex,
            metadata.actual_position_and_clearance.xyz, direction,
            1.0 / direction, localInvocationId.x);
        if (!result.is_hit)
        {
            rayRecord.xyz =
                getSkyColor(direction, normalize(ddgi_radiance_sun.direction)) *
                DDGI_AUTHORED_SKY_RADIANCE_CALIBRATION;
            recordTraceStat(2u);
        }
        else
        {
            float hitDistance = max(result.distance, 1.0e-5);
            bool backface = dot(direction, result.normal) > 0.0;
            rayRecord.w = backface ? -hitDistance : hitDistance;
            recordTraceStat(backface ? 4u : 3u);
        }

        if (any(isnan(rayRecord)) || any(isinf(rayRecord)))
        {
            recordTraceStat(5u);
        }
        ddgi_transient_ray_data.data[outputIndex] = rayRecord;
    }
}
