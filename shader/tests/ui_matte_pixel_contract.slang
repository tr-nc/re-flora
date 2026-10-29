import ui_matte_pixel;

float3 pattern(uint x, uint y)
{
    return float3(float((x * 17u + y * 29u) % 251u) / 250.0,
        float((x + y) % 2u), float(x * x + y) / 2048.0);
}
struct PatternSource : IMatteCellSource
{
    uint2 lo;
    uint2 hi;
    float3 sample(uint2 pixel)
    {
        if (any(pixel < lo) || any(pixel >= hi)) return float3(-1000.0);
        return pattern(pixel.x, pixel.y);
    }
}
struct ConstantSource : IMatteCellSource
{
    float3 sample(uint2 pixel) { return float3(0.17, 0.63, 1.0); }
}
struct ImpulseSource : IMatteCellSource
{
    uint2 bright;
    float3 sample(uint2 pixel) { return all(pixel == bright) ? float3(1.0) : float3(0.0); }
}

export __extern_cpp int main()
{
    // The five chosen texels, and only those texels, contribute 1/5 each.
    for (uint y = 0u; y < 12u; ++y)
    for (uint x = 0u; x < 12u; ++x) {
        ImpulseSource impulse = { uint2(x, y) };
        float value = uiMatteCellFiveTap(impulse, uint2(0), uint2(12)).x;
        bool chosen = (x == 6u && y == 6u)
            || ((x == 3u || x == 9u) && (y == 3u || y == 9u));
        if (abs(value - (chosen ? 0.2 : 0.0)) > 0.000001) return 1;
    }

    // Uneven viewport dimensions, every grid phase, viewport-cropped edge
    // cells, fractional-DPI-rounded sizes, one-pixel and oversized cells.
    uint sizes[7] = { 1u, 4u, 9u, 12u, 21u, 24u, 96u };
    uint width = 37u, height = 29u;
    float2 taps[5] = {
        float2(0.5, 0.5), float2(0.25, 0.25), float2(0.75, 0.25),
        float2(0.25, 0.75), float2(0.75, 0.75)
    };
    for (uint s = 0u; s < 7u; ++s) {
        uint size = sizes[s];
        for (uint phase = 0u; phase < size; ++phase) {
            int2 origin = -int2(phase, (phase * 3u) % size);
            uint visited = 0u;
            for (uint cy = 0u; int(cy * size) + origin.y < int(height); ++cy)
            for (uint cx = 0u; int(cx * size) + origin.x < int(width); ++cx) {
                uint2 xr = uiMatteCellRange(cx, size, width, origin.x);
                uint2 yr = uiMatteCellRange(cy, size, height, origin.y);
                uint2 lo = uint2(xr.x, yr.x), hi = uint2(xr.y, yr.y);
                int2 start = origin + int2(cx, cy) * int(size);
                if (any(lo != uint2(max(start, int2(0))))) return 2;
                if (any(hi != uint2(min(start + int(size), int2(width, height))))) return 3;
                PatternSource source = { lo, hi };
                float3 actual = uiMatteCellFiveTap(source, lo, hi);
                float3 expected = float3(0.0);
                for (uint i = 0u; i < 5u; ++i) {
                    uint2 pixel = lo + uint2(floor(float2(hi - lo) * taps[i]));
                    expected += pattern(pixel.x, pixel.y) / 5.0;
                }
                if (any(abs(actual - expected) > float3(0.00001))) return 4;
                ConstantSource constant;
                if (any(abs(uiMatteCellFiveTap(constant, lo, hi)
                    - float3(0.17, 0.63, 1.0)) > float3(0.000001))) return 5;
                visited += (hi.x - lo.x) * (hi.y - lo.y);
            }
            if (visited != width * height) return 6;
        }
    }
    return 0;
}
