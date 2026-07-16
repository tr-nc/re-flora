import ray;
import tracer_types;

static const uint CLOUD_GROUP_SIZE = 8u;

struct CloudTemporalPushConstants
{
    uint reset_history;
};

[[vk::push_constant]]
ConstantBuffer<CloudTemporalPushConstants> cloud_temporal_push;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(4, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(5, 0)]]
ConstantBuffer<U_CameraInfoPrevFrame> camera_info_prev_frame;
[[vk::binding(15, 0)]] Sampler2D cloud_raw_tex;
[[vk::binding(16, 0)]] Sampler2D cloud_history_tex;

[[vk::binding(17, 0)]]
[[vk::image_format("rgba16f")]]
WTexture2D<float4> cloud_output_tex;

float temporalCloudLayerBottom()
{
    return min(gui_input.cloud_bottom_height,
               gui_input.cloud_top_height - 1.0);
}

float temporalCloudLayerTop()
{
    return max(gui_input.cloud_top_height,
               gui_input.cloud_bottom_height + 1.0);
}

bool temporalCloudHistoryUv(float2 screenUv, out float2 historyUv)
{
    historyUv = screenUv;
    Ray ray = generateCameraRay(screenUv, camera_info.view_proj_mat_inv);
    if (abs(ray.direction.y) < 0.018)
    {
        return false;
    }

    float bottom = temporalCloudLayerBottom();
    float top = temporalCloudLayerTop();
    float3 cameraPosition = camera_info.pos.xyz;
    float bottomDistance = (bottom - cameraPosition.y) / ray.direction.y;
    float topDistance = (top - cameraPosition.y) / ray.direction.y;
    float startDistance = max(min(bottomDistance, topDistance), 0.0);
    float endDistance = max(bottomDistance, topDistance);
    if (endDistance <= startDistance)
    {
        return false;
    }

    endDistance = min(endDistance, max(gui_input.cloud_max_distance, 1.0));
    if (endDistance <= startDistance)
    {
        return false;
    }

    float distance = lerp(startDistance, endDistance, 0.42);
    float3 worldPosition = cameraPosition + ray.direction * distance;
    float4 previousClip = mul(
        camera_info_prev_frame.view_proj_mat, float4(worldPosition, 1.0));
    if (previousClip.w <= 1.0e-5)
    {
        return false;
    }

    float3 previousNdc = previousClip.xyz / previousClip.w;
    historyUv = previousNdc.xy * 0.5 + 0.5;
    return all(historyUv >= float2(0.0)) &&
           all(historyUv <= float2(1.0));
}

float4 currentNeighborhoodMinimum(int2 coordinate, int2 size)
{
    float4 value = cloud_raw_tex.Load(int3(coordinate, 0));
    for (int y = -1; y <= 1; ++y)
    {
        for (int x = -1; x <= 1; ++x)
        {
            int2 sampleCoordinate = clamp(
                coordinate + int2(x, y), int2(0), size - int2(1));
            value = min(value,
                cloud_raw_tex.Load(int3(sampleCoordinate, 0)));
        }
    }
    return value;
}

float4 currentNeighborhoodMaximum(int2 coordinate, int2 size)
{
    float4 value = cloud_raw_tex.Load(int3(coordinate, 0));
    for (int y = -1; y <= 1; ++y)
    {
        for (int x = -1; x <= 1; ++x)
        {
            int2 sampleCoordinate = clamp(
                coordinate + int2(x, y), int2(0), size - int2(1));
            value = max(value,
                cloud_raw_tex.Load(int3(sampleCoordinate, 0)));
        }
    }
    return value;
}

[shader("compute")]
[numthreads(CLOUD_GROUP_SIZE, CLOUD_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    cloud_output_tex.GetDimensions(width, height);
    int2 size = int2(width, height);
    int2 coordinate = int2(dispatchThreadId.xy);
    if (any(coordinate >= size))
    {
        return;
    }

    float4 current = cloud_raw_tex.Load(int3(coordinate, 0));
    if (cloud_temporal_push.reset_history != 0u)
    {
        cloud_output_tex.Store(coordinate, current);
        return;
    }

    float2 screenUv = (float2(coordinate) + float2(0.5)) /
                      float2(size);
    float2 historyUv;
    if (!temporalCloudHistoryUv(screenUv, historyUv))
    {
        cloud_output_tex.Store(coordinate, current);
        return;
    }

    float4 history = cloud_history_tex.SampleLevel(historyUv, 0.0);
    float4 neighborhoodMinimum = currentNeighborhoodMinimum(coordinate, size);
    float4 neighborhoodMaximum = currentNeighborhoodMaximum(coordinate, size);
    float4 neighborhoodRange = neighborhoodMaximum - neighborhoodMinimum;
    float4 margin = max(
        neighborhoodRange * 0.35, float4(0.04, 0.04, 0.04, 0.025));
    history = clamp(
        history, neighborhoodMinimum - margin, neighborhoodMaximum + margin);

    float alphaDelta = abs(history.a - current.a);
    float alphaChange = smoothstep(0.06, 0.32, alphaDelta);
    float temporalAlpha = clamp(gui_input.cloud_temporal_alpha, 0.01, 1.0);
    float currentWeight = lerp(
        temporalAlpha, min(temporalAlpha * 4.0, 1.0), alphaChange);
    if (neighborhoodMaximum.a < 0.015)
    {
        currentWeight = 1.0;
    }

    float4 resolved = lerp(history, current, currentWeight);
    resolved.rgb = max(resolved.rgb, float3(0.0));
    resolved.a = clamp(resolved.a, 0.0, 1.0);
    cloud_output_tex.Store(coordinate, resolved);
}
