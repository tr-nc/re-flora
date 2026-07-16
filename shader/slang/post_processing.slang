import dither;

struct U_PostProcessingInfo
{
    float scaling_factor;
};

[[vk::binding(1, 0)]]
ConstantBuffer<U_PostProcessingInfo> post_processing_info;

[[vk::binding(2, 0)]]
[[vk::image_format("r11g11b10f")]]
RWTexture2D<float4> composited_tex;

[[vk::binding(3, 0)]]
[[vk::image_format("rgba8")]]
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
    int2 mappedUvi = int2(float2(uvi) * scalingFactor);
    float3 finalColor = composited_tex[mappedUvi].rgb;

    finalColor += getDitherMask(uvi) * 3.0;
    screen_output_tex.Store(uvi, float4(finalColor, 1.0));
}
