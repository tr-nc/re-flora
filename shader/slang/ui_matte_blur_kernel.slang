module ui_matte_blur_kernel;

// Dense physical-texel support: sigma changes the weights/radius, never the
// spacing. Sparse sigma-sized steps produce displaced copies of thin features.
public interface IMatteBlurLine
{
    float3 sample(int offset);
}

public int uiMatteBlurRadius(float sigma)
{
    return int(ceil(3.0 * sigma));
}

public float uiMatteBlurWeight(int offset, float sigma)
{
    float x = float(offset) / sigma;
    return exp(-0.5 * x * x);
}

// Shared by the GPU pass and CPU impulse/edge regressions.
public float3 uiMatteBlur<T : IMatteBlurLine>(T line, float sigma)
{
    int radius = uiMatteBlurRadius(sigma);
    float3 sum = float3(0.0);
    float total = 0.0;
    for (int offset = -radius; offset <= radius; ++offset)
    {
        float weight = uiMatteBlurWeight(offset, sigma);
        sum += line.sample(offset) * weight;
        total += weight;
    }
    return sum / total;
}
