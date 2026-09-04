module tone_mapping;

static const float DISPLAY_TONE_MAP_EXPOSURE = 2.0;
static const float DISPLAY_TONE_MAP_SCENE_WHITE = 4.0;
static const float3 DISPLAY_LUMINANCE_WEIGHTS =
    float3(0.2126, 0.7152, 0.0722);

public float toneMapDisplayLuminance(float sceneLuminance)
{
    // Extended Reinhard keeps scene lighting linear while giving the display
    // transform an eye-like shoulder: shadow detail expands, and HDR sunlight
    // approaches scene white without a hard clip.
    float exposed = max(sceneLuminance, 0.0) * DISPLAY_TONE_MAP_EXPOSURE;
    float exposedWhite = DISPLAY_TONE_MAP_SCENE_WHITE *
        DISPLAY_TONE_MAP_EXPOSURE;
    float whiteSquared = exposedWhite * exposedWhite;
    return exposed * (1.0 + exposed / whiteSquared) / (1.0 + exposed);
}

public float3 toneMapForDisplay(float3 sceneColor)
{
    float3 nonnegativeColor = max(sceneColor, float3(0.0));
    float sceneLuminance = dot(nonnegativeColor, DISPLAY_LUMINANCE_WEIGHTS);
    if (sceneLuminance <= 0.0)
    {
        return float3(0.0);
    }

    float displayLuminance = toneMapDisplayLuminance(sceneLuminance);
    return saturate(nonnegativeColor * (displayLuminance / sceneLuminance));
}
