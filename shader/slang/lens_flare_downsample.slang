static const uint LENS_FLARE_GROUP_SIZE = 8u;

[[vk::binding(0, 0)]]
[[vk::image_format("r11f_g11f_b10f")]]
readonly RWTexture2D<float4> lens_flare_full_output_tex;

[[vk::binding(1, 0)]]
[[vk::image_format("r11f_g11f_b10f")]]
WTexture2D<float4> lens_flare_output_tex;

[shader("compute")]
[numthreads(LENS_FLARE_GROUP_SIZE, LENS_FLARE_GROUP_SIZE, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint destinationWidth;
    uint destinationHeight;
    lens_flare_output_tex.GetDimensions(
        destinationWidth, destinationHeight);
    int2 destinationCoordinate = int2(dispatchThreadId.xy);
    if (any(destinationCoordinate >=
            int2(destinationWidth, destinationHeight)))
    {
        return;
    }

    uint sourceWidth;
    uint sourceHeight;
    lens_flare_full_output_tex.GetDimensions(sourceWidth, sourceHeight);
    int2 sourceSize = int2(sourceWidth, sourceHeight);
    int2 destinationSize = int2(destinationWidth, destinationHeight);
    float2 scale = float2(sourceSize) / float2(destinationSize);

    int2 sourceMinimum = int2(floor(
        float2(destinationCoordinate) * scale));
    int2 sourceMaximumExclusive = int2(floor(
        float2(destinationCoordinate + int2(1)) * scale));
    sourceMinimum = clamp(
        sourceMinimum, int2(0), sourceSize - int2(1));
    sourceMaximumExclusive = clamp(
        sourceMaximumExclusive, sourceMinimum + int2(1), sourceSize);

    float3 sum = float3(0.0);
    float count = 0.0;
    for (int y = sourceMinimum.y; y < sourceMaximumExclusive.y; ++y)
    {
        for (int x = sourceMinimum.x; x < sourceMaximumExclusive.x; ++x)
        {
            sum += lens_flare_full_output_tex[int2(x, y)].rgb;
            count += 1.0;
        }
    }

    float3 average = sum / max(count, 1.0);
    lens_flare_output_tex.Store(
        destinationCoordinate, float4(average, 1.0));
}
