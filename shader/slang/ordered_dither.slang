module ordered_dither;

import color;

public static const uint ORDERED_GLOBAL = 1u;
public static const uint ORDERED_GOD_RAYS = 2u;
public static const uint ORDERED_LENS_FLARE = 4u;
public static const uint ORDERED_SKY_BACKGROUND = 8u;
public static const uint ORDERED_TERRAIN_AMBIENT = 16u;

// Rank order from ImageMagick thresholds.xml o4x4 / h4x4o, converted
// from 1..16 to 0..15. Use rank midpoints, not ImageMagick's level/17.
// https://raw.githubusercontent.com/ImageMagick/ImageMagick/main/config/thresholds.xml
static const uint BAYER4[16] = {
    0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5
};
static const uint HALFTONE4[16] = {
    6, 12, 10, 3, 11, 15, 13, 7, 9, 14, 5, 1, 4, 8, 2, 0
};

public float orderedThreshold(int2 cell, uint pattern)
{
    uint2 tile = uint2(cell) & uint2(3u);
    uint index = tile.y * 4u + tile.x;
    uint rank = pattern == 1u ? HALFTONE4[index] : BAYER4[index];
    return (float(rank) + 0.5) / 16.0;
}

// options = flags, pattern, brightness levels, samples-per-axis.
// Global is the only quantizer when active; local preferences remain saved.
public bool orderedDitherEnabled(uint4 options, float strength, uint effect)
{
    return isfinite(strength) && strength > 0.0 &&
        (options.x & effect) != 0u &&
        (effect == ORDERED_GLOBAL || (options.x & ORDERED_GLOBAL) == 0u);
}

public int2 orderedSceneCell(float2 uv, uint2 pixelExtent)
{
    uint2 extent = max(pixelExtent, uint2(1u));
    return clamp(int2(floor(uv * float2(extent))), int2(0), int2(extent) - 1);
}

float orderedFiniteNonnegative(float value)
{
    // Local buffers are rgba16f; avoid non-finite conversion/overflow.
    return isfinite(value) ? clamp(value, 0.0, 65504.0) : 0.0;
}

// Uniform steps extend beyond display white for HDR contributions. Exactly on
// a level remains there. Strength blends the old and quantized values, so 0
// bypasses all conversion/sanitization; it is not a hidden posterize setting.
public float orderedDitherScalar(float value, int2 cell, uint4 options,
                                float strength, uint effect)
{
    if (!orderedDitherEnabled(options, strength, effect)) return value;
    float safe = orderedFiniteNonnegative(value);
    float steps = float(clamp(options.z, 2u, 32u) - 1u);
    float scaled = safe * steps;
    float lower = floor(scaled);
    float quantized = (lower + (frac(scaled) > orderedThreshold(cell, options.y) ? 1.0 : 0.0)) / steps;
    return lerp(safe, quantized, saturate(strength));
}

// One scalar controls one complete color tuple: never quantize/splice channels
// independently. This creates brightness variants, NOT an indexed palette and
// NOT the Contrast-aware guarantee of selecting an existing source sample.
public float3 orderedDitherColor(float3 linearColor, int2 cell, uint4 options,
                                float strength, uint effect)
{
    if (!orderedDitherEnabled(options, strength, effect)) return linearColor;
    float3 safe = float3(orderedFiniteNonnegative(linearColor.r),
                        orderedFiniteNonnegative(linearColor.g),
                        orderedFiniteNonnegative(linearColor.b));
    float3 encoded = linearToSrgb(safe);
    float peak = max(encoded.r, max(encoded.g, encoded.b));
    if (peak <= 0.0) return float3(0.0);
    float quantizedPeak = orderedDitherScalar(peak, cell, options, 1.0, effect);
    // Strength is applied in encoded space once (not twice).
    float targetPeak = lerp(peak, quantizedPeak, saturate(strength));
    float3 result = srgbToLinear(encoded * (targetPeak / peak));
    // Keep the color ray, including the rare FP16 upper boundary rounding up.
    float maximum = max(result.r, max(result.g, result.b));
    return maximum > 65504.0 ? result * (65504.0 / maximum) : result;
}
