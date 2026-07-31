import ddgi_query;
import ddgi_types;
import tracer_types;

[[vk::binding(2, 0)]]
ConstantBuffer<U_ShadingInfo> shading_info;

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
    float2 padding;
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
        case DDGI_PROBE_STATE_INACTIVE: return float3(0.30, 0.32, 0.36);
        case DDGI_PROBE_STATE_VALID: return float3(0.08, 0.95, 0.20);
        case DDGI_PROBE_STATE_RELOCATION_FAILED: return float3(0.95, 0.05, 0.90);
        default: return float3(1.0, 1.0, 1.0);
    }
}

bool statePassesFilter(uint state)
{
    if (pc.filter == 0u) return true;
    bool valid = state == DDGI_PROBE_STATE_VALID;
    return pc.filter == 1u ? valid : !valid;
}

[shader("vertex")]
VertexOutput main(VertexInput input)
{
    VertexOutput output;
    output.position = float4(2.0, 2.0, 2.0, 1.0);
    output.color_alpha = float4(0.0);

    bool relocationMode = pc.display_mode == 2u;
    uint logicalInstance = relocationMode ? input.instance_index / 2u
                                          : input.instance_index;
    uint probeIndex = logicalInstance * max(pc.instance_stride, 1u);
    DdgiProbeMetadata metadata = getDdgiProbeMetadata(probeIndex);
    uint state = metadata.state_and_reserved.x;
    if (!statePassesFilter(state)) return output;

    bool nominalMarker = relocationMode && ((input.instance_index & 1u) == 0u);
    float3 nominalPosition = metadata.nominal_position_and_min_clearance.xyz;
    float3 actualPosition = metadata.actual_position_and_clearance.xyz;
    float3 worldPosition = nominalMarker ? nominalPosition : actualPosition;

    if (pc.camera_radius_world > 0.0 &&
        distance(worldPosition, camera_info.pos.xyz) > pc.camera_radius_world)
    {
        return output;
    }

    float3 color = stateColor(state);
    if (pc.display_mode == 1u)
    {
        float3 irradiance = sampleDdgiProbeIrradiance(
            probeIndex, float3(0.0, 1.0, 0.0), shading_info);
        color = irradiance / (float3(1.0) + irradiance);
    }
    else if (relocationMode)
    {
        float relocationDistance = distance(nominalPosition, actualPosition);
        color = nominalMarker
            ? float3(0.92, 0.92, 0.92)
            : heatMap(saturate(relocationDistance * 16.0));
    }

    float markerScale = pc.marker_size_world * (nominalMarker ? 0.65 : 1.0);
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
