module vsm_filtering;

public static const uint VSM_FILTER_MAX_RADIUS = 64u;

public struct VsmFilterPushConstants
{
    public uint blur_radius;
    public float temporal_alpha;
    public uint reset_history;
};

int filterRadius(uint blurRadius)
{
    return int(min(blurRadius, VSM_FILTER_MAX_RADIUS));
}

float gaussianWeight(float offset, float sigma)
{
    if (sigma <= 0.0)
    {
        return offset == 0.0 ? 1.0 : 0.0;
    }
    return exp(-(offset * offset) / (2.0 * sigma * sigma));
}

public float4 filterVsmHorizontal(
    readonly RWTexture2D<float4> pingTexture,
    int2 coordinate,
    uint blurRadius)
{
    int radius = filterRadius(blurRadius);
    if (radius <= 0)
    {
        return pingTexture[coordinate];
    }

    uint width;
    uint height;
    pingTexture.GetDimensions(width, height);
    float4 filtered = float4(0.0);
    float totalWeight = 0.0;
    float sigma = float(radius) / 2.0;
    for (int offset = -radius; offset <= radius; ++offset)
    {
        if (coordinate.x + offset < 0 ||
            coordinate.x + offset >= int(width))
        {
            continue;
        }
        float weight = gaussianWeight(float(offset), sigma);
        filtered += pingTexture[coordinate + int2(offset, 0)] * weight;
        totalWeight += weight;
    }
    return totalWeight > 0.0 ? filtered / totalWeight : filtered;
}

public float4 filterVsmVertical(
    readonly RWTexture2D<float4> pongTexture,
    int2 coordinate,
    uint blurRadius)
{
    int radius = filterRadius(blurRadius);
    if (radius <= 0)
    {
        return pongTexture[coordinate];
    }

    uint width;
    uint height;
    pongTexture.GetDimensions(width, height);
    float4 filtered = float4(0.0);
    float totalWeight = 0.0;
    float sigma = float(radius) / 2.0;
    for (int offset = -radius; offset <= radius; ++offset)
    {
        if (coordinate.y + offset < 0 ||
            coordinate.y + offset >= int(height))
        {
            continue;
        }
        float weight = gaussianWeight(float(offset), sigma);
        filtered += pongTexture[coordinate + int2(0, offset)] * weight;
        totalWeight += weight;
    }
    return totalWeight > 0.0 ? filtered / totalWeight : filtered;
}

public float4 blendVsmTemporalHistory(
    readonly RWTexture2D<float4> historyTexture,
    int2 coordinate,
    float4 currentFiltered,
    float temporalAlpha,
    uint resetHistory)
{
    if (resetHistory != 0u)
    {
        return currentFiltered;
    }
    float alpha = clamp(temporalAlpha, 0.0, 1.0);
    return lerp(historyTexture[coordinate], currentFiltered, alpha);
}
