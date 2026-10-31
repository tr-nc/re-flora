import glass_surface_sampling;

export __extern_cpp int main()
{
    float2 first = float2(0);
    float2 second = float2(0);
    float mixed = 0;
    for (uint i = 0; i < GLASS_VOXEL_FACE_SAMPLE_COUNT; ++i)
    {
        float2 p = glassVoxelFaceSampleOffset(i);
        if (any(abs(p) >= float2(0.5))) return 1;
        first += p;
        second += p * p;
        mixed += p.x * p.y;
        if (any(p != glassVoxelFaceSampleOffset(i))) return 2;
    }
    first /= float(GLASS_VOXEL_FACE_SAMPLE_COUNT);
    second /= float(GLASS_VOXEL_FACE_SAMPLE_COUNT);
    mixed /= float(GLASS_VOXEL_FACE_SAMPLE_COUNT);
    // Exact moments of a uniform unit square: first/mixed = 0, second = 1/12.
    bool correct = all(abs(first) < float2(1e-6)) &&
        all(abs(second - float2(1.0 / 12.0)) < float2(1e-6)) && abs(mixed) < 1e-6;
    printf("glass face quadrature taps=%u mixed=%f correct=%d\n",
        GLASS_VOXEL_FACE_SAMPLE_COUNT, mixed, int(correct));
    return correct ? 0 : 3;
}
