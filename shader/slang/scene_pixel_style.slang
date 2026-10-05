// PixelOE-inspired contrast-aware color resolve, not its outline-expansion pipeline.
// Algorithm reference (Apache-2.0 project):
// https://github.com/KohakuBlueleaf/PixelOE/blob/7ce444b36d3876a151d845d4493240e904454d89/src/pixeloe/slang/shaders/downscale/contrast.slang
// Own implementation over display-linear RGB; CIELab uses the D65 reference white.

float labCurve(float v)
{
    return v > 0.00885645168 ? pow(v, 1.0 / 3.0) : (903.2962963 * v + 16.0) / 116.0;
}

float labCurveInverse(float v)
{
    return v > 0.2068965517 ? v * v * v : (116.0 * v - 16.0) / 903.2962963;
}

public float3 pixelLinearToLab(float3 rgb)
{
    float3 xyz = float3(
        dot(rgb, float3(0.4124564, 0.3575761, 0.1804375)) / 0.95047,
        dot(rgb, float3(0.2126729, 0.7151522, 0.0721750)),
        dot(rgb, float3(0.0193339, 0.1191920, 0.9503041)) / 1.08883);
    float3 f = float3(labCurve(xyz.x), labCurve(xyz.y), labCurve(xyz.z));
    return float3(116.0 * f.y - 16.0, 500.0 * (f.x - f.y), 200.0 * (f.y - f.z));
}

public float3 pixelLabToLinear(float3 lab)
{
    float fy = (lab.x + 16.0) / 116.0;
    float3 xyz = float3(
        0.95047 * labCurveInverse(fy + lab.y / 500.0),
        labCurveInverse(fy),
        1.08883 * labCurveInverse(fy - lab.z / 200.0));
    return saturate(float3(
        dot(xyz, float3(3.2404542, -1.5371385, -0.4985314)),
        dot(xyz, float3(-0.9692660, 1.8760108, 0.0415560)),
        dot(xyz, float3(0.0556434, -0.2040259, 1.0572252))));
}

// Exactly one 2x2 or 4x4 source block. Components are sorted independently:
// lower-median chroma plus a contrast-selected luminance, not a global RGB mean.
public float3 contrastPixelColor(float3 colors[16], uint count)
{
    float3 ordered[16];
    float meanL = 0.0;
    float fallbackL = 0.0;
    for (uint i = 0u; i < count; i++)
    {
        ordered[i] = pixelLinearToLab(colors[i]);
        meanL += ordered[i].x;
        if (i == count / 2u) fallbackL = ordered[i].x;
    }
    meanL /= float(count);
    // Vector insertion network: independent L/a/b order statistics.
    for (uint i = 1u; i < count; i++)
        for (uint j = i; j > 0u; j--)
        {
            float3 a = ordered[j - 1u];
            float3 b = ordered[j];
            ordered[j - 1u] = min(a, b);
            ordered[j] = max(a, b);
        }
    float3 median = ordered[(count - 1u) / 2u];
    float low = ordered[0].x;
    float high = ordered[count - 1u].x;
    float l = fallbackL;
    if (median.x < meanL && high - median.x > median.x - low) l = low;
    if (median.x > meanL && high - median.x < median.x - low) l = high;
    return pixelLabToLinear(float3(l, median.yz));
}
