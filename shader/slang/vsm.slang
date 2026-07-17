module vsm;

public float2 evsmExponents()
{
    return min(float2(16.0, 5.0), 42.0);
}

public float2 warpDepth(float depth, float2 exponents)
{
    depth = 2.0 * depth - 1.0;
    return float2(exp(exponents.x * depth), -exp(-exponents.y * depth));
}

public float chebyshevUpperBound(float2 moments, float value)
{
    float variance = max(moments.y - moments.x * moments.x, 1.0e-5);
    float difference = (value - moments.x) * 2.0;
    return value <= moments.x
        ? 1.0
        : variance / (variance + difference * difference);
}
