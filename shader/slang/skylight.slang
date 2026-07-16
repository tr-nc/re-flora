module skylight;

public struct SkyColors
{
    public float3 top_color;
    public float3 bottom_color;
};

SkyColors interpolateTimeKeyframes(float sunAltitude)
{
    const float3 top0 = float3(0.001214, 0.001518, 0.003035);
    const float3 bottom0 = float3(0.002428, 0.003035, 0.006995);
    const float3 top1 = float3(0.006049, 0.008023, 0.029557);
    const float3 bottom1 = float3(0.038204, 0.019382, 0.042311);
    const float3 top2 = float3(0.005182, 0.010330, 0.061246);
    const float3 bottom2 = float3(0.014444, 0.011612, 0.064803);
    const float3 top3 = float3(0.008023, 0.023153, 0.127438);
    const float3 bottom3 = float3(0.031896, 0.034340, 0.155926);
    const float3 top4 = float3(0.011612, 0.029557, 0.141263);
    const float3 bottom4 = float3(0.545724, 0.045186, 0.090842);
    const float3 top5 = float3(0.061246, 0.114435, 0.428690);
    const float3 bottom5 = float3(1.000000, 0.171441, 0.026241);
    const float3 top6 = float3(0.187821, 0.341914, 0.938686);
    const float3 bottom6 = float3(1.000000, 0.485150, 0.114435);
    const float3 top7 = float3(0.042311, 0.223228, 0.715694);
    const float3 bottom7 = float3(0.127438, 0.351533, 0.715694);
    const float3 top8 = float3(0.019382, 0.149960, 0.644480);
    const float3 bottom8 = float3(0.090842, 0.283149, 0.679542);
    const float3 top9 = float3(0.011612, 0.107023, 0.545724);
    const float3 bottom9 = float3(0.064803, 0.230740, 0.610496);
    const float3 top10 = float3(0.023153, 0.141263, 0.644480);
    const float3 bottom10 = float3(0.097587, 0.296138, 0.679542);

    SkyColors result;
    if (sunAltitude <= -0.40)
    {
        result.top_color = top0;
        result.bottom_color = bottom0;
    }
    else if (sunAltitude < -0.25)
    {
        float t = (sunAltitude + 0.40) / 0.15;
        result.top_color = lerp(top0, top1, t);
        result.bottom_color = lerp(bottom0, bottom1, t);
    }
    else if (sunAltitude < -0.15)
    {
        float t = (sunAltitude + 0.25) / 0.10;
        result.top_color = lerp(top1, top2, t);
        result.bottom_color = lerp(bottom1, bottom2, t);
    }
    else if (sunAltitude < -0.07)
    {
        float t = (sunAltitude + 0.15) / 0.08;
        result.top_color = lerp(top2, top3, t);
        result.bottom_color = lerp(bottom2, bottom3, t);
    }
    else if (sunAltitude < -0.03)
    {
        float t = (sunAltitude + 0.07) / 0.04;
        result.top_color = lerp(top3, top4, t);
        result.bottom_color = lerp(bottom3, bottom4, t);
    }
    else if (sunAltitude < 0.0)
    {
        float t = (sunAltitude + 0.03) / 0.03;
        result.top_color = lerp(top4, top5, t);
        result.bottom_color = lerp(bottom4, bottom5, t);
    }
    else if (sunAltitude < 0.1)
    {
        float t = sunAltitude / 0.1;
        result.top_color = lerp(top5, top6, t);
        result.bottom_color = lerp(bottom5, bottom6, t);
    }
    else if (sunAltitude < 0.3)
    {
        float t = (sunAltitude - 0.1) / 0.2;
        result.top_color = lerp(top6, top7, t);
        result.bottom_color = lerp(bottom6, bottom7, t);
    }
    else if (sunAltitude < 0.5)
    {
        float t = (sunAltitude - 0.3) / 0.2;
        result.top_color = lerp(top7, top8, t);
        result.bottom_color = lerp(bottom7, bottom8, t);
    }
    else if (sunAltitude < 0.8)
    {
        float t = (sunAltitude - 0.5) / 0.3;
        result.top_color = lerp(top8, top9, t);
        result.bottom_color = lerp(bottom8, bottom9, t);
    }
    else if (sunAltitude < 1.0)
    {
        float t = (sunAltitude - 0.8) / 0.2;
        result.top_color = lerp(top9, top10, t);
        result.bottom_color = lerp(bottom9, bottom10, t);
    }
    else
    {
        result.top_color = top10;
        result.bottom_color = bottom10;
    }
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
