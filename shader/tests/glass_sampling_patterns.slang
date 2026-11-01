import glass_surface_sampling;

export __extern_cpp int main()
{
    int failures = 0;
    for (uint pattern = 0u; pattern <= 5u; ++pattern)
    {
        uint expectedCount = pattern == 1u || pattern == 2u ? 1u :
            pattern == 3u || pattern == 4u ? 2u : 4u;
        uint count = glassVoxelFaceSampleCount(pattern);
        if (count != expectedCount) ++failures;
        for (int view = -40; view <= 40; ++view)
        {
            float3 ray = float3(float(view) * 0.05, 0.35, -1.0);
            float2 axis = float2(1.0, float(view) * 0.01);
            float2 mean = float2(0.0);
            for (uint tap = 0u; tap < count; ++tap)
            {
                float2 p = glassVoxelFacePatternOffset(pattern, tap, ray, axis);
                if (any(isnan(p)) || any(isinf(p)) || any(abs(p) >= float2(0.5))) ++failures;
                if (any(p != glassVoxelFacePatternOffset(pattern, tap, ray, axis))) ++failures;
                if (pattern == 0u || pattern == 5u)
                    if (any(p != glassVoxelFaceSampleOffset(tap))) ++failures;
                mean += p;
            }
            if (pattern != 2u && any(abs(mean) > float2(1.0e-6))) ++failures;
        }
    }
    float2 viewPoint = glassVoxelFacePatternOffset(2u, 0u, float3(1.0, -2.0, 4.0), float2(1.0, 0.0));
    if (any(abs(viewPoint - float2(0.125, -0.25)) > float2(1.0e-6))) ++failures;
    for (uint pattern = 1u; pattern <= 4u; ++pattern)
    {
        float2 p = glassVoxelFacePatternOffset(pattern, 0u, float3(0.0), float2(0.0));
        if (any(isnan(p)) || any(isinf(p)) || any(abs(p) >= float2(0.5))) ++failures;
    }
    // Camera movement must move the representative point continuously, not choose
    // a different subcell or rotate samples using screen/frame noise.
    for (int step = -100; step < 100; ++step)
    {
        float3 ray = float3(float(step) * 0.005, 0.1, -1.0);
        float2 a = glassVoxelFacePatternOffset(2u, 0u, ray, float2(1.0, 0.0));
        float2 b = glassVoxelFacePatternOffset(2u, 0u, ray + float3(0.005, 0.0, 0.0), float2(1.0, 0.0));
        if (length(a - b) > 0.00251) ++failures;
    }
    printf("glass sampling patterns failures=%d\n", failures);
    return failures == 0 ? 0 : 1;
}
