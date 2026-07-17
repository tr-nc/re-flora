import tracer_types;

static const uint CLOUD_GROUP_SIZE = 8u;

struct CloudShadowTemporalPushConstants
{
    uint reset_history;
};

[[vk::push_constant]]
ConstantBuffer<CloudShadowTemporalPushConstants> cloud_shadow_temporal_push;

[[vk::binding(0, 0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(6, 0)]] Sampler2D cloud_shadow_raw_tex;
[[vk::binding(7, 0)]] Sampler2D cloud_shadow_history_tex;

[[vk::binding(8, 0)]]
[[vk::image_format("r16f")]]
WTexture2D<float> cloud_shadow_tex;

float currentMinimum3x3(int2 coordinate, int2 size)
{
    float value = cloud_shadow_raw_tex.Load(int3(coordinate, 0)).r;
    for (int y = -1; y <= 1; ++y)
    {
        for (int x = -1; x <= 1; ++x)
        {
            int2 sampleCoordinate = clamp(
                coordinate + int2(x, y), int2(0), size - int2(1));
            value = min(value,
                cloud_shadow_raw_tex.Load(int3(sampleCoordinate, 0)).r);
        }
    }
    return value;
}

float currentMaximum3x3(int2 coordinate, int2 size)
{
    float value = cloud_shadow_raw_tex.Load(int3(coordinate, 0)).r;
    for (int y = -1; y <= 1; ++y)
    {
        for (int x = -1; x <= 1; ++x)
        {
            int2 sampleCoordinate = clamp(
                coordinate + int2(x, y), int2(0), size - int2(1));
            value = max(value,
                cloud_shadow_raw_tex.Load(int3(sampleCoordinate, 0)).r);
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
    cloud_shadow_tex.GetDimensions(width, height);
    int2 size = int2(width, height);
    int2 coordinate = int2(dispatchThreadId.xy);
    if (any(coordinate >= size))
    {
        return;
    }

    float current = clamp(
        cloud_shadow_raw_tex.Load(int3(coordinate, 0)).r, 0.0, 1.0);
    bool disabled = gui_input.cloud_shadows_enabled == 0u ||
                    gui_input.clouds_enabled == 0u ||
                    gui_input.cloud_coverage <= 0.001 ||
                    gui_input.cloud_density <= 0.001;
    if (disabled || cloud_shadow_temporal_push.reset_history != 0u)
    {
        cloud_shadow_tex.Store(coordinate, current);
        return;
    }

    float history = clamp(
        cloud_shadow_history_tex.Load(int3(coordinate, 0)).r, 0.0, 1.0);
    float neighborhoodMinimum = currentMinimum3x3(coordinate, size);
    float neighborhoodMaximum = currentMaximum3x3(coordinate, size);
    float neighborhoodRange = neighborhoodMaximum - neighborhoodMinimum;
    float margin = max(neighborhoodRange * 0.55, 0.025);
    history = clamp(
        history, neighborhoodMinimum - margin, neighborhoodMaximum + margin);

    float historyDelta = abs(history - current);
    float change = smoothstep(0.035, 0.18, historyDelta);
    float baseCurrentWeight = clamp(
        gui_input.cloud_temporal_alpha * 4.0, 0.10, 0.35);
    float currentWeight = lerp(baseCurrentWeight, 0.70, change);
    if (neighborhoodMinimum > 0.995)
    {
        currentWeight = 1.0;
    }

    float resolved = lerp(history, current, currentWeight);
    cloud_shadow_tex.Store(coordinate, clamp(resolved, 0.0, 1.0));
}
