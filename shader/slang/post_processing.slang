import color;
import dither;
import ordered_dither;
import tone_mapping;
import scene_pixel_filter;
import scene_pixel_style;

struct U_PostProcessingInfo
{
    uint samples_per_axis;
    uint pixel_stride;
    uint resolve_mode;
    float dither_strength_lsb;
    uint ordered_dither_enabled;
    uint ordered_dither_levels;
    float ordered_dither_strength;
};

[[vk::binding(1, 0)]]
ConstantBuffer<U_PostProcessingInfo> post_processing_info;

[[vk::binding(2, 0)]]
[[vk::image_format("rgba16f")]]
RWTexture2D<float4> composited_tex;

[[vk::binding(3, 0)]]
[[vk::image_format("rgba16f")]]
WTexture2D<float4> screen_output_tex;

float3 resolveScenePixel(int2 pixel, uint axis)
{
    if (axis > 1u && post_processing_info.resolve_mode == 1u)
    {
        int2 source = pixel * int(axis);
        float3 colors[16];
        for (uint i = 0u; i < 16u; i++) colors[i] = float3(0.0);
        uint count = axis * axis;
        for (uint sample = 0u; sample < count; sample++)
            colors[sample] = toneMapForDisplay(composited_tex[source + scenePixelSampleOffset(sample, axis)].rgb);
        // Statistics use display colors; return display-linear RGB. No double
        // tone mapping or gamma encoding at presentation.
        return contrastPixelColor(colors, count);
    }
    if (axis > 1u)
    {
        int2 source = pixel * int(axis);
        float3 sum = float3(0.0);
        for (uint sample = 0u; sample < axis * axis; sample++)
            sum += composited_tex[source + scenePixelSampleOffset(sample, axis)].rgb;
        // Preserve the previous HDR-average reference, including operation order.
        return toneMapForDisplay(scenePixelBoxAverage(sum, axis));
    }
    return toneMapForDisplay(composited_tex[pixel].rgb);
}

groupshared float3 resolvedColors[64];

[shader("compute")]
[numthreads(8, 8, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID, uint3 localId : SV_GroupThreadID)
{
    uint outputWidth;
    uint outputHeight;
    screen_output_tex.GetDimensions(outputWidth, outputHeight);
    int2 outputExtent = int2(outputWidth, outputHeight);
    int2 uvi = int2(dispatchThreadId.xy);
    if (any(uvi >= outputExtent)) return;

    uint inputWidth;
    uint inputHeight;
    composited_tex.GetDimensions(inputWidth, inputHeight);
    uint axis = post_processing_info.samples_per_axis;
    uint stride = post_processing_info.pixel_stride;
    uint2 pixelExtent = scenePixelExtent(uint2(inputWidth, inputHeight), axis);
    int2 pixel = scenePixelCoordinate(uvi, stride, pixelExtent);
    float3 finalColor;

    int2 groupOrigin = uvi - int2(localId.xy);
    // Group-uniform condition. Partial edge groups MUST avoid the barrier:
    // out-of-window invocations have already returned. Exact integer strides
    // dividing 8 align coarse cells with groups, so one leader resolves each cell.
    bool fullGroup = all(groupOrigin + int2(7) < outputExtent);
    if (axis > 1u && post_processing_info.resolve_mode == 1u && fullGroup && 8u % stride == 0u)
    {
        uint cell = (localId.y / stride) * (8u / stride) + localId.x / stride;
        if (localId.x % stride == 0u && localId.y % stride == 0u)
            resolvedColors[cell] = resolveScenePixel(pixel, axis);
        GroupMemoryBarrierWithGroupSync();
        finalColor = resolvedColors[cell];
    }
    else
    {
        finalColor = resolveScenePixel(pixel, axis);
    }

    bool globalDither = orderedDitherEnabled(post_processing_info.ordered_dither_enabled,
        post_processing_info.ordered_dither_strength);
    finalColor = orderedDitherColor(finalColor, pixel, post_processing_info.ordered_dither_enabled,
        post_processing_info.ordered_dither_levels, post_processing_info.ordered_dither_strength);

    // Dither also belongs to the final coarse cell, not individual screen pixels.
    float ditherOffset = getDitherOffset(pixel, post_processing_info.dither_strength_lsb);
    if (!globalDither && post_processing_info.dither_strength_lsb > 0.0)
    {
        float3 encodedColor = linearToSrgb(finalColor);
        encodedColor = max(encodedColor + ditherOffset.xxx, float3(0.0));
        finalColor = srgbToLinear(encodedColor);
    }
    screen_output_tex.Store(uvi, float4(finalColor, 1.0));
}
