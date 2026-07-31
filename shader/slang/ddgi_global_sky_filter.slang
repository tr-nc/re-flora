import skylight;
import tracer_types;

static const uint DDGI_IRRADIANCE_INTERIOR_SIDE = 8u;
static const uint DDGI_SKY_FILTER_SAMPLE_COUNT = 2048u;
static const float DDGI_GOLDEN_ANGLE = 2.39996322972865332;
static const float DDGI_FOUR_PI = 12.566370614359172;

[[vk::binding(1, 0)]]
ConstantBuffer<U_SunInfo> sun_info;

[[vk::binding(23, 0)]]
RWTexture2D<float4> ddgi_global_sky_irradiance;

float signNotZero(float value)
{
    return value >= 0.0 ? 1.0 : -1.0;
}

float3 octDecode(float2 encoded)
{
    float3 direction =
        float3(encoded.x, 1.0 - abs(encoded.x) - abs(encoded.y), encoded.y);
    if (direction.y < 0.0)
    {
        float unfoldedX = direction.x;
        float unfoldedZ = direction.z;
        direction.x = (1.0 - abs(unfoldedZ)) * signNotZero(unfoldedX);
        direction.z = (1.0 - abs(unfoldedX)) * signNotZero(unfoldedZ);
    }
    return normalize(direction);
}

float3 fibonacciDirection(uint sampleIndex)
{
    float y = 1.0 - (2.0 * float(sampleIndex) + 1.0) /
                        float(DDGI_SKY_FILTER_SAMPLE_COUNT);
    float radius = sqrt(max(0.0, 1.0 - y * y));
    float azimuth = DDGI_GOLDEN_ANGLE * float(sampleIndex);
    return float3(radius * cos(azimuth), y, radius * sin(azimuth));
}

[shader("compute")]
[numthreads(8, 8, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    if (any(dispatchThreadId.xy >= DDGI_IRRADIANCE_INTERIOR_SIDE)) return;

    float2 encoded =
        ((float2(dispatchThreadId.xy) + 0.5) /
         float(DDGI_IRRADIANCE_INTERIOR_SIDE)) *
            2.0 -
        1.0;
    float3 normal = octDecode(encoded);
    float3 irradiance = 0.0;

    [Loop]
    for (uint sampleIndex = 0u; sampleIndex < DDGI_SKY_FILTER_SAMPLE_COUNT;
         ++sampleIndex)
    {
        float3 direction = fibonacciDirection(sampleIndex);
        float cosineWeight = max(0.0, dot(normal, direction));
        irradiance += getSkyColor(direction, normalize(sun_info.sun_dir)) *
                      cosineWeight;
    }
    irradiance *= DDGI_FOUR_PI / float(DDGI_SKY_FILTER_SAMPLE_COUNT);

    ddgi_global_sky_irradiance[dispatchThreadId.xy + uint2(1u, 1u)] =
        float4(irradiance, 1.0);
}
