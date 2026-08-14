import tracer_types;

static const uint LENS_FLARE_GROUP_SIZE = 8u;

struct LensFlareTemporalPushConstants
{
    uint reset_history;
};

[[vk::push_constant]]
ConstantBuffer<LensFlareTemporalPushConstants> lens_flare_temporal_push;

[[vk::binding(0, 0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(1, 0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(2, 0)]]
ConstantBuffer<U_CameraInfoPrevFrame> camera_info_prev_frame;
[[vk::binding(3, 0)]] Sampler2D lens_flare_raw_tex;
[[vk::binding(4, 0)]] Sampler2D lens_flare_history_tex;

[[vk::binding(5, 0)]]
[[vk::image_format("r11f_g11f_b10f")]]
WTexture2D<float4> lens_flare_output_tex;

float3 currentMinimum3x3(int2 coordinate, int2 size)
{
    float3 value = lens_flare_raw_tex.Load(int3(coordinate, 0)).rgb;
    for (int y = -1; y <= 1; ++y)
    {
        for (int x = -1; x <= 1; ++x)
        {
            int2 sampleCoordinate = clamp(
                coordinate + int2(x, y), int2(0), size - int2(1));
            value = min(value,
                lens_flare_raw_tex.Load(int3(sampleCoordinate, 0)).rgb);
        }
    }
    return value;
}

float3 currentMaximum3x3(int2 coordinate, int2 size)
{
    float3 value = lens_flare_raw_tex.Load(int3(coordinate, 0)).rgb;
    for (int y = -1; y <= 1; ++y)
    {
        for (int x = -1; x <= 1; ++x)
        {
            int2 sampleCoordinate = clamp(
                coordinate + int2(x, y), int2(0), size - int2(1));
            value = max(value,
                lens_flare_raw_tex.Load(int3(sampleCoordinate, 0)).rgb);
        }
    }
    return value;
}

bool lensFlareHistoryUv(float2 screenUv, out float2 historyUv)
{
    float3 sunWorldPosition = camera_info.pos.xyz +
                              sun_info.sun_dir * 10000.0;
    float4 currentClip = mul(
        camera_info.view_proj_mat, float4(sunWorldPosition, 1.0));
    float4 previousClip = mul(
        camera_info_prev_frame.view_proj_mat, float4(sunWorldPosition, 1.0));
    if (currentClip.w <= 1.0e-5 || previousClip.w <= 1.0e-5)
    {
        return false;
    }

    float2 currentSunUv = currentClip.xy / currentClip.w * 0.5 + 0.5;
    float2 previousSunUv = previousClip.xy / previousClip.w * 0.5 + 0.5;
    historyUv = screenUv + previousSunUv - currentSunUv;
    return all(historyUv >= float2(0.0)) &&
           all(historyUv <= float2(1.0));
}

[shader("compute")]
[numthreads(LENS_FLARE_GROUP_SIZE, LENS_FLARE_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    lens_flare_output_tex.GetDimensions(width, height);
    int2 size = int2(width, height);
    int2 coordinate = int2(dispatchThreadId.xy);
    if (any(coordinate >= size))
    {
        return;
    }

    float3 current = max(
        lens_flare_raw_tex.Load(int3(coordinate, 0)).rgb, float3(0.0));
    if (lens_flare_temporal_push.reset_history != 0u ||
        max(current.r, max(current.g, current.b)) <= 1.0e-6)
    {
        lens_flare_output_tex.Store(coordinate, float4(current, 1.0));
        return;
    }

    float2 screenUv = (float2(coordinate) + float2(0.5)) /
                      float2(size);
    float2 historyUv;
    if (!lensFlareHistoryUv(screenUv, historyUv))
    {
        lens_flare_output_tex.Store(coordinate, float4(current, 1.0));
        return;
    }

    float3 history = max(
        lens_flare_history_tex.SampleLevel(historyUv, 0.0).rgb,
        float3(0.0));
    float3 neighborhoodMinimum = currentMinimum3x3(coordinate, size);
    float3 neighborhoodMaximum = currentMaximum3x3(coordinate, size);
    float3 neighborhoodRange = neighborhoodMaximum - neighborhoodMinimum;
    float3 margin = max(neighborhoodRange * 0.65, float3(0.01));
    history = clamp(
        history, neighborhoodMinimum - margin, neighborhoodMaximum + margin);

    float currentScale = max(max(current.r, current.g), max(current.b, 0.05));
    float relativeDelta = length(history - current) / currentScale;
    float change = smoothstep(0.30, 1.25, relativeDelta);
    float currentWeight = lerp(0.12, 0.75, change);
    float3 resolved = lerp(history, current, currentWeight);
    lens_flare_output_tex.Store(
        coordinate, float4(max(resolved, float3(0.0)), 1.0));
}
