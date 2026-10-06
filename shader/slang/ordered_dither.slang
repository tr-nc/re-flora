module ordered_dither;

import color;

// Fixed Bayer 4x4 rank order from ImageMagick thresholds.xml o4x4,
// converted from 1..16 to 0..15. Use rank midpoints, not level/17.
// https://raw.githubusercontent.com/ImageMagick/ImageMagick/main/config/thresholds.xml
static const uint BAYER4[16] = {
    0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5
};

public float orderedThreshold(int2 cell)
{
    uint2 tile = uint2(cell) & uint2(3u);
    return (float(BAYER4[tile.y * 4u + tile.x]) + 0.5) / 16.0;
}

public bool orderedDitherEnabled(uint enabled, float strength)
{
    return enabled != 0u && isfinite(strength) && strength > 0.0;
}

float orderedFiniteNonnegative(float value)
{
    return isfinite(value) ? clamp(value, 0.0, 65504.0) : 0.0;
}

// One scalar chooses the neighboring brightness level for a complete color.
public float orderedDitherScalar(float value, int2 cell, uint levels)
{
    float safe = orderedFiniteNonnegative(value);
    float steps = float(clamp(levels, 2u, 32u) - 1u);
    float scaled = safe * steps;
    float lower = floor(scaled);
    return (lower + (frac(scaled) > orderedThreshold(cell) ? 1.0 : 0.0)) / steps;
}

// Used only after scene composition, tone mapping and pixel color resolve.
// Never splice/quantize channels independently; this is brightness styling,
// not an indexed palette or Contrast-aware source-color membership guarantee.
public float3 orderedDitherColor(float3 linearColor, int2 cell, uint enabled,
                                uint levels, float strength)
{
    // Preserve the exact original path before any conversion/sanitization.
    if (!orderedDitherEnabled(enabled, strength)) return linearColor;
    float3 safe = float3(orderedFiniteNonnegative(linearColor.r),
                        orderedFiniteNonnegative(linearColor.g),
                        orderedFiniteNonnegative(linearColor.b));
    float3 encoded = linearToSrgb(safe);
    float peak = max(encoded.r, max(encoded.g, encoded.b));
    if (peak <= 0.0) return float3(0.0);
    float quantizedPeak = orderedDitherScalar(peak, cell, levels);
    float targetPeak = lerp(peak, quantizedPeak, saturate(strength));
    float3 result = srgbToLinear(encoded * (targetPeak / peak));
    float maximum = max(result.r, max(result.g, result.b));
    return maximum > 65504.0 ? result * (65504.0 / maximum) : result;
}
