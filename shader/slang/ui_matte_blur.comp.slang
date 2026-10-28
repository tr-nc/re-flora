import color;
import ui_matte_material;
import ui_matte_blur_kernel;

struct MatteBlurPush
{
    float pixels_per_point;
    uint horizontal;
};
[[vk::push_constant]] ConstantBuffer<MatteBlurPush> blur_push;
[[vk::binding(0, 0)]]
[[vk::image_format("rgba16f")]]
readonly RWTexture2D<float4> blur_source;
[[vk::binding(1, 0)]]
[[vk::image_format("rgba16f")]]
WTexture2D<float4> blur_output;

struct SceneLine : IMatteBlurLine
{
    int2 origin;
    int2 extent;

    float3 sample(int offset)
    {
        int2 axis = blur_push.horizontal != 0u ? int2(1, 0) : int2(0, 1);
        int2 pixel = clamp(origin + axis * offset, int2(0), extent - 1);
        float3 value = blur_source[pixel].rgb;
        // Convert before filtering, only on the first pass. Both intermediates
        // store encoded sRGB in float textures (no hardware sRGB conversion).
        return blur_push.horizontal != 0u ? linearToSrgb(value) : value;
    }
}

[shader("compute")]
[numthreads(8, 8, 1)]
void main(uint3 thread : SV_DispatchThreadID)
{
    uint width, height;
    blur_output.GetDimensions(width, height);
    if (any(thread.xy >= uint2(width, height))) return;
    SceneLine line = { int2(thread.xy), int2(width, height) };
    float sigma = UI_MATTE_FROST_POINTS * blur_push.pixels_per_point;
    blur_output.Store(thread.xy, float4(uiMatteBlur(line, sigma), 1.0));
}
