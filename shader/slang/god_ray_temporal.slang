import projection;
import tracer_types;

static const uint GOD_RAY_GROUP_SIZE = 8u;

struct GodRayTemporalPushConstants
{
    uint reset_history;
};

[[vk::push_constant]]
ConstantBuffer<GodRayTemporalPushConstants> god_ray_temporal_push;

[[vk::binding(0, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(1, 0)]]
ConstantBuffer<U_CameraInfoPrevFrame> camera_info_prev_frame;
[[vk::binding(2, 0)]] Sampler2D gfx_depth_tex;

[[vk::binding(3, 0)]]
[[vk::image_format("r32f")]]
readonly RWTexture2D<float> compute_depth_tex;

[[vk::binding(4, 0)]] Sampler2D god_ray_raw_tex;
[[vk::binding(5, 0)]] Sampler2D god_ray_history_tex;

[[vk::binding(6, 0)]]
[[vk::image_format("r32f")]]
WTexture2D<float> god_ray_output_tex;

float currentMinimum3x3(int2 coordinate, int2 size)
{
    float value = god_ray_raw_tex.Load(int3(coordinate, 0)).r;
    for (int y = -1; y <= 1; ++y)
    {
        for (int x = -1; x <= 1; ++x)
        {
            int2 sampleCoordinate = clamp(
                coordinate + int2(x, y), int2(0), size - int2(1));
            value = min(value,
                god_ray_raw_tex.Load(int3(sampleCoordinate, 0)).r);
        }
    }
    return value;
}

float currentMaximum3x3(int2 coordinate, int2 size)
{
    float value = god_ray_raw_tex.Load(int3(coordinate, 0)).r;
    for (int y = -1; y <= 1; ++y)
    {
        for (int x = -1; x <= 1; ++x)
        {
            int2 sampleCoordinate = clamp(
                coordinate + int2(x, y), int2(0), size - int2(1));
            value = max(value,
                god_ray_raw_tex.Load(int3(sampleCoordinate, 0)).r);
        }
    }
    return value;
}

bool historyUvForCurrentSurface(float2 screenUv, out float2 historyUv)
{
    uint depthWidth;
    uint depthHeight;
    gfx_depth_tex.GetDimensions(depthWidth, depthHeight);
    int2 depthCoordinate = int2(clamp(
        screenUv * float2(depthWidth, depthHeight), float2(0.0),
        float2(int2(depthWidth, depthHeight) - int2(1))));
    float graphicsDepth = gfx_depth_tex.Load(int3(depthCoordinate, 0)).r;
    float computeDepth = compute_depth_tex[depthCoordinate];
    float depth = min(graphicsDepth, computeDepth);
    float3 worldPosition = projectScreenUvToWorld(
        screenUv, depth, camera_info.view_proj_mat_inv);
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

[shader("compute")]
[numthreads(GOD_RAY_GROUP_SIZE, GOD_RAY_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    god_ray_output_tex.GetDimensions(width, height);
    int2 size = int2(width, height);
    int2 coordinate = int2(dispatchThreadId.xy);
    if (any(coordinate >= size))
    {
        return;
    }

    float current = max(god_ray_raw_tex.Load(int3(coordinate, 0)).r, 0.0);
    if (god_ray_temporal_push.reset_history != 0u)
    {
        god_ray_output_tex.Store(coordinate, current);
        return;
    }

    float2 screenUv = (float2(coordinate) + float2(0.5)) /
                      float2(size);
    float2 historyUv;
    if (!historyUvForCurrentSurface(screenUv, historyUv))
    {
        god_ray_output_tex.Store(coordinate, current);
        return;
    }

    float history = max(
        god_ray_history_tex.SampleLevel(historyUv, 0.0).r, 0.0);
    float neighborhoodMinimum = currentMinimum3x3(coordinate, size);
    float neighborhoodMaximum = currentMaximum3x3(coordinate, size);
    float neighborhoodRange = neighborhoodMaximum - neighborhoodMinimum;
    float margin = max(neighborhoodRange * 0.65, 0.025);
    history = clamp(
        history, neighborhoodMinimum - margin, neighborhoodMaximum + margin);

    float relativeDelta = abs(history - current) / max(abs(current), 0.10);
    float change = smoothstep(0.35, 1.50, relativeDelta);
    float currentWeight = lerp(0.10, 0.65, change);
    float resolved = lerp(history, current, currentWeight);
    god_ray_output_tex.Store(coordinate, max(resolved, 0.0));
}
