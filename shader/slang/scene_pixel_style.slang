// PixelOE-inspired luminance selection, not its chroma reconstruction or
// outline-expansion pipeline. Algorithm reference (Apache-2.0 project):
// https://github.com/KohakuBlueleaf/PixelOE/blob/7ce444b36d3876a151d845d4493240e904454d89/src/pixeloe/slang/shaders/downscale/contrast.slang
// Select an entire existing display-linear RGB sample. Independent Lab channel
// medians can splice foliage and sky into cyan that did not exist in the source.

public float pixelLightness(float3 rgb)
{
    // D65 CIELab L* depends only on linear RGB luminance (reference white Y=1).
    float y = dot(rgb, float3(0.2126729, 0.7151522, 0.0721750));
    float f = y > 0.00885645168 ? pow(y, 1.0 / 3.0) : (903.2962963 * y + 16.0) / 116.0;
    return 116.0 * f - 16.0;
}

// One 2x2 or 4x4 source block. Sort luminance statistics, never RGB components.
// Min/max ties retain the first source sample; fallback retains the row-major
// middle sample. Returning an untouched tuple prevents new hues and gamut clips.
public float3 contrastPixelColor(float3 colors[16], uint count)
{
    float ordered[16];
    float meanL = 0.0;
    uint lowIndex = 0u;
    uint highIndex = 0u;
    float low = pixelLightness(colors[0]);
    float high = low;
    for (uint i = 0u; i < count; i++)
    {
        float l = pixelLightness(colors[i]);
        ordered[i] = l;
        meanL += l;
        if (l < low) { low = l; lowIndex = i; }
        if (l > high) { high = l; highIndex = i; }
    }
    meanL /= float(count);
    for (uint i = 1u; i < count; i++)
        for (uint j = i; j > 0u; j--)
        {
            float a = ordered[j - 1u];
            float b = ordered[j];
            ordered[j - 1u] = min(a, b);
            ordered[j] = max(a, b);
        }
    float medianL = ordered[(count - 1u) / 2u];
    uint selected = count / 2u;
    if (medianL < meanL && high - medianL > medianL - low) selected = lowIndex;
    if (medianL > meanL && high - medianL < medianL - low) selected = highIndex;
    return colors[selected];
}
