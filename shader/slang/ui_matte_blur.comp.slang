import color;
import ui_matte_material;
import ui_matte_blur_kernel;
import ui_matte_pixel;

struct MatteBlurPush
{
    float sigma;
    uint horizontal;
    uint pixel_cell_size;
    uint radius;
    uint2 dispatch_origin;
    uint2 dispatch_extent;
    int2 grid_origin;
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

struct SceneCell : IMatteCellSource
{
    float3 sample(uint2 pixel)
    {
        // Average in the same encoded-sRGB space as the former area filter.
        return linearToSrgb(blur_source[pixel].rgb);
    }
}

[shader("compute")]
[numthreads(8, 8, 1)]
void main(uint3 thread : SV_DispatchThreadID)
{
    if (any(thread.xy >= blur_push.dispatch_extent)) return;
    thread.xy += blur_push.dispatch_origin;
    uint width, height;
    blur_output.GetDimensions(width, height);
    uint cellSize = blur_push.pixel_cell_size;
    if (cellSize != 0u)
    {
        // One invocation and five source reads per cell, directly from the
        // current composite. No horizontal intermediate or second dispatch.
        uint2 cells = (uint2(int2(width, height) - blur_push.grid_origin) + cellSize - 1u) / cellSize;
        if (any(thread.xy >= cells)) return;
        uint2 xr = uiMatteCellRange(thread.x, cellSize, width, blur_push.grid_origin.x);
        uint2 yr = uiMatteCellRange(thread.y, cellSize, height, blur_push.grid_origin.y);
        if (xr.x == xr.y || yr.x == yr.y) return;
        SceneCell source;
        blur_output.Store(thread.xy, float4(uiMatteCellFiveTap(source,
            uint2(xr.x, yr.x), uint2(xr.y, yr.y)), 1.0));
        return;
    }
    if (any(thread.xy >= uint2(width, height))) return;
    SceneLine line = { int2(thread.xy), int2(width, height) };
    blur_output.Store(thread.xy, float4(uiMatteBlur(line, blur_push.sigma, int(blur_push.radius)), 1.0));
}
