import color;
import dither;
import tone_mapping;
import scene_pixel_filter;

struct U_PostProcessingInfo
{
    uint samples_per_axis;
    float dither_strength_lsb;
};

[[vk::binding(1, 0)]]
ConstantBuffer<U_PostProcessingInfo> post_processing_info;

[[vk::binding(2, 0)]]
[[vk::image_format("rgba16f")]]
RWTexture2D<float4> composited_tex;

[[vk::binding(3, 0)]]
[[vk::image_format("rgba16f")]]
WTexture2D<float4> screen_output_tex;

[shader("compute")]
[numthreads(8, 8, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint outputWidth;
    uint outputHeight;
    screen_output_tex.GetDimensions(outputWidth, outputHeight);

    int2 uvi = int2(dispatchThreadId.xy);
    if (any(uvi >= int2(outputWidth, outputHeight)))
    {
        return;
    }

    uint inputWidth;
    uint inputHeight;
    composited_tex.GetDimensions(inputWidth, inputHeight);
    uint samplesPerAxis = post_processing_info.samples_per_axis;
    uint2 pixelExtent = scenePixelExtent(uint2(inputWidth, inputHeight), samplesPerAxis);
    int2 mappedUvi = scenePixelCoordinate(uvi, uint2(outputWidth, outputHeight), pixelExtent);
    float3 finalColor;
    if (samplesPerAxis == 2u)
    {
        int2 source = mappedUvi * 2;
        finalColor = scenePixelBoxAverage(
            composited_tex[source].rgb,
            composited_tex[source + int2(1, 0)].rgb,
            composited_tex[source + int2(0, 1)].rgb,
            composited_tex[source + int2(1, 1)].rgb);
    }
    else
    {
        // Preserve the original point-sampled mode for runtime A/B comparison.
        finalColor = composited_tex[mappedUvi].rgb;
    }
    finalColor = toneMapForDisplay(finalColor);

    // Keep the whole enlarged scene pixel one color, including display dither.
    float ditherOffset = getDitherOffset(mappedUvi, post_processing_info.dither_strength_lsb);
    if (post_processing_info.dither_strength_lsb > 0.0)
    {
        float3 encodedColor = linearToSrgb(finalColor);
        encodedColor = max(encodedColor + ditherOffset.xxx, float3(0.0));
        finalColor = srgbToLinear(encodedColor);
    }
    screen_output_tex.Store(uvi, float4(finalColor, 1.0));
}
