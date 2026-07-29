import tracer_types;

struct EnvironmentProbeSummary
{
    float4 original_position_visibility;
    float4 sample_position_confidence;
    uint4 revisions_state;
    uint4 update_info;
};

struct EnvironmentProbeCoefficients
{
    float4 coefficients[9];
};

struct B_EnvironmentProbeSummaries
{
    EnvironmentProbeSummary data[];
};

struct B_EnvironmentProbeCoefficients
{
    EnvironmentProbeCoefficients data[];
};

[[vk::binding(0, 1)]]
readonly GLSLShaderStorageBuffer<B_EnvironmentProbeSummaries, Std430DataLayout>
    environment_probe_summaries;

[[vk::binding(1, 1)]]
readonly GLSLShaderStorageBuffer<B_EnvironmentProbeCoefficients, Std430DataLayout>
    environment_probe_coefficients;

[[vk::binding(3, 0)]]
ConstantBuffer<U_CameraInfo> camera_info;

struct EnvironmentProbeVisualizationPushConstants
{
    uint display_mode;
    uint filter;
    uint instance_stride;
    uint always_visible;
    float marker_size_world;
    float camera_radius_world;
    float inverse_age_range_frames;
    float padding;
};

[[vk::push_constant]]
ConstantBuffer<EnvironmentProbeVisualizationPushConstants> pc;

struct VertexInput
{
    [[vk::location(0)]] float2 local_position : POSITION0;
    uint instance_index : SV_VulkanInstanceID;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] nointerpolation float4 color_alpha : COLOR0;
};

static const uint PROBE_STATE_INACTIVE = 0u;
static const uint PROBE_STATE_INSIDE_SOLID = 1u;
static const uint PROBE_STATE_RELOCATION_PENDING = 2u;
static const uint PROBE_STATE_VALID = 3u;
static const uint PROBE_STATE_DIRTY = 4u;
static const uint PROBE_STATE_UPDATING = 5u;
static const uint PROBE_STATE_RELOCATION_FAILED = 6u;

float environmentProbeShBasis(uint index, float3 direction)
{
    float x = direction.x;
    float y = direction.y;
    float z = direction.z;
    switch (index)
    {
        case 0u: return 0.2820948;
        case 1u: return 0.48860252 * z;
        case 2u: return 0.48860252 * y;
        case 3u: return 0.48860252 * x;
        case 4u: return 1.0925485 * x * z;
        case 5u: return 1.0925485 * z * y;
        case 6u: return 0.31539157 * (3.0 * y * y - 1.0);
        case 7u: return 1.0925485 * x * y;
        default: return 0.54627424 * (x * x - z * z);
    }
}

float3 evaluateProbeUpIrradiance(EnvironmentProbeCoefficients probe)
{
    float3 irradiance = float3(0.0);
    [unroll]
    for (uint coefficientIndex = 0u; coefficientIndex < 9u; ++coefficientIndex)
    {
        irradiance += probe.coefficients[coefficientIndex].xyz *
                      environmentProbeShBasis(
                          coefficientIndex, float3(0.0, 1.0, 0.0));
    }
    irradiance = max(irradiance, float3(0.0));
    return irradiance / (float3(1.0) + irradiance);
}

float3 heatMap(float value)
{
    float t = saturate(value);
    float3 cold = float3(0.05, 0.15, 0.95);
    float3 middle = float3(0.05, 0.95, 0.35);
    float3 hot = float3(1.0, 0.15, 0.03);
    return t < 0.5
        ? lerp(cold, middle, t * 2.0)
        : lerp(middle, hot, (t - 0.5) * 2.0);
}

float3 stateColor(uint state)
{
    switch (state)
    {
        case PROBE_STATE_INACTIVE: return float3(0.30, 0.32, 0.36);
        case PROBE_STATE_INSIDE_SOLID: return float3(0.95, 0.08, 0.06);
        case PROBE_STATE_RELOCATION_PENDING: return float3(1.0, 0.46, 0.03);
        case PROBE_STATE_VALID: return float3(0.08, 0.95, 0.20);
        case PROBE_STATE_DIRTY: return float3(1.0, 0.82, 0.05);
        case PROBE_STATE_UPDATING: return float3(0.05, 0.86, 1.0);
        case PROBE_STATE_RELOCATION_FAILED: return float3(0.95, 0.05, 0.90);
        default: return float3(1.0, 1.0, 1.0);
    }
}

bool statePassesFilter(uint state)
{
    if (pc.filter == 0u) return true;
    bool valid = state == PROBE_STATE_VALID ||
                 state == PROBE_STATE_DIRTY ||
                 state == PROBE_STATE_UPDATING;
    if (pc.filter == 1u) return valid;
    if (pc.filter == 2u) return !valid;
    return state == PROBE_STATE_DIRTY || state == PROBE_STATE_UPDATING;
}

[shader("vertex")]
VertexOutput main(VertexInput input)
{
    VertexOutput output;
    output.position = float4(2.0, 2.0, 2.0, 1.0);
    output.color_alpha = float4(0.0);

    bool relocationMode = pc.display_mode == 4u;
    uint logicalInstance = relocationMode ? input.instance_index / 2u
                                          : input.instance_index;
    uint probeIndex = logicalInstance * max(pc.instance_stride, 1u);
    EnvironmentProbeSummary summary =
        environment_probe_summaries.data[probeIndex];
    uint state = summary.revisions_state.z;
    if (!statePassesFilter(state)) return output;

    bool originalRelocationMarker =
        relocationMode && ((input.instance_index & 1u) == 0u);
    float3 originalPosition = summary.original_position_visibility.xyz;
    float3 samplePosition = summary.sample_position_confidence.xyz;
    float3 worldPosition = originalRelocationMarker
        ? originalPosition
        : samplePosition;

    if (pc.camera_radius_world > 0.0 &&
        distance(worldPosition, camera_info.pos.xyz) > pc.camera_radius_world)
    {
        return output;
    }

    float3 color = stateColor(state);
    if (pc.display_mode == 1u)
    {
        color = heatMap(summary.original_position_visibility.w);
    }
    else if (pc.display_mode == 2u)
    {
        color = evaluateProbeUpIrradiance(
            environment_probe_coefficients.data[probeIndex]);
    }
    else if (pc.display_mode == 3u)
    {
        color = heatMap(float(summary.update_info.y) *
                        pc.inverse_age_range_frames);
    }
    else if (relocationMode)
    {
        float relocationDistance = distance(originalPosition, samplePosition);
        color = originalRelocationMarker
            ? float3(0.92, 0.92, 0.92)
            : heatMap(saturate(relocationDistance * 16.0));
    }

    float markerScale = pc.marker_size_world *
                        (originalRelocationMarker ? 0.65 : 1.0);
    float3 cameraRight =
        mul(camera_info.view_mat_inv, float4(1.0, 0.0, 0.0, 0.0)).xyz;
    float3 cameraUp =
        mul(camera_info.view_mat_inv, float4(0.0, 1.0, 0.0, 0.0)).xyz;
    worldPosition +=
        (cameraRight * input.local_position.x +
         cameraUp * input.local_position.y) * markerScale;

    float4 clipPosition =
        mul(camera_info.view_proj_mat, float4(worldPosition, 1.0));
    if (clipPosition.w <= 0.0) return output;
    if (pc.always_visible != 0u) clipPosition.z = 0.0;

    output.position = clipPosition;
    output.color_alpha = float4(color, 0.88);
    return output;
}
