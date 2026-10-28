import ui_matte_blur_kernel;
import ui_matte_material;

struct Impulse : IMatteBlurLine
{
    int position;
    float3 sample(int offset) { return float3(position + offset == 0 ? 1.0 : 0.0); }
}
struct Constant : IMatteBlurLine
{
    float3 sample(int offset) { return float3(0.2, 0.5, 0.8); }
}
struct Edge : IMatteBlurLine
{
    int position;
    float3 sample(int offset) { return float3(position + offset >= 0 ? 1.0 : 0.0); }
}

export __extern_cpp int main()
{
    // Real production accumulation at fractional, normal and high-DPI scales.
    float scales[4] = { 0.75, 1.0, 1.75, 2.0 };
    for (int s = 0; s < 4; ++s)
    {
        float sigma = 7.0 * scales[s];
        int radius = int(ceil(3.0 * sigma));
        float previous = 1.0;
        float mass = 0.0;
        float variance = 0.0;
        for (int x = 0; x <= radius + 1; ++x)
        {
            Impulse line = { x };
            Impulse mirror = { -x };
            float value = uiMatteBlur(line, sigma, radius).r;
            // A bright line must become one smooth lobe, never shifted replicas.
            if (value > previous + 0.000001) return 1;
            if (x <= radius && value <= 0.0) return 2;
            if (abs(value - uiMatteBlur(mirror, sigma, radius).r) > 0.000001) return 3;
            mass += value * (x == 0 ? 1.0 : 2.0);
            variance += 2.0 * value * float(x * x);
            previous = value;
        }
        if (abs(mass - 1.0) > 0.00001) return 4;
        // Truncation at three sigma removes at most a few percent of variance.
        if (abs(variance / (sigma * sigma) - 1.0) > 0.04) return 5;
        Constant constant;
        if (any(abs(uiMatteBlur(constant, sigma, radius) - float3(0.2, 0.5, 0.8)) > float3(0.00001))) return 6;
        previous = 0.0;
        for (int x = -radius - 1; x <= radius + 1; ++x)
        {
            Edge edge = { x };
            float value = uiMatteBlur(edge, sigma, radius).r;
            if (value < previous - 0.000001 || value < 0.0 || value > 1.000001) return 7;
            previous = value;
        }
    }
    return 0;
}
