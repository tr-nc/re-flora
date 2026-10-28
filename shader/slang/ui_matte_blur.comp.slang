import color;
import ui_matte_material;
import ui_matte_blur_kernel;
import ui_matte_pixel;

struct MatteBlurPush
{
    float pixels_per_point;
    uint horizontal;
    uint pixel_cell_size;
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
    uint cellSize = blur_push.pixel_cell_size;
    if (cellSize != 0u)
    {
        // Horizontal: one mean per cell-column and source row, packed at x=cell.
        // Vertical: one mean per complete cell, packed at (cellX, cellY).
        // Every source pixel contributes once, rather than repeating NxN work
        // for every fragment of the block. Existing images are reused on toggles.
        bool horizontal = blur_push.horizontal != 0u;
        uint2 cells = (uint2(width, height) + cellSize - 1u) / cellSize;
        uint2 dispatchExtent = horizontal ? uint2(cells.x, height) : cells;
        if (any(thread.xy >= dispatchExtent)) return;
        uint2 range = uiMatteCellRange(horizontal ? thread.x : thread.y,
            cellSize, horizontal ? width : height);
        int2 origin = horizontal ? int2(range.x, thread.y) : int2(thread.x, range.x);
        SceneLine cellLine = { origin, int2(width, height) };
        blur_output.Store(thread.xy, float4(uiMatteCellMean(cellLine, range.y - range.x), 1.0));
        return;
    }
    if (any(thread.xy >= uint2(width, height))) return;
    SceneLine line = { int2(thread.xy), int2(width, height) };
    float sigma = UI_MATTE_FROST_POINTS * blur_push.pixels_per_point;
    blur_output.Store(thread.xy, float4(uiMatteBlur(line, sigma), 1.0));
}
