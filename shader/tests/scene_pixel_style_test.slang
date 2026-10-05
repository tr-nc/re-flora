import scene_pixel_style;

bool near(float3 a, float3 b) { return all(abs(a - b) < 0.00004); }

export __extern_cpp int main()
{
    float3 values[16];
    for (uint i = 0u; i < 16u; i++) values[i] = float3(0.0);
    for (uint count = 4u; count <= 16u; count *= 4u)
    {
        // Uniform blocks preserve their displayed color, including the Lab toe.
        for (uint level = 0u; level < 5u; level++)
        {
            float3 c = float3(0.0001, 0.07, 0.8) * (float(level) / 4.0);
            for (uint i = 0u; i < count; i++) values[i] = c;
            if (!near(contrastPixelColor(values, count), c)) return 1;
            if (!near(pixelLabToLinear(pixelLinearToLab(c)), c)) return 2;
        }
        // Balanced dark/bright coverage chooses contrast instead of grey average.
        for (uint i = 0u; i < count; i++) values[i] = float3(i < count / 2u ? 0.0 : 1.0);
        if (!near(contrastPixelColor(values, count), float3(0.0))) return 3;
        // A bright majority still wins: this is not minority-detail protection.
        values[0] = float3(0.0);
        for (uint i = 1u; i < count; i++) values[i] = float3(1.0);
        if (!near(contrastPixelColor(values, count), float3(1.0))) return 4;
        // Mixed chroma remains finite and display-gamut bounded.
        for (uint i = 0u; i < count; i++) values[i] = float3(float(i % 3u) / 2.0, float(i % 5u) / 4.0, float(i % 7u) / 6.0);
        float3 c = contrastPixelColor(values, count);
        if (any(isnan(c)) || any(isinf(c)) || any(c < 0.0) || any(c > 1.0)) return 5;
    }
    return 0;
}
