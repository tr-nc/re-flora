import scene_pixel_style;
import tone_mapping;

bool near(float3 a, float3 b) { return all(abs(a - b) < 0.00004); }

bool isSourceColor(float3 result, float3 colors[16], uint count)
{
    for (uint i = 0u; i < count; i++)
        if (all(result == colors[i])) return true;
    return false;
}

export __extern_cpp int main()
{
    float3 values[16];
    for (uint i = 0u; i < 16u; i++) values[i] = float3(0.0);
    for (uint axis = 2u; axis <= 4u; axis++)
    {
        uint count = axis * axis;
        uint darkCount = (count + 1u) / 2u;
        // Uniform blocks preserve exact RGB tuples, including the lightness toe.
        for (uint level = 0u; level < 5u; level++)
        {
            float3 c = float3(0.0001, 0.07, 0.8) * (float(level) / 4.0);
            for (uint i = 0u; i < count; i++) values[i] = c;
            if (!near(contrastPixelColor(values, count), c)) return 1;
            if (!isSourceColor(contrastPixelColor(values, count), values, count)) return 2;
        }
        // Balanced dark/bright coverage chooses contrast instead of grey average.
        for (uint i = 0u; i < count; i++) values[i] = float3(i < darkCount ? 0.0 : 1.0);
        if (!near(contrastPixelColor(values, count), float3(0.0))) return 3;
        // A bright majority still wins: this is not minority-detail protection.
        values[0] = float3(0.0);
        for (uint i = 1u; i < count; i++) values[i] = float3(1.0);
        if (!near(contrastPixelColor(values, count), float3(1.0))) return 4;
        // Regression: green foliage plus blue sky must not synthesize cyan.
        // Both 2x2 and 4x4 balanced edge blocks reproduce the old Lab splice.
        float3 foliage = float3(0.05, 0.35, 0.02);
        float3 sky = float3(0.22, 0.48, 0.95);
        for (uint i = 0u; i < count; i++) values[i] = i < darkCount ? foliage : sky;
        float3 edge = contrastPixelColor(values, count);
        if (edge.r < 0.04 && edge.g > 0.15 && edge.b > 0.15) return 16;
        if (!isSourceColor(edge, values, count)) return 17;
        if (!near(edge, foliage)) return 18;
        // Exercise the actual caller's per-source HDR tone-map order.
        for (uint i = 0u; i < count; i++) values[i] = toneMapForDisplay(i < darkCount ? foliage * 3.0 : sky * 2.0);
        if (!isSourceColor(contrastPixelColor(values, count), values, count)) return 20;
        // All foliage/sky coverage patterns in 2x2; deterministic patterns in 4x4.
        // Legitimate blue sky is allowed: the invariant is source membership.
        for (uint pattern = 0u; pattern < 256u; pattern++)
        {
            uint bits = count == 4u ? pattern : pattern * 40503u;
            for (uint i = 0u; i < count; i++) values[i] = (bits & (1u << i)) != 0u ? foliage : sky;
            if (!isSourceColor(contrastPixelColor(values, count), values, count)) return 21;
        }
        // Genuine dark blue content must survive, not be removed by hue clamping.
        float3 blue = float3(0.01, 0.04, 0.4);
        for (uint i = 0u; i < count; i++) values[i] = i < darkCount ? blue : float3(0.4, 0.9, 0.05);
        if (!near(contrastPixelColor(values, count), blue)) return 22;
        // Mixed chroma remains finite and display-gamut bounded.
        for (uint i = 0u; i < count; i++) values[i] = float3(float(i % 3u) / 2.0, float(i % 5u) / 4.0, float(i % 7u) / 6.0);
        float3 c = contrastPixelColor(values, count);
        if (any(isnan(c)) || any(isinf(c)) || any(c < 0.0) || any(c > 1.0)) return 5;
        if (!isSourceColor(c, values, count)) return 19;
    }
    // Fixed-input property sweep: the resolve must never invent an RGB tuple.
    uint state = 7u;
    for (uint trial = 0u; trial < 256u; trial++)
    {
        uint axis = 2u + trial % 3u;
        uint count = axis * axis;
        for (uint i = 0u; i < count; i++)
        {
            state = state * 1664525u + 1013904223u;
            values[i] = float3(float(state & 255u), float((state >> 8u) & 255u), float((state >> 16u) & 255u)) / 255.0;
        }
        if (!isSourceColor(contrastPixelColor(values, count), values, count)) return 23;
    }
    if (abs(pixelLightness(float3(0.0))) > 0.00004) return 24;
    if (abs(pixelLightness(float3(1.0)) - 100.0) > 0.00004) return 25;
    return 0;
}
