import ddgi_types;
import skylight;

static const uint DDGI_IRRADIANCE_INTERIOR_SIDE = 8u;
static const uint DDGI_SKY_FILTER_SAMPLE_COUNT = 2048u;
static const float DDGI_GOLDEN_ANGLE = 2.39996322972865332;
static const float DDGI_FOUR = 4.0;
static const float DDGI_AUTHORED_SKY_RADIANCE_CALIBRATION =
    3.14159265358979323846;

[[vk::binding(23, 0)]]
RWTexture2D<float4> ddgi_global_sky_irradiance;

[[vk::binding(30, 0)]]
ConstantBuffer<U_DdgiRadianceSun> ddgi_radiance_sun;

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
        irradiance += getSkyColor(direction, normalize(ddgi_radiance_sun.direction)) *
                      DDGI_AUTHORED_SKY_RADIANCE_CALIBRATION * cosineWeight;
    }
    // Store diffuse irradiance divided by pi. The authored-sky calibration
    // above keeps this scene-facing result at the established brightness.
    irradiance *= DDGI_FOUR / float(DDGI_SKY_FILTER_SAMPLE_COUNT);

    ddgi_global_sky_irradiance[dispatchThreadId.xy + uint2(1u, 1u)] =
        float4(irradiance, 1.0);
}
