import ordered_dither;
import color;
import scene_pixel_filter;

bool close(float a, float b) { return abs(a - b) < 0.00002; }
bool same(float3 a, float3 b) { return all(a == b); }

export __extern_cpp int main()
{
    float3 original = float3(0.034, 0.15, 2.4);
    float3 invalid = float3(-2.0, asfloat(0x7fc00000u), asfloat(0x7f800000u));
    // Exact bypass: no conversion/sanitization before the global gate.
    if (!same(orderedDitherColor(original, int2(0), 0u, 8u, 1.0), original)) return 1;
    if (!same(orderedDitherColor(original, int2(0), 1u, 8u, 0.0), original)) return 2;
    if (!same(orderedDitherColor(original, int2(0), 1u, 8u, -1.0), original)) return 3;
    if (orderedDitherEnabled(0u, 1.0)) return 4;
    if (!orderedDitherEnabled(1u, 1.0)) return 5;
    if (orderedDitherEnabled(1u, asfloat(0x7fc00000u))) return 6;
    float3 bypass = orderedDitherColor(invalid, int2(0), 0u, 8u, 1.0);
    if (bypass.r != -2.0 || !isnan(bypass.g) || !isinf(bypass.b)) return 24;

    uint seen = 0u;
    float sum = 0.0;
    uint upperCount = 0u;
    for (int y = 0; y < 4; ++y) for (int x = 0; x < 4; ++x)
    {
        int2 cell = int2(x,y);
        float threshold = orderedThreshold(cell);
        if (threshold <= 0.0 || threshold >= 1.0) return 7;
        uint rank = uint(threshold * 16.0);
        if ((seen & (1u << rank)) != 0u) return 8;
        seen |= 1u << rank;
        sum += threshold;
        if (threshold < 0.5) upperCount++;
        if (threshold != orderedThreshold(cell + int2(4,8))) return 9;
        if (threshold != orderedThreshold(cell - int2(4,8))) return 10;
        float previous = -1.0;
        for (uint i = 0u; i <= 100u; ++i)
        {
            float value = float(i) / 100.0;
            float q = orderedDitherScalar(value, cell, 8u);
            if (q < previous || q < 0.0 || q > 1.0) return 11;
            if (abs(q - value) > 1.0 / 7.0 + 0.00001) return 12;
            previous = q;
        }
        for (uint level = 0u; level < 8u; ++level)
            if (!close(orderedDitherScalar(float(level)/7.0, cell, 8u), float(level)/7.0)) return 13;
        float3 result = orderedDitherColor(original, cell, 1u, 8u, 1.0);
        float3 encoded = linearToSrgb(result);
        float3 source = linearToSrgb(original);
        if (!close(encoded.r / encoded.b, source.r / source.b) ||
            !close(encoded.g / encoded.b, source.g / source.b)) return 14;
        if (!all(isfinite(result)) || result.b <= 1.0) return 15;
        if (!same(orderedDitherColor(invalid, cell, 1u, 8u, 1.0), float3(0))) return 16;
        if (!same(orderedDitherColor(float3(0), cell, 1u, 8u, 1.0), float3(0))) return 17;
        float3 high = orderedDitherColor(float3(65504), cell, 1u, 8u, 1.0);
        if (!all(isfinite(high)) || any(high > float3(65504.1))) return 18;
        // Strength is applied once in encoded RGB space.
        float3 partial = linearToSrgb(orderedDitherColor(original, cell, 1u, 8u, 0.25));
        if (any(abs(partial - lerp(source, encoded, 0.25)) > float3(0.00002))) return 25;
    }
    if (seen != 65535u || !close(sum, 8.0) || upperCount != 8u) return 19;
    for (uint i = 0u; i <= 100u; ++i)
    {
        float value = float(i)/100.0;
        float mean = 0.0;
        for (int y=0; y<4; ++y) for (int x=0; x<4; ++x)
            mean += orderedDitherScalar(value, int2(x,y), 8u) / 16.0;
        if (abs(mean - value) > 1.0/(7.0*32.0)+0.00001) return 20;
    }
    // The post pass's final cell coordinate is independent of source density.
    // Enlarged blocks remain uniform, including cropped odd-size edge cells.
    for (uint stride=1u; stride<=8u; stride*=2u)
    {
        uint2 screen = uint2(1023,767);
        uint2 extent = (screen+stride-1u)/stride;
        for (uint axis=1u; axis<=4u; axis*=2u)
        {
            uint2 render = extent*axis;
            if (any(scenePixelExtent(render, axis) != extent)) return 21;
            for (uint y=0u; y<screen.y; y+=37u) for (uint x=0u; x<screen.x; x+=31u)
            {
                int2 cell=scenePixelCoordinate(int2(x,y),stride,extent);
                int2 origin=cell*int(stride);
                float3 expected=orderedDitherColor(original,cell,1u,8u,1.0);
                for (uint dy=0u; dy<stride; ++dy) for (uint dx=0u; dx<stride; ++dx)
                {
                    int2 output=min(origin+int2(dx,dy),int2(screen)-1);
                    int2 actual=scenePixelCoordinate(output,stride,scenePixelExtent(render,axis));
                    if (!same(orderedDitherColor(original,actual,1u,8u,1.0),expected)) return 22;
                }
            }
            if (any(scenePixelCoordinate(int2(screen)-1,stride,extent)!=int2(extent)-1)) return 23;
        }
    }
    printf("global Bayer: exact bypass, ranks, bounds, HDR tuple, strength, coarse/partial cells passed\n");
    return 0;
}
