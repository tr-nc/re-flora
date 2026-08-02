module skylight;

import sky_environment_data;

public struct SkyColors
{
    public float3 top_color;
    public float3 bottom_color;
};

SkyColors interpolateTimeKeyframes(float sunAltitude)
{
    SkyColors result;
    if (sunAltitude <= SKY_COLOR_ALTITUDES[0])
    {
        result.top_color = SKY_COLOR_TOP[0];
        result.bottom_color = SKY_COLOR_BOTTOM[0];
        return result;
    }

    [ForceUnroll]
    for (uint upper = 1u; upper < SKY_COLOR_KEYFRAME_COUNT; ++upper)
    {
        if (sunAltitude < SKY_COLOR_ALTITUDES[upper])
        {
            uint lower = upper - 1u;
            float t = (sunAltitude - SKY_COLOR_ALTITUDES[lower]) /
                      (SKY_COLOR_ALTITUDES[upper] - SKY_COLOR_ALTITUDES[lower]);
            result.top_color = lerp(SKY_COLOR_TOP[lower], SKY_COLOR_TOP[upper], t);
            result.bottom_color = lerp(
                SKY_COLOR_BOTTOM[lower], SKY_COLOR_BOTTOM[upper], t);
            return result;
        }
    }

    uint last = SKY_COLOR_KEYFRAME_COUNT - 1u;
    result.top_color = SKY_COLOR_TOP[last];
    result.bottom_color = SKY_COLOR_BOTTOM[last];
    return result;
}

float interpolateViewAltitude(float altitude)
{
    if (altitude <= -1.0) return 0.0;
    if (altitude < -0.15) return lerp(0.0, 0.03, (altitude + 1.0) / 0.85);
    if (altitude < 0.0) return lerp(0.03, 0.55, (altitude + 0.15) / 0.15);
    if (altitude < 0.08) return lerp(0.55, 0.72, altitude / 0.08);
    if (altitude < 0.2) return lerp(0.72, 0.86, (altitude - 0.08) / 0.12);
    if (altitude < 0.4) return lerp(0.86, 0.96, (altitude - 0.2) / 0.2);
    if (altitude < 1.0) return lerp(0.96, 1.0, (altitude - 0.4) / 0.6);
    return 1.0;
}

float henyeyGreensteinPhase(float cosine, float eccentricity)
{
    float eccentricitySquared = eccentricity * eccentricity;
    float base = 1.0 + eccentricitySquared - 2.0 * eccentricity * cosine;
    return (1.0 - eccentricitySquared) / (4.0 * 3.14159265 * base * sqrt(base));
}

public float3 getSkyColor(float3 viewDirection, float3 sunDirection)
{
    float altitude = viewDirection.y;
    float sunAltitude = sunDirection.y;
    SkyColors skyColors = interpolateTimeKeyframes(sunAltitude);

    float blendFactor = smoothstep(0.0, 1.0, interpolateViewAltitude(altitude));
    float3 baseSkyColor = lerp(skyColors.bottom_color, skyColors.top_color, blendFactor);

    float cosine = dot(viewDirection, sunDirection);
    float eccentricity = lerp(0.76, 0.82, clamp(1.0 - abs(sunAltitude), 0.0, 1.0));
    float haloPhase = henyeyGreensteinPhase(cosine, eccentricity);
    float eccentricitySquared = eccentricity * eccentricity;
    float oneMinusEccentricity = 1.0 - eccentricity;
    float peak = (1.0 - eccentricitySquared) /
                 (4.0 * 3.14159265 * oneMinusEccentricity * oneMinusEccentricity *
                  oneMinusEccentricity);
    float normalizedHalo = haloPhase / peak;
    float haloStrength = normalizedHalo *
                         lerp(0.35, 0.18, clamp(sunAltitude * 2.0, 0.0, 1.0));

    float sunBlendFactor = smoothstep(0.0, 1.0, interpolateViewAltitude(sunAltitude));
    float3 haloColor = lerp(skyColors.bottom_color, skyColors.top_color, sunBlendFactor);
    return lerp(baseSkyColor, haloColor, clamp(haloStrength, 0.0, 1.0));
}

// All environment-lighting transports consume this shared radiance definition.
// The pi calibration preserves the authored presentation while DDGI stores
// diffuse irradiance divided by pi and surface path tracing uses cosine-weighted
// importance sampling.
public float3 getAuthoredSkyRadiance(float3 viewDirection,
                                    float3 sunDirection)
{
    return getSkyColor(viewDirection, sunDirection) *
        3.14159265358979323846;
}

public float3 getSkyColorWithSun(float3 viewDirection, float3 sunDirection,
                                 float3 sunColor, float sunLuminance, float sunSize)
{
    float3 skyColor = getSkyColor(viewDirection, sunDirection);
    float sunDistance = (1.0 - dot(viewDirection, sunDirection)) / sunSize;
    float sun = 0.05 / max(sunDistance, 0.001) + 0.02;
    float3 sunContribution = float3(sun / 0.477, sun + 0.5, sun + 0.8);
    sunContribution *= sunLuminance * 0.2;
    float3 luminanceSunColor = sunColor * sunContribution;
    float sunBlendFactor = clamp(sun * 0.1, 0.0, 1.0);
    return lerp(skyColor, luminanceSunColor, sunBlendFactor);
}
