module ui_matte_material;

// Approved re-flora/matte-glass-study/v1 recipe. CSS pixels become egui points.
public static const float3 UI_MATTE_TINT_SRGB = float3(217.0 / 255.0, 228.0 / 255.0, 194.0 / 255.0);
public static const float UI_MATTE_BODY = 0.18;
public static const float UI_MATTE_SATURATION = 0.5;
public static const float UI_MATTE_DITHER = 0.09;

// Exact seeded-monochrome-32x32-v1 LCG sequence from the browser. Affine
// exponentiation finds a texel in at most 11 steps, without a generated asset.
public uint uiMatteGrainState(uint index)
{
    uint remaining = index + 1u;
    uint multiplier = 1664525u;
    uint increment = 1013904223u;
    uint state = 1837u;
    while (remaining != 0u)
    {
        if ((remaining & 1u) != 0u) state = state * multiplier + increment;
        increment = (multiplier + 1u) * increment;
        multiplier *= multiplier;
        remaining >>= 1u;
    }
    return state;
}

public float uiMatteGrain(float2 localPoints)
{
    uint2 pixel = uint2(max(floor(localPoints), float2(0.0))) % 32u;
    uint state = uiMatteGrainState(pixel.y * 32u + pixel.x);
    return ((state >> 24u) < 128u ? 90.0 : 220.0) / 255.0;
}

public float3 uiMatteTintSrgb(float3 blurredSrgb)
{
    // CSS saturate() and source-over tint/grain operate in the encoded reference
    // space. The fragment converts the result back to linear for the game target.
    float luminance = dot(blurredSrgb, float3(0.213, 0.715, 0.072));
    float3 frosted = lerp(float3(luminance), blurredSrgb, UI_MATTE_SATURATION);
    return lerp(frosted, UI_MATTE_TINT_SRGB, UI_MATTE_BODY);
}

public float3 uiMatteSurfaceSrgb(float3 blurredSrgb, float grain)
{
    float3 body = uiMatteTintSrgb(blurredSrgb);
    return lerp(body, float3(grain), UI_MATTE_DITHER * (140.0 / 255.0));
}
