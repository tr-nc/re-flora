import ui_matte_pixel;
import ui_matte_blur_kernel;

float3 pattern(uint x, uint y)
{
    return float3(float((x * 17u + y * 29u) % 251u) / 250.0,
        float((x + y) % 2u), float(x * x + y) / 2048.0);
}
struct Row : IMatteBlurLine
{
    uint x;
    uint y;
    float3 sample(int offset) { return pattern(x + uint(offset), y); }
}
struct Column : IMatteBlurLine
{
    uint x;
    uint y;
    uint count;
    float3 sample(int offset) {
        Row row = { x, y + uint(offset) };
        return uiMatteCellMean(row, count);
    }
}

export __extern_cpp int main()
{
    // Uneven viewport dimensions, cropped edge cells, fractional-DPI-rounded
    // cell sizes, one-pixel cells, and a cell larger than the viewport.
    uint sizes[7] = { 1u, 4u, 9u, 12u, 21u, 24u, 96u };
    uint width = 37u, height = 29u;
    for (uint s = 0u; s < 7u; ++s) {
        uint size = sizes[s];
        uint visited = 0u;
        for (uint cy = 0u; cy * size < height; ++cy)
        for (uint cx = 0u; cx * size < width; ++cx) {
            uint2 xr = uiMatteCellRange(cx, size, width);
            uint2 yr = uiMatteCellRange(cy, size, height);
            Column column = { xr.x, yr.x, xr.y - xr.x };
            float3 actual = uiMatteCellMean(column, yr.y - yr.x);
            float3 expected = float3(0.0);
            uint count = 0u;
            for (uint y = yr.x; y < yr.y; ++y)
            for (uint x = xr.x; x < xr.y; ++x) {
                expected += pattern(x, y);
                ++count;
                if (x / size != cx || y / size != cy) return 1;
            }
            expected /= float(count);
            if (any(abs(actual - expected) > float3(0.00001))) return 2;
            visited += count;
        }
        if (visited != width * height) return 3;
    }
    return 0;
}
