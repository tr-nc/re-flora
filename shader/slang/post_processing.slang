import color;
import dither;
import tone_mapping;

struct U_PostProcessingInfo
{
    float scaling_factor;
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
    float2 scalingFactor = float2(inputWidth, inputHeight) / float2(outputWidth, outputHeight);
    // Point sampling of the scene's actual low-resolution target. UI is
    // composited later at native resolution; no per-model pixel lattice.
    int2 mappedUvi = min(int2((float2(uvi) + 0.5) * scalingFactor),
                        int2(inputWidth, inputHeight) - 1);
    float3 finalColor = composited_tex[mappedUvi].rgb;
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
