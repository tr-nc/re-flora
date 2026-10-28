import color;
import ui_matte_material;

export __extern_cpp int main()
{
    // Compare skip-ahead against the browser's sequential LCG for every texel.
    uint state = 1837u;
    for (uint i = 0u; i < 1024u; ++i)
    {
        state = state * 1664525u + 1013904223u;
        if (uiMatteGrainState(i) != state) return 1;
        float2 point = float2(i % 32u, i / 32u);
        float expectedGrain = ((state >> 24u) < 128u ? 90.0 : 220.0) / 255.0;
        if (abs(uiMatteGrain(point) - expectedGrain) > 0.000001) return 2;
        if (uiMatteGrain(point) != uiMatteGrain(point + float2(32.0))) return 3;
    }
    if (UI_MATTE_BODY != 0.18 ||
        UI_MATTE_DITHER != 0.09 || UI_MATTE_SATURATION != 0.5) return 4;

    // Source-over arithmetic from the approved browser recipe, not linear-space
    // tint blending. Uniform inputs also exercise bright/dark backdrop limits.
    float3 backdrops[3] = { float3(0.0), float3(1.0), float3(0.2, 0.5, 0.8) };
    for (uint i = 0u; i < 3u; ++i)
    {
        float3 source = backdrops[i];
        float gray = source.r * 0.213 + source.g * 0.715 + source.b * 0.072;
        float3 expected = (source * 0.5 + gray * 0.5) * 0.82 + float3(217.0, 228.0, 194.0) / 255.0 * 0.18;
        float alpha = 0.09 * 140.0 / 255.0;
        expected = expected * (1.0 - alpha) + (90.0 / 255.0) * alpha;
        float3 actual = uiMatteSurfaceSrgb(source, 90.0 / 255.0);
        if (any(abs(actual - expected) > float3(0.00001))) return 5;
        if (any(abs(linearToSrgb(srgbToLinear(actual)) - actual) > float3(0.00001))) return 6;
    }
    return 0;
}
