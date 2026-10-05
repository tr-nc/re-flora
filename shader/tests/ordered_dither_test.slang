import ordered_dither;
import color;
import scene_pixel_filter;

bool close(float a, float b) { return abs(a - b) < 0.00002; }
bool same(float3 a, float3 b) { return all(a == b); }

export __extern_cpp int main()
{
    uint4 options = uint4(2u, 0u, 8u, 1u);
    float3 original = float3(0.034, 0.15, 2.4);
    // Exact old path, including invalid inputs: no decode/encode round-trip.
    if (!same(orderedDitherColor(original, int2(0), uint4(0,0,8,1), 1.0, ORDERED_GLOBAL), original)) return 1;
    if (!same(orderedDitherColor(original, int2(0), options, 0.0, ORDERED_GOD_RAYS), original)) return 2;
    if (!same(orderedDitherColor(original, int2(0), options, -1.0, ORDERED_GOD_RAYS), original)) return 3;
    if (orderedDitherEnabled(uint4(31,0,8,1), 1.0, ORDERED_GOD_RAYS)) return 4;
    if (!orderedDitherEnabled(uint4(31,0,8,1), 1.0, ORDERED_GLOBAL)) return 5;
    if (orderedDitherEnabled(options, asfloat(0x7fc00000u), ORDERED_GOD_RAYS)) return 6;

    for (uint pattern = 0u; pattern < 2u; ++pattern)
    {
        options.y = pattern;
        uint seen = 0u;
        float sum = 0.0;
        uint upperCount = 0u;
        for (int y = 0; y < 4; ++y) for (int x = 0; x < 4; ++x)
        {
            int2 cell = int2(x,y);
            float threshold = orderedThreshold(cell, pattern);
            if (threshold <= 0.0 || threshold >= 1.0) return 7;
            uint rank = uint(threshold * 16.0);
            if ((seen & (1u << rank)) != 0u) return 8;
            seen |= 1u << rank;
            sum += threshold;
            if (threshold < 0.5) upperCount++;
            if (threshold != orderedThreshold(cell + int2(4,8), pattern)) return 9;
            if (threshold != orderedThreshold(cell - int2(4,8), pattern)) return 10;
            float previous = -1.0;
            for (uint i = 0u; i <= 100u; ++i)
            {
                float value = float(i) / 100.0;
                float q = orderedDitherScalar(value, cell, options, 1.0, ORDERED_GOD_RAYS);
                if (q < previous || q < 0.0 || q > 1.0) return 11;
                if (abs(q - value) > 1.0 / 7.0 + 0.00001) return 12;
                previous = q;
            }
            for (uint level = 0u; level < 8u; ++level)
                if (!close(orderedDitherScalar(float(level)/7.0, cell, options, 1.0, ORDERED_GOD_RAYS), float(level)/7.0)) return 13;
            float3 result = orderedDitherColor(original, cell, options, 1.0, ORDERED_GOD_RAYS);
            float3 encoded = linearToSrgb(result);
            float3 source = linearToSrgb(original);
            if (!close(encoded.r / encoded.b, source.r / source.b) ||
                !close(encoded.g / encoded.b, source.g / source.b)) return 14;
            if (!all(isfinite(result)) || result.b <= 1.0) return 15;
            float3 invalid = float3(-2.0, asfloat(0x7fc00000u), asfloat(0x7f800000u));
            if (!same(orderedDitherColor(invalid, cell, options, 1.0, ORDERED_GOD_RAYS), float3(0))) return 16;
            if (!same(orderedDitherColor(float3(0), cell, options, 1.0, ORDERED_GOD_RAYS), float3(0))) return 17;
            float3 high = orderedDitherColor(float3(65504), cell, options, 1.0, ORDERED_GOD_RAYS);
            if (!all(isfinite(high)) || any(high > float3(65504.1))) return 18;
        }
        if (seen != 65535u || !close(sum, 8.0) || upperCount != 8u) return 19;
        // Uniform ramp tile mean approximates source within half a rank step.
        for (uint i = 0u; i <= 100u; ++i)
        {
            float value = float(i)/100.0;
            float mean = 0.0;
            for (int y=0; y<4; ++y) for (int x=0; x<4; ++x)
                mean += orderedDitherScalar(value, int2(x,y), options, 1.0, ORDERED_GOD_RAYS) / 16.0;
            if (abs(mean - value) > 1.0/(7.0*32.0)+0.00001) return 20;
        }
    }
    // Four ratios, two source densities/resolves share one coarse pattern.
    // Last screen cell is cropped, not stretched, even at 1023x767.
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
                for (uint sample=0u; sample<axis*axis; ++sample)
                {
                    int2 pixel=cell*int(axis)+scenePixelSampleOffset(sample,axis);
                    float2 uv=(float2(pixel)+0.5)/float2(render);
                    if (any(orderedSceneCell(uv,extent)!=cell)) return 22;
                }
            }
            if (any(scenePixelCoordinate(int2(screen)-1,stride,extent)!=int2(extent)-1)) return 23;
        }
    }
    printf("ordered dither: bypass, precedence, ranks, bounds, HDR tuple, coarse/partial cells passed\n");
    return 0;
}
